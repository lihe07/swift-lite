#![allow(dead_code)]

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
