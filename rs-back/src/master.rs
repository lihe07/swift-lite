use crate::db::{self, Db};
use crate::proto;
use crate::task::PredictionTask;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

// The task queue is a multi-consumer (MPMC) channel: every connected worker holds
// a clone of the receiver and awaits `recv()` independently. This is deliberate —
// an earlier `Arc<Mutex<mpsc::Receiver>>` design serialized the workers, because a
// worker parked in `read_task` held the lock for up to 5s and starved the others,
// so their remote workers hit the 10s ping timeout and reconnected forever.
pub type TaskTx = async_channel::Sender<String>;
pub type TaskRx = async_channel::Receiver<String>;

/// A worker is "online" if its last_ping is within this many seconds of now.
pub const ONLINE_SECS: i32 = 60;
/// A worker row is purged after this many seconds without any ping (1 week).
pub const PURGE_SECS: i32 = 604_800;

/// last_ping cutoff (inclusive lower bound) for a worker to count as online.
pub fn online_cutoff(now: i32) -> i32 {
    now - ONLINE_SECS
}

/// last_ping cutoff (exclusive upper bound) below which a row is purged.
pub fn purge_cutoff(now: i32) -> i32 {
    now - PURGE_SECS
}

/// Bind the master TCP listener, retrying briefly so a transient `Address in use`
/// (e.g. racing a just-stopped predecessor for the port) self-heals without a
/// process restart. Returns the last error if every attempt fails.
async fn bind_listener(addr: &str, attempts: u32, delay: Duration) -> std::io::Result<TcpListener> {
    let mut last_err = None;
    for i in 0..attempts {
        match TcpListener::bind(addr).await {
            Ok(l) => return Ok(l),
            Err(e) => {
                tracing::warn!(
                    "master bind {addr} failed (attempt {}/{}): {e}",
                    i + 1,
                    attempts
                );
                last_err = Some(e);
                tokio::time::sleep(delay).await;
            }
        }
    }
    Err(last_err.expect("attempts >= 1"))
}

/// Spawn the listener, the expirer, and re-enqueue outstanding tasks.
pub async fn start(pool: Db, addr: &str, tx: TaskTx, rx: TaskRx) -> anyhow::Result<()> {
    // Re-enqueue outstanding work (startup requeue).
    {
        let c = pool.get().await?;
        let rows = c
            .query(
                "SELECT id FROM detections WHERE status = 'queue' OR status = 'processing'",
                &[],
            )
            .await?;
        for row in rows {
            let id: String = row.get("id");
            let _ = tx.try_send(id);
        }
    }

    // expire_workers loop
    {
        let pool = pool.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                if let Ok(c) = pool.get().await {
                    let _ = c
                        .execute(
                            "DELETE FROM workers WHERE last_ping < $1",
                            &[&purge_cutoff(db::now())],
                        )
                        .await;
                }
            }
        });
    }

    let listener = bind_listener(addr, 10, Duration::from_secs(1)).await?;
    tracing::info!("master accepting on {addr}");
    loop {
        let (sock, peer) = listener.accept().await?;
        let _ = sock.set_nodelay(true);
        let pool = pool.clone();
        let tx = tx.clone();
        let rx = rx.clone();
        tokio::spawn(async move {
            let mut w = Worker::new(sock, peer.ip().to_string(), pool, tx, rx);
            let id = w.id.clone();
            if let Err(e) = w.run().await {
                tracing::warn!("worker {id} ended: {e}");
            }
            w.cleanup().await;
        });
    }
}

// NOTE: the worker row is keyed by the worker-supplied name (`id == name`), so a
// worker's stats persist across reconnects. tasks_done/avg_det_time are NOT held
// in memory and synced — they are incremented atomically in SQL per finished task
// (see record_task_done). This is deliberate: with reconnect churn a worker can
// have brief overlapping connections, and syncing an in-memory counter let them
// clobber each other back to 0. Names are expected to be unique per worker.
struct Worker {
    sock: TcpStream,
    id: String,
    name: String,
    remote_addr: String,
    connected_at: i32,
    last_ping: i32,
    pool: Db,
    tx: TaskTx,
    rx: TaskRx,
}

impl Worker {
    fn new(sock: TcpStream, remote_addr: String, pool: Db, tx: TaskTx, rx: TaskRx) -> Self {
        let now = db::now();
        Worker {
            sock,
            id: uuid::Uuid::new_v4().to_string(),
            name: "worker".to_string(),
            remote_addr,
            connected_at: now,
            last_ping: now,
            pool,
            tx,
            rx,
        }
    }

    /// Send ping, expect a 50-byte reply beginning with b"pong\0". Updates name + last_ping.
    async fn ping(&mut self) -> bool {
        use tokio::io::AsyncReadExt;
        if proto::write_command(&mut self.sock, "ping").await.is_err() {
            return false;
        }
        let mut buf = [0u8; 50];
        match timeout(Duration::from_secs(3), self.sock.read_exact(&mut buf)).await {
            Ok(Ok(_)) => {}
            _ => return false,
        }
        if !buf.starts_with(b"pong\0") {
            return false;
        }
        self.last_ping = db::now();
        // name = bytes after "pong\0" up to trailing nulls
        let tail = &buf[5..];
        let name = String::from_utf8_lossy(tail)
            .trim_matches('\0')
            .trim()
            .to_string();
        if !name.is_empty() {
            self.name = name;
        }
        true
    }

    /// Heartbeat: ensure the worker row exists (keyed by name) and refresh
    /// liveness fields. Crucially, ON CONFLICT it updates ONLY name/last_ping/
    /// remote_addr — never connected_at/tasks_done/avg_det_time — so a reconnect
    /// (or an overlapping stale connection) cannot reset accumulated stats or the
    /// first-seen connection time. The row is created with zero stats on first sight.
    async fn sync_to_db(&self) -> anyhow::Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "INSERT INTO workers (id, name, connected_at, last_ping, tasks_done, remote_addr, avg_det_time)
             VALUES ($1,$2,$3,$4,0,$5,0)
             ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name,
               last_ping=EXCLUDED.last_ping, remote_addr=EXCLUDED.remote_addr",
            &[
                &self.id, &self.name, &self.connected_at, &self.last_ping, &self.remote_addr,
            ],
        )
        .await?;
        Ok(())
    }

    /// Atomically record one finished detection: bump tasks_done and fold det_time
    /// into the running average, entirely in SQL. Because the read-modify-write is
    /// a single statement, concurrent/overlapping connections for the same worker
    /// accumulate correctly instead of clobbering each other. The avg formula
    /// matches the Python original: avg' = (avg*n + det)/(n+1), evaluated with the
    /// pre-update n on both sides.
    async fn record_task_done(&self, det_time: f64) -> anyhow::Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "UPDATE workers
             SET avg_det_time = (avg_det_time * tasks_done + $1) / (tasks_done + 1),
                 tasks_done = tasks_done + 1,
                 last_ping = $2
             WHERE id = $3",
            &[&det_time, &db::now(), &self.id],
        )
        .await?;
        Ok(())
    }

    /// Pull next task id from the shared MPMC queue, waiting up to `secs`.
    /// Does not hold any lock across the wait, so workers never starve each other.
    async fn read_task(&self, secs: u64) -> Option<String> {
        match timeout(Duration::from_secs(secs), self.rx.recv()).await {
            Ok(Ok(id)) => Some(id),
            _ => None, // timed out, or channel closed
        }
    }

    /// Send a predict request and parse the JSON response. Returns None on any failure.
    async fn predict(
        &mut self,
        img_path: &str,
        query: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        // image (local file path -> "predict")
        if proto::write_command(&mut self.sock, "predict").await.is_err() {
            return None;
        }
        let img = tokio::fs::read(img_path).await.ok()?;
        if proto::write_framed(&mut self.sock, &img).await.is_err() {
            return None;
        }
        let query_bytes = serde_json::to_vec(query).ok()?;
        if proto::write_framed(&mut self.sock, &query_bytes).await.is_err() {
            return None;
        }
        if self.sock.write_u8(0).await.is_err() {
            return None;
        }
        // response: u32 size + body + trailing 0
        let size = timeout(Duration::from_secs(20), self.sock.read_u32())
            .await
            .ok()?
            .ok()?;
        let mut data = vec![0u8; size as usize];
        timeout(Duration::from_secs(10), self.sock.read_exact(&mut data))
            .await
            .ok()?
            .ok()?;
        let end = timeout(Duration::from_secs(10), self.sock.read_u8())
            .await
            .ok()?
            .ok()?;
        if end != 0 {
            return None;
        }
        let obj: serde_json::Value = serde_json::from_slice(&data).ok()?;
        // Stats are recorded by the caller via record_task_done (atomic SQL).
        Some(obj)
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        if !self.ping().await {
            anyhow::bail!("failed initial ping");
        }
        // Key the worker row by the worker-supplied name (stable across reconnects).
        self.id = self.name.clone();
        loop {
            self.sync_to_db().await.ok();
            let Some(task_id) = self.read_task(5).await else {
                if !self.ping().await {
                    anyhow::bail!("ping failed (idle)");
                }
                continue;
            };
            let Some(task) = PredictionTask::from_id(&self.pool, task_id.trim()).await? else {
                continue;
            };
            if !self.ping().await {
                task.set_status(&self.pool, "queue").await.ok();
                let _ = self.tx.try_send(task.id.clone());
                anyhow::bail!("ping failed before predict");
            }
            task.set_status(&self.pool, "processing").await.ok();

            // build query with worker-side overrides
            let mut p = task.params;
            if !p.tiling {
                p.window_size = 1.0;
                p.overlap = 0.0;
            }
            let query = json!({
                "window_size": p.window_size,
                "overlap": p.overlap,
                "threshold": 0.05,
                "iou": 0.95,
            });

            match self.predict(&task.image_url(), &query).await {
                Some(result) => {
                    let det_time = result["det_time"].as_f64().unwrap_or(0.0);
                    self.record_task_done(det_time).await.ok();
                    task.done(&self.pool, &result).await?;
                }
                None => {
                    task.set_status(&self.pool, "queue").await.ok();
                    let _ = self.tx.try_send(task.id.clone());
                    anyhow::bail!("predict failed");
                }
            }
        }
    }

    async fn cleanup(&self) {
        // Intentionally does NOT delete the worker row: stats persist across
        // reconnects and are only removed by the 1-week purge in `start`.
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn cutoffs() {
        assert_eq!(online_cutoff(1000), 1000 - 60);
        assert_eq!(purge_cutoff(1_000_000), 1_000_000 - 604_800);
    }

    // Regression test for the worker-starvation bug: two consumers sharing the
    // queue receiver must both be able to wait and receive concurrently. With the
    // old Arc<Mutex<mpsc>> design one consumer held the lock across its wait and
    // starved the other (so its remote worker hit the 10s ping timeout).
    #[tokio::test]
    async fn mpmc_receiver_does_not_starve_concurrent_consumers() {
        let (tx, rx) = async_channel::unbounded::<String>();
        let rx2 = rx.clone();
        let h1 = tokio::spawn(async move {
            timeout(Duration::from_secs(1), rx.recv()).await
        });
        let h2 = tokio::spawn(async move {
            timeout(Duration::from_secs(1), rx2.recv()).await
        });
        // let both consumers park on recv() before any message is sent
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.try_send("a".into()).unwrap();
        tx.try_send("b".into()).unwrap();
        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();
        assert!(r1.is_ok() && r1.unwrap().is_ok(), "consumer 1 did not receive");
        assert!(r2.is_ok() && r2.unwrap().is_ok(), "consumer 2 did not receive");
    }

    #[tokio::test]
    async fn bind_listener_succeeds_on_free_port() {
        let l = bind_listener("127.0.0.1:0", 3, Duration::from_millis(10))
            .await
            .expect("should bind a free ephemeral port");
        drop(l);
    }

    #[tokio::test]
    async fn bind_listener_errors_when_port_stays_held() {
        let held = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = held.local_addr().unwrap().to_string();
        // port is held for the whole (short) retry window -> all attempts fail
        let r = bind_listener(&addr, 3, Duration::from_millis(10)).await;
        assert!(r.is_err());
    }
}
