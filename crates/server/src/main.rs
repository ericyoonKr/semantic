use anyhow::Context;
use server::{AppState, Config, http, logging, shutdown};

fn main() -> anyhow::Result<()> {
    let cfg = Config::from_env()?;
    let _log = logging::init(&cfg.log_dir())?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(cfg.threads.tokio_workers)
        .max_blocking_threads(cfg.threads.tokio_blocking)
        .enable_all()
        .build()
        .context("async runtime could not be built")?;
    runtime.block_on(run(cfg))
}

async fn run(cfg: Config) -> anyhow::Result<()> {
    let bind = cfg.bind;
    let grace = cfg.limits.shutdown_grace;
    let state = AppState::open(cfg).await?;
    let app = http::router(state.clone());
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .context("bind address could not be opened")?;
    tracing::info!(%bind, "listening");
    tokio::spawn(shutdown::on_signal(state.shutdown.clone()));
    let serving =
        axum::serve(listener, app).with_graceful_shutdown(state.shutdown.clone().cancelled_owned());
    shutdown::serve_until_stopped(serving, &state, grace).await?;
    tracing::info!("stopped");
    Ok(())
}
