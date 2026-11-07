use std::panic::{AssertUnwindSafe, catch_unwind};

use tokio::sync::oneshot;

use crate::error::ApiError;

pub async fn cpu<T, F>(pool: &rayon::ThreadPool, f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    pool.spawn(move || {
        let out = catch_unwind(AssertUnwindSafe(f));
        let _ = tx.send(out);
    });
    match rx.await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(_)) => {
            tracing::error!("cpu task panicked");
            Err(ApiError::internal())
        }
        Err(_) => Err(ApiError::internal()),
    }
}

pub async fn blocking<T, F>(f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        tracing::error!(panicked = e.is_panic(), "blocking task failed");
        ApiError::internal()
    })
}
