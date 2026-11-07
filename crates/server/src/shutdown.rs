use std::future::IntoFuture;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::state::AppState;

pub async fn on_signal(stop: CancellationToken) {
    match tokio::signal::ctrl_c().await {
        Ok(()) => tracing::info!("stop signal received"),
        Err(e) => tracing::error!(error = %e, "stop signal could not be watched"),
    }
    stop.cancel();
}

pub async fn serve_until_stopped<S>(
    serving: S,
    state: &AppState,
    grace: Duration,
) -> std::io::Result<()>
where
    S: IntoFuture<Output = std::io::Result<()>>,
{
    let stop = state.shutdown.clone();
    let bounded = async {
        stop.cancelled().await;
        tokio::time::sleep(grace).await;
    };
    let served = tokio::select! {
        r = serving.into_future() => r,
        () = bounded => {
            tracing::warn!("in-flight requests did not finish within the grace period");
            Ok(())
        }
    };
    finish(state, grace).await;
    served
}

pub async fn finish(state: &AppState, grace: Duration) {
    state.shutdown.cancel();
    state.tracker.close();
    if tokio::time::timeout(grace, state.tracker.wait())
        .await
        .is_err()
    {
        tracing::warn!("session actors did not stop within the grace period");
    }
    let db = state.db.clone();
    match tokio::task::spawn_blocking(move || db.flush()).await {
        Ok(Ok(())) => tracing::info!("session store flushed"),
        Ok(Err(e)) => tracing::error!(error = %e, "session store flush failed"),
        Err(_) => tracing::error!("session store flush task failed"),
    }
}
