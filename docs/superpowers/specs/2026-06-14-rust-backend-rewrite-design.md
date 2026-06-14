# Rust Backend Rewrite — Design

**Date:** 2026-06-14
**Status:** Approved

## Goal

Rewrite the Python backend (`back/`) in Rust while preserving full compatibility
with everything that depends on it. The existing Vue frontend, the existing Rust
ML worker (`rs-worker/`), the live PostgreSQL database, and the on-disk detection
artifacts must all keep working without modification.

The Python backend stays in place; the new crate lives at `rs-back/`.

## What the backend does today

The Python backend (Sanic + psycopg2 + OpenCV) performs four jobs:

1. **REST API** under `/api` — detections CRUD, params/remark updates, image
   serving, worker list, april-fools counter.
2. **TCP master** on `0.0.0.0:12345` — assigns queued detection tasks to ML
   workers over a custom binary protocol. The existing `rs-worker` is its client.
3. **PostgreSQL persistence** — tables `detections`, `workers`, `april_fools`.
4. **Post-processing** — runs NMS on returned boxes, draws boxes + tiling windows
   onto the image, writes `result.json` / `boxes.json` / `origin.boxes.jpg` /
   `origin.windows.jpg` under `./detections/{id}/`.

## Compatibility constraints (non-negotiable)

- **Reuse the live Postgres in place** — same `DB` URL, same
  `CREATE TABLE IF NOT EXISTS` schema and column types. Existing rows keep working.
- **Same disk layout** — `./detections/{id}/{origin,origin.boxes,origin.windows}.jpg`,
  `result.json`, `boxes.json`.
- **Byte-compatible TCP protocol** — identical ping/predict framing so `rs-worker`
  connects unchanged.
- **Same HTTP surface** — identical routes, JSON shapes, and HTTP status codes.

## Architecture (Approach A)

A single Rust binary on a tokio runtime running everything:

- an **axum** HTTP server for `/api`,
- a **TCP listener** for the master protocol,
- **one tokio task per connected worker**,
- a background **expirer** task.

The task queue (today a `multiprocessing.Queue` shared via Sanic `shared_ctx`)
becomes an in-process `tokio::mpsc` channel — no IPC. PostgreSQL access goes
through `tokio-postgres` + a `deadpool-postgres` connection pool. Image drawing
uses pure-Rust `image` + `imageproc` (no native OpenCV dependency).

Rejected alternatives: actix-web (no upside here; axum shares tokio with the
worker), and a two-binary split mirroring Python's multiprocessing (would need
real IPC/DB-polling for the shared queue — more moving parts, no benefit).

### Module layout

```
rs-back/
  Cargo.toml
  assets/font.ttf          # embedded font for score labels (imageproc needs a glyph font)
  src/
    main.rs                # bootstrap: init DB/tables, spawn master + expirer, run axum
    config.rs              # DB url, BASE, MASTER addr, HTTP port (env-overridable defaults)
    db.rs                  # deadpool-postgres pool, ensure_tables, detection/worker queries
    routes.rs              # all /api handlers (<- main.py)
    task.rs                # PredictionTask: from_id, image_url, nms_only, set_status, done (<- common.py)
    nms.rs                 # nms() (<- common.py)
    draw.rs                # box + window drawing via image/imageproc (<- cv2 calls in done())
    master.rs              # TCP listener + per-worker task + queue + expire_workers (<- master.py)
    proto.rs               # ping/predict wire framing (read_until_zero, !I length frames)
```

Shared axum `State`: the deadpool pool + the `mpsc::Sender<String>` task queue.
The master holds the `Receiver`.

## Data model

Schema created with `CREATE TABLE IF NOT EXISTS`, identical to today:

```sql
CREATE TABLE IF NOT EXISTS detections (
  id TEXT PRIMARY KEY, params TEXT, modified_at INTEGER, created_at INTEGER,
  num INTEGER, remark TEXT, status TEXT);

CREATE TABLE IF NOT EXISTS workers (
  id TEXT PRIMARY KEY, name TEXT, remote_addr TEXT, connected_at INTEGER,
  last_ping INTEGER, tasks_done INTEGER, avg_det_time FLOAT);

CREATE TABLE IF NOT EXISTS april_fools (
  id SERIAL PRIMARY KEY, created_at TIMESTAMP);
```

`created_at` / `modified_at` are stored as **truncated epoch seconds** (matching
today's `time.time()` float written into an INTEGER column).

`detections.params` is a JSON **string** in the column; the API parses it to an
object on read. Default params for a new detection:
`{tiling:true, window_size:0.3, overlap:0.1, threshold:0.3, iou:0.5}`.

## HTTP surface (exact parity)

| Method | Route | Behavior |
|---|---|---|
| GET | `/api/` | `{"message":"Hello World"}` |
| POST | `/api/april-fools` | insert `NOW()`, return `{"count":N}` |
| POST | `/api/detections` | multipart field `file` → decode image → write `origin.jpg` → insert row `status=queue` → enqueue id → return detection JSON. Missing file → `400 {"error":"No Image"}` |
| GET | `/api/detections` | paginated list. Query: `size` (default 20, `[1,100]`), `page` (default 1, `>=1`), `sortby` (default `modified_at`, one of `modified_at`/`num`/`status`/`created_at`), `sort` (default `desc`, `asc`/`desc`). Returns `{total, data:[{id,num,modified_at,created_at,remark,status}]}` ordered by `<sortby> <sort> NULLS LAST`. Invalid args → `400`. |
| GET | `/api/detections/<id>` | detection JSON; `params` parsed to object; adds `"queue":1` when status=queue. Not found → `404 {"error":"Not Found"}` |
| PUT | `/api/detections/<id>/params` | body = params object. Validate all 5 keys present; `tiling` bool; `window_size` in `(0,1]`; `overlap` in `[0,1)`; `threshold` in `[0,1]`; `iou` in `[0,1]`. If row missing → `404`. If status != `done` → return unchanged detection. If params equal current → return row. If only `threshold`/`iou` changed (tiling/window_size/overlap unchanged) → set params + `status=queue`, then run **inline `nms_only`** (no worker) and return. Otherwise set params + `status=queue` and enqueue id. |
| PUT | `/api/detections/<id>/remark` | update `remark` (default `""`); missing row → `404`; returns detection JSON |
| DELETE | `/api/detections/<id>` | `rmtree ./detections/<id>` if present + delete row → `{"id":id}` |
| GET | `/api/detections/<id>/<im>` | `im` ∈ `{origin,boxes,windows}` mapped to `origin.jpg`/`origin.boxes.jpg`/`origin.windows.jpg`; serve with `Cache-Control: max-age=2629746`. Unknown `im` or missing file → `404` |
| GET | `/api/workers` | `{"data":[{id,name,connected_at,avg_det_time,last_ping,tasks_done}]}` ordered by `last_ping desc` |

Detection JSON fields: `id, params (object), modified_at, created_at, num, remark,
status` (+ optional `queue`).

## TCP master protocol (byte-identical)

Listener binds `MASTER` (`0.0.0.0:12345`) with `SO_REUSEPORT`. Per accepted
connection, spawn a worker task with a fresh uuid and stats
(`connected_at`, `last_ping`, `tasks_done=0`, `avg_det_time=0.0`, `remote_addr`,
`name`).

**ping:** send `ping\0`; expect a 50-byte reply starting with `pong\0`; 3s timeout.
The bytes after `pong\0` up to the trailing `\0` padding are the worker `name`
(if non-empty). No valid pong → worker is dropped.

**Worker loop:**
1. Upsert worker row (`INSERT … ON CONFLICT (id) DO UPDATE`).
2. Pull a task id from the queue with a 5s timeout → `PredictionTask::from_id`.
3. If no task → ping; if ping fails break, else continue.
4. If task → ping check (on failure: requeue task, break).
5. `set_status("processing")`, transform query, `predict`, then `task.done(result)`.
   On predict failure: requeue task, break.

**Query transform before send:** copy params; if `!tiling` → `window_size=1.0`,
`overlap=0.0`; always override `threshold=0.05`, `iou=0.95`. (The worker runs a
loose detection; the master applies the real `threshold`/`iou` in NMS.)

**predict framing:** `image_url()` returns the local path
`./detections/{id}/origin.jpg` (not `http`), so send `predict\0` + `!I`(big-endian
u32) image length + image bytes + `!I` query-JSON length + query bytes + `\0`.
`predict_url\0` path (URL instead of bytes) is preserved for protocol
completeness. Read response: `!I` size + `size` bytes + a trailing `\0` byte
(else treat as failure). Update
`avg_det_time = (avg_det_time*tasks_done + det_time)/(tasks_done+1)`, then
`tasks_done += 1`.

**On disconnect/close:** delete the worker row; close DB resources.

**expire_workers:** every 30s, `DELETE FROM workers WHERE last_ping < now-30`.

**Startup requeue:** select all detections with `status IN ('queue','processing')`
and enqueue their ids.

## Post-processing (`PredictionTask::done`)

Response JSON from worker contains: `boxes` (`[x1,y1,x2,y2,score]`), `windows_lt`
(`[[x,y],…]`), `window_size` (`[h,w]`), `window_num`, `det_time`, `transfer_time`.

Steps (identical order to today):
1. Write `result.json` (raw response).
2. `boxes = nms(boxes, params.threshold, params.iou)` → write `boxes.json`.
3. Draw on a copy of `origin.jpg`: for each box a **green** rectangle (2px) + the
   score text (`{score:.2f}`) at the top-left → write `origin.boxes.jpg`.
4. On that same image (boxes already drawn), add **blue** window rectangles from
   `windows_lt` with size `[h,w]` → write `origin.windows.jpg`.
   (So the windows image contains boxes + windows; the boxes image has boxes only.
   Colors match OpenCV's BGR intent: `(0,255,0)`=green for boxes,
   `(255,0,0)` BGR=blue for windows.)
5. `UPDATE detections SET num=len(boxes), status='done', modified_at=now`.

`nms_only` reloads `result.json` and reruns steps 2–5 (used by the inline
threshold/iou-only param change — no worker round-trip).

## NMS algorithm (port of `common.py:nms`)

Input boxes `[x1,y1,x2,y2,score]`. Filter `score > threshold` (strict). Sort by
score descending. Greedily pick the top box, suppress remaining boxes whose IoU
with it is `> iou`, repeat. `area = (y2-y1)*(x2-x1)`. Return surviving boxes in the
same `[x1,y1,x2,y2,score]` shape.

## Image fidelity

Pure Rust (`image` + `imageproc`), no native OpenCV dependency. Rectangles match
exactly; the score-label font is the closest practical match to OpenCV's
`FONT_HERSHEY_SIMPLEX` via an embedded TTF (`assets/font.ttf`). Slight font
appearance difference is accepted (per design decision).

## Configuration

`config.rs` defaults to today's values, each overridable by an env var:

- `DB` = `postgresql://swift:swift@localhost:5432/swift`
- `BASE` = `http://back.bwrrc.org.cn`
- `MASTER` = `0.0.0.0:12345`
- HTTP port = `20000`

## Error handling

- Validation/lookup failures return the same JSON error bodies and status codes
  as today (`400` for bad input, `404 {"error":"Not Found"}` for missing rows/files).
- Unexpected DB / IO / decode errors map to `500`.
- A worker task that errors closes its connection and deletes its row; it never
  takes down the HTTP server or master listener.

## Testing strategy

- **NMS unit tests** — port the algorithm and assert against hand-computed cases
  (overlap suppression, threshold filtering, ordering).
- **Param-validation table tests** — boundary values for each of the 5 params.
- **Proto round-trip test** — encode a `predict` frame and decode it back; verify
  `!I` framing and null terminators.
- **HTTP integration tests** — drive the axum router in-process for the CRUD and
  validation paths (using a test database or a thin DB abstraction).

## Out of scope

- No change to `rs-worker/`, the frontend, or the database schema.
- No new features; behavior parity only.
