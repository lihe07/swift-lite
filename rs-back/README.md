# rs-back

A drop-in Rust reimplementation of the Python backend in `../back`, built with
axum + tokio + tokio-postgres. It preserves full compatibility:

- **HTTP API** under `/api` — same routes, JSON shapes, and status codes.
- **TCP master** on `0.0.0.0:12345` — byte-identical protocol, so the existing
  `../rs-worker` connects unchanged.
- **PostgreSQL** — same `CREATE TABLE IF NOT EXISTS` schema and column types;
  reuses the live database in place.
- **Disk layout** — `./detections/{id}/{origin,origin.boxes,origin.windows}.jpg`,
  `result.json`, `boxes.json`.

Image drawing is pure Rust (`image` + `imageproc`); detection-score labels use a
bundled font (`assets/font.ttf`) instead of OpenCV's HERSHEY_SIMPLEX, so the box
text looks slightly different. Everything else is behavior-for-behavior identical.

## Run

```sh
./start.sh
# or
cargo run --release
```

## Configuration (env overrides)

| Var | Default |
|---|---|
| `DB` | `postgresql://swift:swift@localhost:5432/swift` |
| `BASE` | `http://back.bwrrc.org.cn` |
| `MASTER` | `0.0.0.0:12345` |
| `HTTP_PORT` | `20000` |

## Test

```sh
cargo test
```
