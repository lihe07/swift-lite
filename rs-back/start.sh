#!/bin/bash
# Build and run the Rust backend. Mirrors back/start.sh (HTTP on :20000, master on :12345).
# Config is overridable via env: DB, BASE, MASTER, HTTP_PORT.
set -e
cd "$(dirname "$0")"
cargo build --release
exec ./target/release/rs-back
