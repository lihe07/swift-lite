use deadpool_postgres::{Config as PgConfig, Pool, Runtime};
use serde_json::{json, Value};
use tokio_postgres::{NoTls, Row};

pub type Db = Pool;

/// Build a connection pool from a libpq-style URL.
pub fn make_pool(db_url: &str) -> anyhow::Result<Pool> {
    let pg: tokio_postgres::Config = db_url.parse()?;
    let mut cfg = PgConfig::new();
    cfg.host = pg.get_hosts().first().and_then(|h| match h {
        tokio_postgres::config::Host::Tcp(s) => Some(s.clone()),
        _ => None,
    });
    cfg.port = pg.get_ports().first().copied();
    cfg.user = pg.get_user().map(|s| s.to_string());
    cfg.password = pg
        .get_password()
        .map(|p| String::from_utf8_lossy(p).to_string());
    cfg.dbname = pg.get_dbname().map(|s| s.to_string());
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    Ok(pool)
}

/// CREATE TABLE IF NOT EXISTS for all three tables (identical to back/main.py).
pub async fn ensure_tables(pool: &Pool) -> anyhow::Result<()> {
    let c = pool.get().await?;
    c.batch_execute(
        "CREATE TABLE IF NOT EXISTS detections (id TEXT PRIMARY KEY, params TEXT, modified_at INTEGER, created_at INTEGER, num INTEGER, remark TEXT, status TEXT);
         CREATE TABLE IF NOT EXISTS workers (id TEXT PRIMARY KEY, name TEXT, remote_addr TEXT, connected_at INTEGER, last_ping INTEGER, tasks_done INTEGER, avg_det_time FLOAT);
         CREATE TABLE IF NOT EXISTS april_fools (id SERIAL PRIMARY KEY, created_at TIMESTAMP);",
    )
    .await?;
    Ok(())
}

/// Current unix time in whole seconds (matches int(time.time())).
/// Returned as i32 because the timestamp columns are Postgres INTEGER (int4),
/// and tokio-postgres binds parameter types strictly.
pub fn now() -> i32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i32
}

/// Convert a detections row (SELECT *) into the API JSON object, parsing `params`.
pub fn detection_row_to_json(row: &Row) -> Value {
    let params_str: Option<String> = row.try_get("params").ok().flatten();
    let params: Value = params_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    json!({
        "id": row.get::<_, String>("id"),
        "params": params,
        "modified_at": row.try_get::<_, Option<i32>>("modified_at").ok().flatten(),
        "created_at": row.try_get::<_, Option<i32>>("created_at").ok().flatten(),
        "num": row.try_get::<_, Option<i32>>("num").ok().flatten(),
        "remark": row.try_get::<_, Option<String>>("remark").ok().flatten(),
        "status": row.try_get::<_, Option<String>>("status").ok().flatten(),
    })
}

/// Port of _update_detection: set any provided fields plus modified_at = now().
pub async fn update_detection(
    pool: &Pool,
    id: &str,
    num: Option<i32>,
    status: Option<&str>,
    remark: Option<&str>,
    params: Option<&str>,
) -> anyhow::Result<()> {
    let c = pool.get().await?;
    let mut sets: Vec<String> = Vec::new();
    let mut vals: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
    let mut n = 1;
    if let Some(v) = num {
        sets.push(format!("num = ${n}"));
        vals.push(Box::new(v));
        n += 1;
    }
    if let Some(v) = status {
        sets.push(format!("status = ${n}"));
        vals.push(Box::new(v.to_string()));
        n += 1;
    }
    if let Some(v) = remark {
        sets.push(format!("remark = ${n}"));
        vals.push(Box::new(v.to_string()));
        n += 1;
    }
    if let Some(v) = params {
        sets.push(format!("params = ${n}"));
        vals.push(Box::new(v.to_string()));
        n += 1;
    }
    sets.push(format!("modified_at = ${n}"));
    vals.push(Box::new(now()));
    n += 1;
    let sql = format!("UPDATE detections SET {} WHERE id = ${n}", sets.join(", "));
    vals.push(Box::new(id.to_string()));
    let params_ref: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vals
        .iter()
        .map(|b| b.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
        .collect();
    c.execute(sql.as_str(), &params_ref).await?;
    Ok(())
}
