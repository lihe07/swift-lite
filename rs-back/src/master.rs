use crate::db::{self, Db};
use crate::proto;
use crate::task::PredictionTask;
use serde_json::json;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};

pub type TaskTx = mpsc::UnboundedSender<String>;
pub type TaskRx = Arc<Mutex<mpsc::UnboundedReceiver<String>>>;

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

/// Seed (tasks_done, avg_det_time, connected_at) for a connecting worker.
/// If a prior row exists, accumulate from it (preserving first-seen connected_at);
/// otherwise start fresh anchored at `now`.
pub fn seed_identity(existing: Option<(i64, f64, i32)>, now: i32) -> (i64, f64, i32) {
    match existing {
        Some((tasks_done, avg, connected_at)) => (tasks_done, avg, connected_at),
        None => (0, 0.0, now),
    }
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
            let _ = tx.send(id);
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

    let listener = TcpListener::bind(addr).await?;
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

// NOTE: the worker row is keyed by the worker-supplied name (`id == name`).
// Two simultaneous connections sharing a name will share/clobber one row; names
// are expected to be unique per worker, so this is accepted.
struct Worker {
    sock: TcpStream,
    id: String,
    name: String,
    remote_addr: String,
    connected_at: i32,
    last_ping: i32,
    tasks_done: i64,
    avg_det_time: f64,
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
            tasks_done: 0,
            avg_det_time: 0.0,
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

    /// After the first ping (name known), key the row by name and seed stats
    /// from any existing row so they accumulate across reconnects.
    async fn load_identity(&mut self) -> anyhow::Result<()> {
        self.id = self.name.clone();
        let existing = {
            let c = self.pool.get().await?;
            let rows = c
                .query(
                    "SELECT tasks_done, avg_det_time, connected_at FROM workers WHERE id = $1",
                    &[&self.id],
                )
                .await?;
            rows.first().map(|r| {
                (
                    r.try_get::<_, Option<i32>>("tasks_done")
                        .ok()
                        .flatten()
                        .unwrap_or(0) as i64,
                    r.try_get::<_, Option<f64>>("avg_det_time")
                        .ok()
                        .flatten()
                        .unwrap_or(0.0),
                    r.try_get::<_, Option<i32>>("connected_at")
                        .ok()
                        .flatten()
                        .unwrap_or(self.connected_at),
                )
            })
        };
        let (tasks_done, avg, connected_at) = seed_identity(existing, self.connected_at);
        self.tasks_done = tasks_done;
        self.avg_det_time = avg;
        self.connected_at = connected_at;
        Ok(())
    }

    /// Upsert the worker stats row.
    async fn sync_to_db(&self) -> anyhow::Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "INSERT INTO workers (id, name, connected_at, last_ping, tasks_done, remote_addr, avg_det_time)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (id) DO UPDATE SET name=EXCLUDED.name, connected_at=EXCLUDED.connected_at,
               last_ping=EXCLUDED.last_ping, tasks_done=EXCLUDED.tasks_done,
               remote_addr=EXCLUDED.remote_addr, avg_det_time=EXCLUDED.avg_det_time",
            &[
                &self.id, &self.name, &self.connected_at, &self.last_ping,
                &(self.tasks_done as i32), &self.remote_addr, &self.avg_det_time,
            ],
        )
        .await?;
        Ok(())
    }

    /// Pull next task id from the shared queue, waiting up to `secs`.
    async fn read_task(&self, secs: u64) -> Option<String> {
        let mut rx = self.rx.lock().await;
        match timeout(Duration::from_secs(secs), rx.recv()).await {
            Ok(Some(id)) => Some(id),
            _ => None,
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
        let det_time = obj["det_time"].as_f64().unwrap_or(0.0);
        self.avg_det_time = (self.avg_det_time * self.tasks_done as f64 + det_time)
            / (self.tasks_done as f64 + 1.0);
        self.tasks_done += 1;
        Some(obj)
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        if !self.ping().await {
            anyhow::bail!("failed initial ping");
        }
        self.load_identity().await?;
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
                let _ = self.tx.send(task.id.clone());
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
                    task.done(&self.pool, &result).await?;
                }
                None => {
                    task.set_status(&self.pool, "queue").await.ok();
                    let _ = self.tx.send(task.id.clone());
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

    #[test]
    fn seed_fresh_when_no_prior_row() {
        assert_eq!(seed_identity(None, 1234), (0, 0.0, 1234));
    }

    #[test]
    fn seed_accumulates_and_preserves_first_connection() {
        // prior row: 7 tasks, avg 1.5s, first connected at t=500; reconnecting at t=9000
        let (td, avg, ca) = seed_identity(Some((7, 1.5, 500)), 9000);
        assert_eq!(td, 7);
        assert_eq!(avg, 1.5);
        assert_eq!(ca, 500); // first-seen preserved, NOT reset to 9000
    }
}
