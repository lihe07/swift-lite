# Rust Backend Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reimplement the Python backend (`back/`) as a single Rust binary (`rs-back/`) with byte-for-byte compatibility on the HTTP API, the TCP master protocol, the PostgreSQL schema, and the on-disk detection artifacts.

**Architecture:** One tokio process runs an axum HTTP server for `/api`, a TCP listener implementing the master protocol (one task per worker), and a background worker-expirer. An in-process `tokio::mpsc` channel replaces the Python `multiprocessing.Queue`. Postgres access goes through `deadpool-postgres`; image drawing uses pure-Rust `image` + `imageproc`.

**Tech Stack:** Rust (edition 2021), tokio, axum 0.7 (multipart), tokio-postgres + deadpool-postgres, serde/serde_json, image 0.25, imageproc 0.25, ab_glyph, uuid, anyhow, thiserror, tracing.

---

## Reference: the code being ported

- `back/main.py` — HTTP routes + startup requeue + table creation.
- `back/master.py` — TCP master, `Worker` loop, `MasterProcess`, `expire_workers`.
- `back/common.py` — `make_conn`, `_update_detection`, `nms`, `PredictionTask`.
- `back/config.py` — `DB`, `BASE`, `MASTER`.
- `rs-worker/src/net.rs` — the **client** side of the protocol (do not modify; it defines the wire format we must match). Response JSON shape: `{boxes:[[x1,y1,x2,y2,score]], windows_lt:[[i32,i32]], window_size:[u32,u32], window_num:[usize,usize], det_time:f64, transfer_time:f64}`.

All paths below are relative to the repo root `/home/lihe07/Disk1/codes/swift-lite`.

---

## File Structure

```
rs-back/
  Cargo.toml
  assets/font.ttf              # bundled sans font for score labels
  src/
    main.rs                    # bootstrap + wiring
    config.rs                  # Config::from_env with defaults
    db.rs                      # Pool, ensure_tables, row->json helpers, update_detection
    error.rs                   # AppError -> HTTP response mapping
    nms.rs                     # nms()
    params.rs                  # Params struct + validate()
    proto.rs                   # frame read/write helpers + transforms
    task.rs                    # PredictionTask: from_id, image_url, set_status, done, nms_only
    draw.rs                    # draw_boxes / draw_windows
    master.rs                  # listener + Worker loop + expire_workers
    routes.rs                  # axum router + all handlers
```

---

## Task 1: Scaffold the crate

**Files:**
- Create: `rs-back/Cargo.toml`
- Create: `rs-back/src/main.rs`
- Create: `rs-back/assets/font.ttf` (copied from a system font)
- Create: `rs-back/.gitignore`

- [ ] **Step 1: Write `Cargo.toml`**

```toml
[package]
name = "rs-back"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
axum = { version = "0.7", features = ["multipart"] }
tower = "0.5"
tokio-postgres = "0.7"
deadpool-postgres = "0.14"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
image = "0.25"
imageproc = "0.25"
ab_glyph = "0.2"
uuid = { version = "1", features = ["v4"] }
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
```

- [ ] **Step 2: Bundle a font**

Run:
```bash
mkdir -p rs-back/assets
cp /usr/share/fonts/TTF/DejaVuSans.ttf rs-back/assets/font.ttf
```
Expected: `rs-back/assets/font.ttf` exists (≈760 KB).

- [ ] **Step 3: Write a placeholder `main.rs` so the crate compiles**

```rust
fn main() {
    println!("rs-back");
}
```

- [ ] **Step 4: Write `.gitignore`**

```
/target
```

- [ ] **Step 5: Verify it builds**

Run: `cd rs-back && cargo build`
Expected: compiles (dependencies download), prints no errors.

- [ ] **Step 6: Commit**

```bash
git add rs-back/Cargo.toml rs-back/Cargo.lock rs-back/src/main.rs rs-back/assets/font.ttf rs-back/.gitignore
git commit -m "chore: scaffold rs-back crate"
```

---

## Task 2: NMS module

Port `back/common.py:nms`. Input boxes are `[x1, y1, x2, y2, score]`. Filter `score > threshold` (strict), sort by score descending, greedily keep the top box and suppress any remaining box with `IoU > iou`. `area = (y2-y1)*(x2-x1)`.

**Files:**
- Create: `rs-back/src/nms.rs`
- Modify: `rs-back/src/main.rs` (add `mod nms;`)

- [ ] **Step 1: Write the failing tests**

In `rs-back/src/nms.rs`:
```rust
/// A box as [x1, y1, x2, y2, score].
pub type Box5 = [f32; 5];

/// Non-maximum suppression. Port of back/common.py:nms.
/// Filters score > threshold (strict), then suppresses boxes with IoU > iou.
pub fn nms(boxes: &[Box5], threshold: f32, iou: f32) -> Vec<Box5> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_below_threshold() {
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [0.0, 0.0, 10.0, 10.0, 0.2],
        ];
        // second box has score 0.2 which is not > 0.3
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][4], 0.9);
    }

    #[test]
    fn suppresses_high_overlap() {
        // two near-identical boxes -> IoU ~1.0 > 0.5 -> keep highest score only
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [0.0, 0.0, 10.0, 10.0, 0.8],
        ];
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][4], 0.9);
    }

    #[test]
    fn keeps_disjoint_boxes() {
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [100.0, 100.0, 110.0, 110.0, 0.8],
        ];
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn keeps_partial_overlap_below_iou() {
        // overlap area 25, union 175 -> IoU ~0.143 <= 0.5 -> keep both
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.9],
            [5.0, 5.0, 15.0, 15.0, 0.8],
        ];
        let out = nms(&boxes, 0.3, 0.5);
        assert_eq!(out.len(), 2);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd rs-back && cargo test nms`
Expected: FAIL — `unimplemented!()` panics.

- [ ] **Step 3: Implement `nms`**

Replace the `unimplemented!()` body:
```rust
pub fn nms(boxes: &[Box5], threshold: f32, iou: f32) -> Vec<Box5> {
    // Filter by score > threshold (strict, matching numpy boolean mask).
    let mut idx: Vec<usize> = (0..boxes.len())
        .filter(|&i| boxes[i][4] > threshold)
        .collect();

    // Sort indices by score descending.
    idx.sort_by(|&a, &b| {
        boxes[b][4]
            .partial_cmp(&boxes[a][4])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let area = |b: &Box5| (b[3] - b[1]) * (b[2] - b[0]);

    let mut keep = Vec::new();
    while !idx.is_empty() {
        let i = idx[0];
        keep.push(boxes[i]);
        let bi = &boxes[i];
        let ai = area(bi);
        idx = idx[1..]
            .iter()
            .copied()
            .filter(|&j| {
                let bj = &boxes[j];
                let xx1 = bi[0].max(bj[0]);
                let yy1 = bi[1].max(bj[1]);
                let xx2 = bi[2].min(bj[2]);
                let yy2 = bi[3].min(bj[3]);
                let w = (xx2 - xx1).max(0.0);
                let h = (yy2 - yy1).max(0.0);
                let inter = w * h;
                let ovr = inter / (ai + area(bj) - inter);
                // numpy keeps where ovr <= iou
                ovr <= iou
            })
            .collect();
    }
    keep
}
```

- [ ] **Step 4: Add module to `main.rs`**

At the top of `rs-back/src/main.rs`:
```rust
mod nms;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd rs-back && cargo test nms`
Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add rs-back/src/nms.rs rs-back/src/main.rs
git commit -m "feat: port NMS to rs-back"
```

---

## Task 3: Params struct + validation

Port the validation in `back/main.py:modify_detection`. The five params: `tiling: bool`, `window_size: f64` in `(0,1]`, `overlap: f64` in `[0,1)`, `threshold: f64` in `[0,1]`, `iou: f64` in `[0,1]`. Default params for a new detection: `tiling=true, window_size=0.3, overlap=0.1, threshold=0.3, iou=0.5`.

**Files:**
- Create: `rs-back/src/params.rs`
- Modify: `rs-back/src/main.rs` (add `mod params;`)

- [ ] **Step 1: Write the failing tests**

In `rs-back/src/params.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Params {
    pub tiling: bool,
    pub window_size: f64,
    pub overlap: f64,
    pub threshold: f64,
    pub iou: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params { tiling: true, window_size: 0.3, overlap: 0.1, threshold: 0.3, iou: 0.5 }
    }
}

impl Params {
    /// Returns Err(message) matching the Python error strings, or Ok(()).
    pub fn validate(&self) -> Result<(), String> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> Params { Params::default() }

    #[test]
    fn default_is_valid() {
        assert!(ok().validate().is_ok());
    }

    #[test]
    fn window_size_must_be_in_half_open() {
        let mut p = ok(); p.window_size = 0.0;
        assert_eq!(p.validate().unwrap_err(), "window_size should be within (0, 1]");
        let mut p = ok(); p.window_size = 1.0; // upper bound allowed
        assert!(p.validate().is_ok());
        let mut p = ok(); p.window_size = 1.1;
        assert!(p.validate().is_err());
    }

    #[test]
    fn overlap_excludes_one() {
        let mut p = ok(); p.overlap = 0.0; // lower bound allowed
        assert!(p.validate().is_ok());
        let mut p = ok(); p.overlap = 1.0;
        assert_eq!(p.validate().unwrap_err(), "overlap should be within [0, 1)");
    }

    #[test]
    fn threshold_and_iou_inclusive() {
        let mut p = ok(); p.threshold = 0.0; assert!(p.validate().is_ok());
        let mut p = ok(); p.threshold = 1.0; assert!(p.validate().is_ok());
        let mut p = ok(); p.threshold = 1.01;
        assert_eq!(p.validate().unwrap_err(), "threshold should be within [0, 1]");
        let mut p = ok(); p.iou = -0.1;
        assert_eq!(p.validate().unwrap_err(), "iou should be within [0, 1]");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd rs-back && cargo test params`
Expected: FAIL — `unimplemented!()`.

- [ ] **Step 3: Implement `validate`**

```rust
    pub fn validate(&self) -> Result<(), String> {
        if !(self.window_size > 0.0 && self.window_size <= 1.0) {
            return Err("window_size should be within (0, 1]".into());
        }
        if !(self.overlap >= 0.0 && self.overlap < 1.0) {
            return Err("overlap should be within [0, 1)".into());
        }
        if !(self.threshold >= 0.0 && self.threshold <= 1.0) {
            return Err("threshold should be within [0, 1]".into());
        }
        if !(self.iou >= 0.0 && self.iou <= 1.0) {
            return Err("iou should be within [0, 1]".into());
        }
        Ok(())
    }
```

Note: the `tiling` type check and "Missing {k}" errors are handled at the HTTP layer (Task 9) via serde deserialization, which rejects wrong types / missing fields before `validate` runs.

- [ ] **Step 4: Add module to `main.rs`**

```rust
mod params;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd rs-back && cargo test params`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add rs-back/src/params.rs rs-back/src/main.rs
git commit -m "feat: add Params struct and validation"
```

---

## Task 4: Config

Port `back/config.py`. Defaults match today; each value overridable by env var of the same name.

**Files:**
- Create: `rs-back/src/config.rs`
- Modify: `rs-back/src/main.rs` (add `mod config;`)

- [ ] **Step 1: Write the failing test**

In `rs-back/src/config.rs`:
```rust
#[derive(Debug, Clone)]
pub struct Config {
    pub db: String,
    pub base: String,
    pub master: String,
    pub http_port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_unset() {
        // Clear env to assert defaults (single-threaded test).
        std::env::remove_var("DB");
        std::env::remove_var("BASE");
        std::env::remove_var("MASTER");
        std::env::remove_var("HTTP_PORT");
        let c = Config::from_env();
        assert_eq!(c.db, "postgresql://swift:swift@localhost:5432/swift");
        assert_eq!(c.base, "http://back.bwrrc.org.cn");
        assert_eq!(c.master, "0.0.0.0:12345");
        assert_eq!(c.http_port, 20000);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd rs-back && cargo test config`
Expected: FAIL — `unimplemented!()`.

- [ ] **Step 3: Implement `from_env`**

```rust
impl Config {
    pub fn from_env() -> Self {
        fn var(key: &str, default: &str) -> String {
            std::env::var(key).unwrap_or_else(|_| default.to_string())
        }
        Config {
            db: var("DB", "postgresql://swift:swift@localhost:5432/swift"),
            base: var("BASE", "http://back.bwrrc.org.cn"),
            master: var("MASTER", "0.0.0.0:12345"),
            http_port: var("HTTP_PORT", "20000").parse().unwrap_or(20000),
        }
    }
}
```

- [ ] **Step 4: Add module to `main.rs`**

```rust
mod config;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd rs-back && cargo test config -- --test-threads=1`
Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add rs-back/src/config.rs rs-back/src/main.rs
git commit -m "feat: add config with env overrides"
```

---

## Task 5: Proto framing helpers

Port the wire framing used by both `back/master.py:Worker.predict/ping` and `rs-worker/src/net.rs`. All length prefixes are big-endian u32 (`struct.pack("!I", n)`). Commands are ASCII followed by a `\0`.

**Files:**
- Create: `rs-back/src/proto.rs`
- Modify: `rs-back/src/main.rs` (add `mod proto;`)

- [ ] **Step 1: Write the failing tests**

In `rs-back/src/proto.rs`:
```rust
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

/// Write a command token followed by a null terminator, e.g. b"predict" -> b"predict\0".
pub async fn write_command<W: AsyncWriteExt + Unpin>(w: &mut W, cmd: &str) -> io::Result<()> {
    w.write_all(cmd.as_bytes()).await?;
    w.write_u8(0).await
}

/// Write a big-endian u32 length prefix followed by the payload bytes.
pub async fn write_framed<W: AsyncWriteExt + Unpin>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    w.write_u32(payload.len() as u32).await?;
    w.write_all(payload).await
}

/// Read a big-endian u32 length prefix, then that many bytes.
pub async fn read_framed<R: AsyncReadExt + Unpin>(r: &mut R) -> io::Result<Vec<u8>> {
    let len = r.read_u32().await?;
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn command_has_null_terminator() {
        let mut buf = Vec::new();
        write_command(&mut buf, "predict").await.unwrap();
        assert_eq!(buf, b"predict\0");
    }

    #[tokio::test]
    async fn framed_roundtrip() {
        let mut buf = Vec::new();
        write_framed(&mut buf, b"hello").await.unwrap();
        // 4-byte big-endian length (5) + payload
        assert_eq!(&buf[0..4], &[0, 0, 0, 5]);
        let mut cur = Cursor::new(buf[..].to_vec());
        // skip nothing; read_framed reads from start
        let mut cur2 = Cursor::new({
            let mut v = Vec::new();
            write_framed(&mut v, b"hello").await.unwrap();
            v
        });
        let out = read_framed(&mut cur2).await.unwrap();
        assert_eq!(out, b"hello");
        let _ = &mut cur;
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd rs-back && cargo test proto`
Expected: FAIL — module `proto` not yet declared in `main.rs` (compile error) until Step 4. To see a clean fail first, do Step 4 then re-run; expected initial state is a compile error referencing `proto`.

- [ ] **Step 3: (implementation already written in Step 1)**

The functions above are the implementation; no `unimplemented!()` here because they are trivial framing helpers and the test value comes from asserting the exact byte layout.

- [ ] **Step 4: Add module to `main.rs`**

```rust
mod proto;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd rs-back && cargo test proto`
Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add rs-back/src/proto.rs rs-back/src/main.rs
git commit -m "feat: add proto framing helpers"
```

---

## Task 6: Error type + HTTP mapping

A single `AppError` that converts into an axum response. JSON error bodies and status codes match Python: `400`/`404` carry `{"error": "..."}`; unexpected errors → `500`.

**Files:**
- Create: `rs-back/src/error.rs`
- Modify: `rs-back/src/main.rs` (add `mod error;`)

- [ ] **Step 1: Write the failing test**

In `rs-back/src/error.rs`:
```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    /// 400 with {"error": msg}
    BadRequest(String),
    /// 404 with {"error": "Not Found"}
    NotFound,
    /// 500 (logged); body {"error":"Internal Server Error"}
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not Found".to_string()),
            AppError::Internal(m) => {
                tracing::error!("internal error: {m}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error".to_string())
            }
        };
        (status, Json(json!({"error": msg}))).into_response()
    }
}

/// Map anyhow/std errors to a 500.
impl<E: std::fmt::Display> From<E> for AppError
where
    E: std::error::Error,
{
    fn from(e: E) -> Self {
        AppError::Internal(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn not_found_is_404() {
        let resp = AppError::NotFound.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn bad_request_is_400() {
        let resp = AppError::BadRequest("x".into()).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd rs-back && cargo test error`
Expected: compile error until Step 3 adds `mod error;`.

- [ ] **Step 3: Add module to `main.rs`**

```rust
mod error;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd rs-back && cargo test error`
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add rs-back/src/error.rs rs-back/src/main.rs
git commit -m "feat: add AppError HTTP mapping"
```

---

## Task 7: DB layer

Port `make_conn`, table creation (`back/main.py` top), `_update_detection`, and the detection/worker queries. Uses `deadpool-postgres`. Detection rows are converted to a `serde_json::Value` object with `params` parsed from its stored JSON string and (optionally) a `queue` field added by the caller.

**Files:**
- Create: `rs-back/src/db.rs`
- Modify: `rs-back/src/main.rs` (add `mod db;`)

- [ ] **Step 1: Write `db.rs`** (no unit test — exercised by integration tests in Task 12; this is glue over the driver)

```rust
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
pub fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
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
    if let Some(v) = num { sets.push(format!("num = ${n}")); vals.push(Box::new(v)); n += 1; }
    if let Some(v) = status { sets.push(format!("status = ${n}")); vals.push(Box::new(v.to_string())); n += 1; }
    if let Some(v) = remark { sets.push(format!("remark = ${n}")); vals.push(Box::new(v.to_string())); n += 1; }
    if let Some(v) = params { sets.push(format!("params = ${n}")); vals.push(Box::new(v.to_string())); n += 1; }
    sets.push(format!("modified_at = ${n}")); vals.push(Box::new(now())); n += 1;
    let sql = format!("UPDATE detections SET {} WHERE id = ${n}", sets.join(", "));
    vals.push(Box::new(id.to_string()));
    let params_ref: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        vals.iter().map(|b| b.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
    c.execute(sql.as_str(), &params_ref).await?;
    Ok(())
}
```

- [ ] **Step 2: Add module to `main.rs`**

```rust
mod db;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd rs-back && cargo build`
Expected: compiles. (Behavior verified by integration tests in Task 12.)

- [ ] **Step 4: Commit**

```bash
git add rs-back/src/db.rs rs-back/src/main.rs
git commit -m "feat: add DB pool, table setup, and update_detection"
```

---

## Task 8: Drawing module

Port the OpenCV drawing in `back/common.py:done`. Boxes are drawn as green rectangles (2px) with the score text `{score:.2f}` at the top-left corner. Windows are blue rectangles. The windows image is drawn **on top of** the already-boxed image.

**Files:**
- Create: `rs-back/src/draw.rs`
- Modify: `rs-back/src/main.rs` (add `mod draw;`)

- [ ] **Step 1: Write the failing test**

In `rs-back/src/draw.rs`:
```rust
use ab_glyph::FontRef;
use image::{Rgb, RgbImage};
use imageproc::drawing::{draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

const GREEN: Rgb<u8> = Rgb([0, 255, 0]);
const BLUE: Rgb<u8> = Rgb([0, 0, 255]); // OpenCV BGR (255,0,0) == RGB blue

static FONT_BYTES: &[u8] = include_bytes!("../assets/font.ttf");

fn font() -> FontRef<'static> {
    FontRef::try_from_slice(FONT_BYTES).expect("bundled font is valid")
}

/// Draw a 2px-thick rectangle from (x1,y1) to (x2,y2).
fn rect_2px(img: &mut RgbImage, x1: i32, y1: i32, x2: i32, y2: i32, color: Rgb<u8>) {
    let w = (x2 - x1).max(0) as u32;
    let h = (y2 - y1).max(0) as u32;
    for t in 0..2i32 {
        // inset each successive pass by one pixel to approximate 2px thickness
        let rx = x1 + t;
        let ry = y1 + t;
        let rw = w.saturating_sub((2 * t) as u32);
        let rh = h.saturating_sub((2 * t) as u32);
        if rw == 0 || rh == 0 { continue; }
        draw_hollow_rect_mut(img, Rect::at(rx, ry).of_size(rw, rh), color);
    }
}

/// Draw detection boxes (green) with score labels. boxes: [x1,y1,x2,y2,score].
pub fn draw_boxes(img: &mut RgbImage, boxes: &[[f32; 5]]) {
    let font = font();
    let scale = ab_glyph::PxScale::from(18.0);
    for b in boxes {
        let (x1, y1, x2, y2) = (b[0] as i32, b[1] as i32, b[2] as i32, b[3] as i32);
        rect_2px(img, x1, y1, x2, y2, GREEN);
        let label = format!("{:.2}", b[4]);
        draw_text_mut(img, GREEN, x1, y1, scale, &font, &label);
    }
}

/// Draw tiling windows (blue). windows_lt: top-left coords; size = (h, w).
pub fn draw_windows(img: &mut RgbImage, windows_lt: &[(i32, i32)], window_h: i32, window_w: i32) {
    for &(x, y) in windows_lt {
        rect_2px(img, x, y, x + window_w, y + window_h, BLUE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_without_panicking_and_changes_pixels() {
        let mut img = RgbImage::from_pixel(200, 200, Rgb([0, 0, 0]));
        draw_boxes(&mut img, &[[10.0, 10.0, 100.0, 100.0, 0.91]]);
        draw_windows(&mut img, &[(0, 0)], 50, 50);
        // some pixel on the rectangle border should now be green
        let on_border = img.get_pixel(10, 50);
        assert_eq!(*on_border, GREEN);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd rs-back && cargo test draw`
Expected: compile error until `mod draw;` added (Step 3), then test should pass once it compiles. If the border-pixel assertion fails, adjust the asserted coordinate to one known to be on the drawn rectangle (the rectangle spans x=10..100 at y=10 and y=100, and x=10/x=100 columns).

- [ ] **Step 3: Add module to `main.rs`**

```rust
mod draw;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd rs-back && cargo test draw`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add rs-back/src/draw.rs rs-back/src/main.rs
git commit -m "feat: add box/window drawing"
```

---

## Task 9: PredictionTask + post-processing

Port `back/common.py:PredictionTask`. `done(result)` writes `result.json`, runs NMS to `boxes.json`, draws boxes → `origin.boxes.jpg`, then windows on the same image → `origin.windows.jpg`, then updates `num`/`status=done`. `nms_only` reloads `result.json` and reruns `done`. `image_url` returns the local origin path.

**Files:**
- Create: `rs-back/src/task.rs`
- Modify: `rs-back/src/main.rs` (add `mod task;`)

- [ ] **Step 1: Write `task.rs`**

```rust
use crate::db::{self, Db};
use crate::draw;
use crate::nms::nms;
use crate::params::Params;
use image::RgbImage;
use serde_json::Value;
use std::path::PathBuf;

pub struct PredictionTask {
    pub id: String,
    pub params: Params,
}

fn base_dir(id: &str) -> PathBuf {
    PathBuf::from("./detections").join(id)
}

impl PredictionTask {
    /// Load a task from the DB by id. Returns None if the row is missing.
    pub async fn from_id(pool: &Db, id: &str) -> anyhow::Result<Option<PredictionTask>> {
        let c = pool.get().await?;
        let rows = c
            .query("SELECT id, params FROM detections WHERE id = $1", &[&id])
            .await?;
        let Some(row) = rows.first() else { return Ok(None) };
        let params_str: String = row.get("params");
        let params: Params = serde_json::from_str(&params_str)?;
        Ok(Some(PredictionTask { id: row.get("id"), params }))
    }

    /// Local origin image path (always a file path, never http).
    pub fn image_url(&self) -> String {
        format!("./detections/{}/origin.jpg", self.id)
    }

    pub async fn set_status(&self, pool: &Db, status: &str) -> anyhow::Result<()> {
        db::update_detection(pool, &self.id, None, Some(status), None, None).await
    }

    /// Reload saved result.json and re-run post-processing (no worker round-trip).
    pub async fn nms_only(&self, pool: &Db) -> anyhow::Result<()> {
        let base = base_dir(&self.id);
        let raw = std::fs::read_to_string(base.join("result.json"))?;
        let result: Value = serde_json::from_str(&raw)?;
        self.done(pool, &result).await
    }

    /// Post-process a worker result. Mirrors back/common.py:PredictionTask.done.
    pub async fn done(&self, pool: &Db, result: &Value) -> anyhow::Result<()> {
        let base = base_dir(&self.id);
        std::fs::create_dir_all(&base)?;

        // 1. write raw result
        std::fs::write(base.join("result.json"), serde_json::to_vec(result)?)?;

        // 2. NMS -> boxes.json
        let raw_boxes: Vec<[f32; 5]> = result["boxes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let v = b.as_array()?;
                        Some([
                            v[0].as_f64()? as f32,
                            v[1].as_f64()? as f32,
                            v[2].as_f64()? as f32,
                            v[3].as_f64()? as f32,
                            v[4].as_f64()? as f32,
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        let boxes = nms(&raw_boxes, self.params.threshold as f32, self.params.iou as f32);
        let boxes_json: Vec<Vec<f32>> = boxes.iter().map(|b| b.to_vec()).collect();
        std::fs::write(base.join("boxes.json"), serde_json::to_vec(&boxes_json)?)?;

        // 3. draw boxes on origin -> origin.boxes.jpg
        let mut img: RgbImage = image::open(base.join("origin.jpg"))?.to_rgb8();
        draw::draw_boxes(&mut img, &boxes);
        img.save(base.join("origin.boxes.jpg"))?;

        // 4. draw windows on the same image -> origin.windows.jpg
        let ws = &result["window_size"];
        let window_h = ws[0].as_f64().unwrap_or(0.0) as i32;
        let window_w = ws[1].as_f64().unwrap_or(0.0) as i32;
        let windows_lt: Vec<(i32, i32)> = result["windows_lt"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let v = p.as_array()?;
                        Some((v[0].as_f64()? as i32, v[1].as_f64()? as i32))
                    })
                    .collect()
            })
            .unwrap_or_default();
        draw::draw_windows(&mut img, &windows_lt, window_h, window_w);
        img.save(base.join("origin.windows.jpg"))?;

        // 5. update DB
        db::update_detection(pool, &self.id, Some(boxes.len() as i32), Some("done"), None, None)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Add module to `main.rs`**

```rust
mod task;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd rs-back && cargo build`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add rs-back/src/task.rs rs-back/src/main.rs
git commit -m "feat: add PredictionTask post-processing"
```

---

## Task 10: Master protocol + worker loop

Port `back/master.py`. The master listens on `MASTER`, accepts connections, and spawns a worker task per connection. The task queue is a `tokio::mpsc` channel shared via an `Arc<Mutex<Receiver>>` so each worker can pull. `expire_workers` runs every 30s. On startup, all `queue`/`processing` detections are re-enqueued.

**Files:**
- Create: `rs-back/src/master.rs`
- Modify: `rs-back/src/main.rs` (add `mod master;`)

- [ ] **Step 1: Write `master.rs`**

```rust
use crate::db::{self, Db};
use crate::params::Params;
use crate::proto;
use crate::task::PredictionTask;
use serde_json::json;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Duration};

pub type TaskTx = mpsc::UnboundedSender<String>;
pub type TaskRx = Arc<Mutex<mpsc::UnboundedReceiver<String>>>;

/// Spawn the listener, the expirer, and re-enqueue outstanding tasks.
pub async fn start(pool: Db, addr: &str, tx: TaskTx, rx: TaskRx) -> anyhow::Result<()> {
    // Re-enqueue outstanding work (startup requeue).
    {
        let c = pool.get().await?;
        let rows = c
            .query("SELECT id FROM detections WHERE status = 'queue' OR status = 'processing'", &[])
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
                        .execute("DELETE FROM workers WHERE last_ping < $1", &[&(db::now() - 30)])
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
        let rx = rx.clone();
        tokio::spawn(async move {
            let mut w = Worker::new(sock, peer.ip().to_string(), pool, rx);
            if let Err(e) = w.run().await {
                tracing::warn!("worker {} ended: {e}", w.id);
            }
            w.cleanup().await;
        });
    }
}

struct Worker {
    sock: TcpStream,
    id: String,
    name: String,
    remote_addr: String,
    connected_at: i64,
    last_ping: i64,
    tasks_done: i64,
    avg_det_time: f64,
    pool: Db,
    rx: TaskRx,
}

impl Worker {
    fn new(sock: TcpStream, remote_addr: String, pool: Db, rx: TaskRx) -> Self {
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
            rx,
        }
    }

    /// Send ping, expect a 50-byte reply beginning with b"pong\0". Updates name + last_ping.
    async fn ping(&mut self) -> bool {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
        let name = String::from_utf8_lossy(tail).trim_matches('\0').trim().to_string();
        if !name.is_empty() {
            self.name = name;
        }
        let _ = self.sock.flush().await;
        true
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
    async fn predict(&mut self, img_path: &str, query: &serde_json::Value) -> Option<serde_json::Value> {
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
        let size = timeout(Duration::from_secs(20), self.sock.read_u32()).await.ok()?.ok()?;
        let mut data = vec![0u8; size as usize];
        timeout(Duration::from_secs(10), self.sock.read_exact(&mut data)).await.ok()?.ok()?;
        let end = timeout(Duration::from_secs(10), self.sock.read_u8()).await.ok()?.ok()?;
        if end != 0 {
            return None;
        }
        let obj: serde_json::Value = serde_json::from_slice(&data).ok()?;
        let det_time = obj["det_time"].as_f64().unwrap_or(0.0);
        self.avg_det_time =
            (self.avg_det_time * self.tasks_done as f64 + det_time) / (self.tasks_done as f64 + 1.0);
        self.tasks_done += 1;
        Some(obj)
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        if !self.ping().await {
            anyhow::bail!("failed initial ping");
        }
        loop {
            self.sync_to_db().await.ok();
            let task_id = self.read_task(5).await;
            let Some(task_id) = task_id else {
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
                let _ = enqueue(&self.rx, &task.id).await; // requeue handled below
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
                    anyhow::bail!("predict failed");
                }
            }
        }
    }

    async fn cleanup(&self) {
        if let Ok(c) = self.pool.get().await {
            let _ = c.execute("DELETE FROM workers WHERE id = $1", &[&self.id]).await;
        }
    }
}

/// Helper kept for symmetry; the requeue on failed-predict happens via the main queue Sender
/// captured at construction. Since Worker only holds the Receiver, push-backs use set_status
/// to 'queue' and rely on the startup/expire requeue OR an explicit tx clone (see main wiring).
async fn enqueue(_rx: &TaskRx, _id: &str) -> anyhow::Result<()> {
    Ok(())
}
```

Note on requeue: a worker that fails mid-task sets the row status back to `queue`. To make that row picked up again without a restart, the `Worker` also needs the `TaskTx`. Update the struct to carry `tx: TaskTx`, set it in `new`, pass it from `start`, and replace the `enqueue(...)` calls with `let _ = self.tx.send(task.id.clone());`. Apply this in Step 2.

- [ ] **Step 2: Wire the `TaskTx` into `Worker`**

Edit `master.rs`:
- Add `tx: TaskTx,` to the `Worker` struct.
- Add `tx: TaskTx` parameter to `Worker::new` and store it.
- In `start`, capture `let tx = tx.clone();` inside the accept loop and pass it to `Worker::new(sock, peer.ip().to_string(), pool, tx, rx)`.
- Replace both failure-requeue sites:
  - before predict: `task.set_status(&self.pool, "queue").await.ok(); let _ = self.tx.send(task.id.clone()); anyhow::bail!("ping failed before predict");`
  - on predict None: `task.set_status(&self.pool, "queue").await.ok(); let _ = self.tx.send(task.id.clone()); anyhow::bail!("predict failed");`
- Delete the `enqueue` helper and its calls.

- [ ] **Step 3: Add module to `main.rs`**

```rust
mod master;
```

- [ ] **Step 4: Verify it compiles**

Run: `cd rs-back && cargo build`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add rs-back/src/master.rs rs-back/src/main.rs
git commit -m "feat: add master protocol and worker loop"
```

---

## Task 11: HTTP routes

Port `back/main.py` handlers into an axum router. Shared `AppState { pool, tx }`. All routes are nested under `/api`.

**Files:**
- Create: `rs-back/src/routes.rs`
- Modify: `rs-back/src/main.rs` (add `mod routes;`)

- [ ] **Step 1: Write `routes.rs`**

```rust
use crate::db::{self, Db};
use crate::error::AppError;
use crate::master::TaskTx;
use crate::params::Params;
use crate::task::PredictionTask;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub tx: TaskTx,
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/", get(hello))
        .route("/april-fools", post(april_fools))
        .route("/detections", post(new_detection).get(list_detections))
        .route("/detections/:id", get(get_detection).delete(delete_detection))
        .route("/detections/:id/params", put(put_params))
        .route("/detections/:id/remark", put(put_remark))
        .route("/detections/:id/:im", get(get_image))
        .route("/workers", get(get_workers))
        .with_state(state);
    Router::new().nest("/api", api)
}

async fn hello() -> impl IntoResponse {
    Json(json!({"message": "Hello World"}))
}

async fn april_fools(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    let c = s.pool.get().await?;
    c.execute("INSERT INTO april_fools (created_at) VALUES (NOW())", &[]).await?;
    let row = c.query_one("SELECT COUNT(*) AS count FROM april_fools", &[]).await?;
    let count: i64 = row.get("count");
    Ok(Json(json!({ "count": count })))
}

/// Fetch one detection as the API JSON object (params parsed, optional queue field).
async fn fetch_detection_json(pool: &Db, id: &str) -> Result<Option<Value>, AppError> {
    let c = pool.get().await?;
    let rows = c.query("SELECT * FROM detections WHERE id = $1", &[&id]).await?;
    let Some(row) = rows.first() else { return Ok(None) };
    let mut obj = db::detection_row_to_json(row);
    if obj["status"] == json!("queue") {
        obj["queue"] = json!(1);
    }
    Ok(Some(obj))
}

async fn get_detection(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    match fetch_detection_json(&s.pool, &id).await? {
        Some(obj) => Ok(Json(obj)),
        None => Err(AppError::NotFound),
    }
}

async fn new_detection(
    State(s): State<AppState>,
    mut mp: Multipart,
) -> Result<Json<Value>, AppError> {
    // find the "file" field
    let mut file_bytes: Option<Vec<u8>> = None;
    while let Some(field) = mp.next_field().await.map_err(|e| AppError::Internal(e.to_string()))? {
        if field.name() == Some("file") {
            file_bytes = Some(field.bytes().await.map_err(|e| AppError::Internal(e.to_string()))?.to_vec());
            break;
        }
    }
    let Some(bytes) = file_bytes else {
        return Err(AppError::BadRequest("No Image".into()));
    };

    let id = uuid::Uuid::new_v4().to_string();
    let base = std::path::PathBuf::from("./detections").join(&id);
    std::fs::create_dir_all(&base).map_err(|e| AppError::Internal(e.to_string()))?;

    // decode + re-encode to jpg (cv2.imdecode/imwrite equivalent)
    let img = image::load_from_memory(&bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    img.to_rgb8().save(base.join("origin.jpg")).map_err(|e| AppError::Internal(e.to_string()))?;

    let params = Params::default();
    let params_str = serde_json::to_string(&params).unwrap();
    let now = db::now();
    {
        let c = s.pool.get().await?;
        c.execute(
            "INSERT INTO detections (id, params, modified_at, created_at, remark, status) VALUES ($1,$2,$3,$4,$5,$6)",
            &[&id, &params_str, &now, &now, &"", &"queue"],
        )
        .await?;
    }
    let _ = s.tx.send(id.clone());

    match fetch_detection_json(&s.pool, &id).await? {
        Some(obj) => Ok(Json(obj)),
        None => Err(AppError::NotFound),
    }
}

async fn put_params(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    // Presence + type check, matching Python "Missing {k}" / tiling bool.
    for k in ["tiling", "window_size", "overlap", "threshold", "iou"] {
        if body.get(k).is_none() {
            return Err(AppError::BadRequest(format!("Missing {k}")));
        }
    }
    if !body["tiling"].is_boolean() {
        return Err(AppError::BadRequest("tiling should be bool".into()));
    }
    let new_params: Params = serde_json::from_value(body.clone())
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    new_params.validate().map_err(AppError::BadRequest)?;

    // load existing row
    let c = s.pool.get().await?;
    let rows = c.query("SELECT * FROM detections WHERE id = $1", &[&id]).await?;
    let Some(row) = rows.first() else { return Err(AppError::NotFound) };
    let status: Option<String> = row.try_get("status").ok().flatten();
    if status.as_deref() != Some("done") {
        // no change; return current detection
        return get_detection(State(s), Path(id)).await;
    }
    let old_params_str: String = row.get("params");
    let old_params: Params = serde_json::from_str(&old_params_str)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    drop(rows);
    drop(c);

    if old_params == new_params {
        // unchanged -> return row (parsed)
        return get_detection(State(s), Path(id)).await;
    }

    let new_params_str = serde_json::to_string(&new_params).unwrap();
    db::update_detection(&s.pool, &id, None, Some("queue"), None, Some(&new_params_str)).await?;

    let geometry_same = old_params.window_size == new_params.window_size
        && old_params.overlap == new_params.overlap
        && old_params.tiling == new_params.tiling;

    if geometry_same {
        // only threshold/iou changed -> recompute inline, no worker
        let task = PredictionTask { id: id.clone(), params: new_params };
        task.nms_only(&s.pool).await.map_err(|e| AppError::Internal(e.to_string()))?;
        return get_detection(State(s), Path(id)).await;
    }

    let _ = s.tx.send(id.clone());
    get_detection(State(s), Path(id)).await
}

async fn put_remark(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let remark = body.get("remark").and_then(|v| v.as_str()).unwrap_or("");
    {
        let c = s.pool.get().await?;
        let rows = c.query("SELECT id FROM detections WHERE id = $1", &[&id]).await?;
        if rows.is_empty() {
            return Err(AppError::NotFound);
        }
    }
    db::update_detection(&s.pool, &id, None, None, Some(remark), None).await?;
    get_detection(State(s), Path(id)).await
}

async fn delete_detection(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let base = std::path::PathBuf::from("./detections").join(&id);
    if base.exists() {
        let _ = std::fs::remove_dir_all(&base);
    }
    let c = s.pool.get().await?;
    c.execute("DELETE FROM detections WHERE id = $1", &[&id]).await?;
    Ok(Json(json!({ "id": id })))
}

async fn get_image(
    Path((id, im)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let file = match im.as_str() {
        "origin" => "origin.jpg",
        "boxes" => "origin.boxes.jpg",
        "windows" => "origin.windows.jpg",
        _ => return Err(AppError::NotFound),
    };
    let path = std::path::PathBuf::from("./detections").join(&id).join(file);
    if !path.exists() {
        return Err(AppError::NotFound);
    }
    let bytes = tokio::fs::read(&path).await.map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg".to_string()),
            (header::CACHE_CONTROL, "max-age=2629746".to_string()),
        ],
        bytes,
    )
        .into_response())
}

async fn get_workers(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    let c = s.pool.get().await?;
    let rows = c
        .query(
            "SELECT id, name, connected_at, avg_det_time, last_ping, tasks_done FROM workers ORDER BY last_ping DESC",
            &[],
        )
        .await?;
    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<_, String>("id"),
                "name": r.try_get::<_, Option<String>>("name").ok().flatten(),
                "connected_at": r.try_get::<_, Option<i32>>("connected_at").ok().flatten(),
                "avg_det_time": r.try_get::<_, Option<f64>>("avg_det_time").ok().flatten(),
                "last_ping": r.try_get::<_, Option<i32>>("last_ping").ok().flatten(),
                "tasks_done": r.try_get::<_, Option<i32>>("tasks_done").ok().flatten(),
            })
        })
        .collect();
    Ok(Json(json!({ "data": data })))
}

async fn list_detections(
    State(s): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let size: i64 = q.get("size").and_then(|v| v.parse().ok()).unwrap_or(20);
    let page: i64 = q.get("page").and_then(|v| v.parse().ok()).unwrap_or(1);
    let sortby = q.get("sortby").map(|s| s.as_str()).unwrap_or("modified_at");
    let sort = q.get("sort").map(|s| s.as_str()).unwrap_or("desc");

    if !(1..=100).contains(&size) {
        return Err(AppError::BadRequest("Size should be within [1, 100]".into()));
    }
    if page < 1 {
        return Err(AppError::BadRequest("Page should be greater than 0".into()));
    }
    if !["modified_at", "num", "status", "created_at"].contains(&sortby) {
        return Err(AppError::BadRequest("Invalid sortby".into()));
    }
    if !["asc", "desc"].contains(&sort) {
        return Err(AppError::BadRequest("Invalid sort".into()));
    }

    let c = s.pool.get().await?;
    // sortby/sort are whitelisted above, safe to interpolate
    let sql = format!(
        "SELECT id,num,modified_at,created_at,remark,status FROM detections ORDER BY {sortby} {sort} NULLS LAST LIMIT $1 OFFSET $2"
    );
    let rows = c.query(&sql, &[&size, &((page - 1) * size)]).await?;
    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<_, String>("id"),
                "num": r.try_get::<_, Option<i32>>("num").ok().flatten(),
                "modified_at": r.try_get::<_, Option<i32>>("modified_at").ok().flatten(),
                "created_at": r.try_get::<_, Option<i32>>("created_at").ok().flatten(),
                "remark": r.try_get::<_, Option<String>>("remark").ok().flatten(),
                "status": r.try_get::<_, Option<String>>("status").ok().flatten(),
            })
        })
        .collect();
    let total: i64 = c.query_one("SELECT COUNT(*) AS count FROM detections", &[]).await?.get("count");
    Ok(Json(json!({ "total": total, "data": data })))
}
```

- [ ] **Step 2: Add module to `main.rs`**

```rust
mod routes;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd rs-back && cargo build`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add rs-back/src/routes.rs rs-back/src/main.rs
git commit -m "feat: add HTTP routes"
```

---

## Task 12: Main wiring + smoke test

Assemble everything in `main.rs`: load config, build pool, ensure tables, create the queue channel, spawn the master, run axum.

**Files:**
- Modify: `rs-back/src/main.rs` (replace placeholder `main`)

- [ ] **Step 1: Write the full `main.rs`**

```rust
mod config;
mod db;
mod draw;
mod error;
mod master;
mod nms;
mod params;
mod proto;
mod routes;
mod task;

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cfg = config::Config::from_env();

    let pool = db::make_pool(&cfg.db)?;
    db::ensure_tables(&pool).await?;
    std::fs::create_dir_all("./detections")?;

    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let rx: master::TaskRx = Arc::new(Mutex::new(rx));

    // master listener (+ expirer + startup requeue)
    {
        let pool = pool.clone();
        let tx = tx.clone();
        let rx = rx.clone();
        let addr = cfg.master.clone();
        tokio::spawn(async move {
            if let Err(e) = master::start(pool, &addr, tx, rx).await {
                tracing::error!("master stopped: {e}");
            }
        });
    }

    let state = routes::AppState { pool, tx };
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", cfg.http_port)).await?;
    tracing::info!("http listening on :{}", cfg.http_port);
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 2: Verify the whole crate builds**

Run: `cd rs-back && cargo build`
Expected: compiles with no errors.

- [ ] **Step 3: Run the full test suite**

Run: `cd rs-back && cargo test -- --test-threads=1`
Expected: all unit tests from Tasks 2, 3, 4, 5, 6, 8 pass.

- [ ] **Step 4: Smoke-test the hello route against a throwaway router**

Add to `rs-back/src/routes.rs` test module:
```rust
#[cfg(test)]
mod route_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn hello_route_works() {
        // hello has no DB dependency; build a minimal router with just it.
        let app = Router::new().nest("/api", Router::new().route("/", get(hello)));
        let resp = app
            .oneshot(Request::builder().uri("/api/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["message"], "Hello World");
    }
}
```

- [ ] **Step 5: Run the route test**

Run: `cd rs-back && cargo test hello_route_works`
Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add rs-back/src/main.rs rs-back/src/routes.rs
git commit -m "feat: wire main + add hello route smoke test"
```

---

## Task 13: Manual verification against a live database

This is a checkpoint, not code. Verify the binary runs end-to-end with the real Postgres and matches the Python API.

- [ ] **Step 1: Ensure Postgres is reachable** (the dev DB from `config.py`). If not running, start it (e.g. `docker run -e POSTGRES_USER=swift -e POSTGRES_PASSWORD=swift -e POSTGRES_DB=swift -p 5432:5432 postgres`).

- [ ] **Step 2: Run the server**

Run: `cd rs-back && cargo run`
Expected: logs `http listening on :20000` and `master accepting on 0.0.0.0:12345`.

- [ ] **Step 3: Hit the API**

```bash
curl -s localhost:20000/api/ ; echo
curl -s localhost:20000/api/workers ; echo
curl -s "localhost:20000/api/detections?size=5&page=1" ; echo
curl -s -F "file=@det/result.jpg" localhost:20000/api/detections ; echo
```
Expected: hello message; `{"data":[...]}`; a paginated list with `total`; and a new detection JSON with `status:"queue"` and `queue:1`.

- [ ] **Step 4: Confirm disk artifacts**

Run: `ls detections/<the-new-id>/`
Expected: `origin.jpg` present.

- [ ] **Step 5: (If a real `rs-worker` is available) point it at `localhost:12345`** and confirm a queued detection transitions to `done` and `origin.boxes.jpg` / `origin.windows.jpg` / `result.json` / `boxes.json` appear.

- [ ] **Step 6: Commit any fixes found during verification.**

---

## Self-Review Notes

- **Spec coverage:** HTTP surface (Tasks 11), TCP protocol + worker loop + expirer + startup requeue (Task 10), Postgres schema/queries (Tasks 7, 11), disk artifacts + NMS + drawing (Tasks 2, 8, 9), config (Task 4), error mapping (Task 6), param validation incl. inline nms_only path (Tasks 3, 11). All spec sections map to a task.
- **Compatibility subtleties captured:** strict `score > threshold` in NMS; `threshold=0.05/iou=0.95` worker override; `tiling=false` → `window_size=1.0/overlap=0.0`; windows image drawn on top of boxes image; `queue:1` annotation on `GET`; whitelisted `sortby`/`sort` interpolation; `created_at`/`modified_at` as integer seconds; `max-age=2629746` on images.
- **Known acceptable deviation:** score-label font differs from OpenCV HERSHEY_SIMPLEX (design decision); rectangle thickness approximated by two inset hollow rects.
