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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cfg = config::Config::from_env();

    let pool = db::make_pool(&cfg.db)?;
    db::ensure_tables(&pool).await?;
    std::fs::create_dir_all("./detections")?;

    // MPMC task queue: every worker holds a receiver clone (see master.rs).
    let (tx, rx) = async_channel::unbounded::<String>();

    // master listener (+ expirer + startup requeue). If the master ever stops
    // (e.g. it cannot bind the port), exit the whole process so systemd's
    // `Restart=always` relaunches it — rather than silently serving HTTP with a
    // dead master and never processing detections.
    {
        let pool = pool.clone();
        let tx = tx.clone();
        let rx = rx.clone();
        let addr = cfg.master.clone();
        tokio::spawn(async move {
            if let Err(e) = master::start(pool, &addr, tx, rx).await {
                tracing::error!("master stopped, exiting for restart: {e}");
                std::process::exit(1);
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
