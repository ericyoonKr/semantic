use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::error::ApiError;
use crate::session::SessionId;
use crate::state::AppState;
use crate::tasks;

pub async fn delete(
    State(state): State<AppState>,
    Path(sid): Path<String>,
) -> Result<StatusCode, ApiError> {
    let sid = SessionId::parse(&sid)?;
    if let Some(handle) = state.sessions.take(&sid) {
        handle.cancel.cancel();
        let grace = state.cfg.limits.shutdown_grace;
        if tokio::time::timeout(grace, handle.done.cancelled())
            .await
            .is_err()
        {
            tracing::warn!("session actor did not stop in time");
        }
    }
    let _guard = state.kg_ids.lock(&sid).await;
    let db = state.db.clone();
    let session = sid.as_str().to_string();
    tasks::blocking(move || db.delete_session(&session)).await??;
    tracing::info!("session state deleted");
    Ok(StatusCode::NO_CONTENT)
}
