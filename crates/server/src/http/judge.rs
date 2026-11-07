use std::time::Instant;

use axum::Json;
use axum::extract::{Path, State};
use tokio::sync::oneshot;

use crate::error::ApiError;
use crate::judge::request::prepare;
use crate::judge::{JudgeBody, JudgeReply};
use crate::session::{SessionEvent, SessionId, send};
use crate::state::AppState;

pub async fn judge(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(body): Json<JudgeBody>,
) -> Result<Json<JudgeReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let deadline = state.cfg.judge.deadline;
    let job = prepare(&state.rules, &sid, body, Instant::now() + deadline)?;
    let (reply, rx) = oneshot::channel();
    let event = SessionEvent::Judge {
        job: Box::new(job),
        ticket: None,
        reply,
        span: tracing::Span::current(),
    };
    send(&state, &sid, event)?;
    match tokio::time::timeout(deadline, rx).await {
        Ok(Ok(out)) => out.map(Json),
        Ok(Err(_)) => Err(ApiError::closed()),
        Err(_) => Err(ApiError::timeout()),
    }
}
