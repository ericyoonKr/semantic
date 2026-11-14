use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};

use crate::error::ApiError;
use crate::funcall::check::{CheckReply, CheckRequest, MAX_SENTENCES, UnreadReply, UnreadRequest, check_sentences, unread_in_corpus};
use crate::funcall::coverage::{CoverageParams, CoverageReply};
use crate::funcall::docindex::{DocSearchReply, DocSearchRequest, search};
use crate::funcall::docs::session_docs;
use crate::funcall::holdings::{guidance_for, notes_for, scope_for};
use crate::funcall::known::law_of_label;
use crate::funcall::next_step::{Context, Step, StepReply, StepRequest, asks_for_format, asks_for_law, keyword, question_laws};
use crate::funcall::quotes::{QuotesReply, QuotesRequest, match_quotes};
use crate::funcall::relevance::{JudgeReply, JudgeRequest, judge, unjudged};
use crate::funcall::review::{ReviewReply, ReviewRequest, review};
use crate::funcall::route::is_legal;
use crate::session::SessionId;
use crate::state::AppState;

pub async fn docs_search(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    body: Result<Json<DocSearchRequest>, JsonRejection>,
) -> Result<Json<DocSearchReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: query"))?;
    if req.query.trim().is_empty() {
        return Err(ApiError::bad_request("인자 오류: query"));
    }
    Ok(Json(search(&state, sid.as_str(), &req).await))
}

pub async fn research_quotes(body: Result<Json<QuotesRequest>, JsonRejection>) -> Result<Json<QuotesReply>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: items"))?;
    let reply = match_quotes(&req);
    tracing::info!(items = req.items.len(), matched = reply.matched.iter().filter(|m| **m).count(), "research.quotes");
    Ok(Json(reply))
}

pub async fn research_judge(
    State(state): State<AppState>,
    body: Result<Json<JudgeRequest>, JsonRejection>,
) -> Result<Json<JudgeReply>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: issue·items"))?;
    let reply = match state.engine.as_ref() {
        Some(parts) if state.cfg.research_judge => judge(&parts.llm, &req).await,
        _ => unjudged(&req),
    };
    tracing::info!(items = req.items.len(), kept = reply.keep.len(), judged = reply.judged, "research.judge");
    Ok(Json(reply))
}

pub async fn research_review(
    State(state): State<AppState>,
    body: Result<Json<ReviewRequest>, JsonRejection>,
) -> Result<Json<ReviewReply>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: request_id·thread_id"))?;
    let reply = review(&state, &req).await;
    tracing::info!(searched = req.searched, doc_lines = req.doc_lines.len(), sent_back = reply.note.is_some(), "research.review");
    Ok(Json(reply))
}

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
) -> Result<Json<StepReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: request_id·question"))?;
    let notes = notes_for(&req.question, state.holdings.get());
    let mut ctx = Context::new(&req, &state.reads.snapshot(sid.as_str()));
    ctx.scope = scope_for(&notes);
    let read_kinds: Vec<String> = ctx.read.iter().filter_map(|l| law_of_label(l)).filter_map(|law| state.known.kind_of(law)).collect();
    if let Some(text) = req.answer.as_deref() {
        let ask = UnreadRequest { thread_id: sid.as_str().to_string(), text: text.to_string() };
        let found = unread_in_corpus(&state, &ask).await;
        let step = state.steps.after_answer(sid.as_str(), &req, &found.unread, &found.uncited, found.cites, &ctx);
        tracing::info!(
            unread = ?found.unread,
            uncited = found.uncited.len(),
            cites = found.cites,
            reread = matches!(step, Step::Task { .. }),
            "next_step.after_answer"
        );
        let guidance = guidance_for(&req.question, read_kinds.iter().map(String::as_str));
        return Ok(Json(StepReply { step, notes, guidance }));
    }
    let db = state.db.clone();
    let session = sid.as_str().to_string();
    let docs = tokio::task::spawn_blocking(move || session_docs(&db, &session)).await.unwrap_or_default();
    let mut legal = true;
    if state.cfg.route_legal && keyword(&req.question).is_some() && state.steps.new_turn(sid.as_str(), &req.request_id) {
        if asks_for_law(&req.question) {
            tracing::info!(legal, by = "word", history = ctx.history.len(), "route.legal");
        } else if asks_for_format(&req.question) {
            legal = false;
            tracing::info!(legal, by = "word", history = ctx.history.len(), "route.legal");
        } else if let Some(parts) = state.engine.as_ref() {
            legal = is_legal(&parts.llm, &req.question, !docs.is_empty(), &ctx.history, ctx.last_answer.as_deref()).await.unwrap_or(true);
            tracing::info!(legal, by = "model", history = ctx.history.len(), "route.legal");
        }
    }
    let mut laws = Vec::new();
    for name in question_laws(&req.question) {
        if state.known.in_corpus(state.corpus.as_ref(), &name).await {
            laws.push(name);
        }
    }
    state.coverage.require(&req.request_id, &laws);
    let step = state.steps.next(sid.as_str(), &req, legal, &docs, &laws, &ctx);
    let mut kinds = read_kinds;
    kinds.extend(laws.iter().filter_map(|law| state.known.kind_of(law)));
    let guidance = guidance_for(&req.question, kinds.iter().map(String::as_str));
    if let Step::Task { description, .. } = &step {
        tracing::info!(
            history = ctx.history.len(),
            read = ctx.read.len(),
            notes = notes.len(),
            guidance = guidance.len(),
            chars = description.chars().count(),
            "next_step.task"
        );
    }
    Ok(Json(StepReply { step, notes, guidance }))
}

pub async fn answer_check(
    State(state): State<AppState>,
    body: Result<Json<CheckRequest>, JsonRejection>,
) -> Result<Json<CheckReply>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: thread_id·sentences"))?;
    if req.sentences.len() > MAX_SENTENCES {
        return Err(ApiError::bad_request("인자 오류: 문장이 너무 많습니다"));
    }
    Ok(Json(check_sentences(&state, &req).await))
}

pub async fn answer_unread(
    State(state): State<AppState>,
    body: Result<Json<UnreadRequest>, JsonRejection>,
) -> Result<Json<UnreadReply>, ApiError> {
    let Json(req) = body.map_err(|_| ApiError::bad_request("인자 오류: thread_id·text"))?;
    Ok(Json(unread_in_corpus(&state, &req).await))
}
