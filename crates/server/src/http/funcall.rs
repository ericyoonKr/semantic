use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};

use crate::error::ApiError;
use crate::funcall::check::{CheckReply, CheckRequest, MAX_SENTENCES, check_sentences};
use crate::funcall::coverage::{CoverageParams, CoverageReply};
use crate::funcall::docs::session_docs;
use crate::funcall::next_step::{Step, StepRequest, keyword};
use crate::funcall::route::is_legal;
use crate::session::SessionId;
use crate::state::AppState;

pub async fn research_coverage(
    State(state): State<AppState>,
    params: Result<Query<CoverageParams>, QueryRejection>,
) -> Result<Json<CoverageReply>, ApiError> {
    let Query(p) = params.map_err(|_| ApiError::bad_request("인자 오류: request_id"))?;
    Ok(Json(state.coverage.reply(&p.request_id)))
}

pub async fn next_step(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    body: Result<Json<StepRequest>, JsonRejection>,
) -> Result<Json<Step>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: request_id·question"))?;
    let db = state.db.clone();
    let session = sid.as_str().to_string();
    let docs = tokio::task::spawn_blocking(move || session_docs(&db, &session)).await.unwrap_or_default();
    let mut legal = true;
    if state.cfg.route_legal
        && keyword(&req.question).is_some()
        && state.steps.new_turn(sid.as_str(), &req.request_id)
        && let Some(parts) = state.engine.as_ref()
    {
        legal = is_legal(&parts.llm, &req.question, !docs.is_empty()).await.unwrap_or(true);
        tracing::info!(legal, "route.legal");
    }
    Ok(Json(state.steps.next(sid.as_str(), &req, legal, &docs)))
}

pub async fn answer_check(
    State(state): State<AppState>,
    body: Result<Json<CheckRequest>, JsonRejection>,
) -> Result<Json<CheckReply>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: thread_id·sentences"))?;
    if req.sentences.len() > MAX_SENTENCES {
        return Err(ApiError::bad_request("인자 오류: 문장이 너무 많습니다"));
    }
    Ok(Json(check_sentences(&state.reads, &req)))
}
