use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use chrono::NaiveDate;
use retrieval::{AnnexCompute, AnnexLookup, AnnexTable, Computation, LOOKUP_MAX};
use serde::Deserialize;

use crate::error::ApiError;
use crate::http::search::{corpus, json_body, run};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LookupRequest {
    pub law: String,
    pub annex: String,
    pub as_of: Option<NaiveDate>,
    pub row: Option<String>,
    pub words: Option<String>,
    pub column: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ComputeRequest {
    pub node_id: String,
    pub column: Option<String>,
    pub elapsed: Option<i64>,
}

fn present(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn required(v: &str, name: &str) -> Result<String, ApiError> {
    let t = v.trim();
    if t.is_empty() {
        return Err(ApiError::bad_request(format!("인자 오류: {name}가 비어 있습니다")));
    }
    Ok(t.to_string())
}

pub async fn lookup(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: Result<Json<LookupRequest>, JsonRejection>,
) -> Result<Json<AnnexTable>, ApiError> {
    let req = json_body(body)?;
    let limit = match req.limit {
        None => 0,
        Some(n @ 1..=LOOKUP_MAX) => n,
        Some(_) => {
            return Err(ApiError::bad_request(format!("인자 오류: limit는 1부터 {LOOKUP_MAX}까지")));
        }
    };
    let q = AnnexLookup {
        law: required(&req.law, "law")?,
        annex: required(&req.annex, "annex")?,
        as_of: req.as_of,
        row: present(req.row),
        words: present(req.words),
        column: present(req.column),
        limit,
    };
    let c = corpus(&state)?;
    let out = run(&state, retrieval::annex_lookup(c.pool(), &q)).await?;
    state.coverage.law(&headers);
    if let Some(thread) = crate::funcall::readlog::thread_of(&headers) {
        state.reads.record(&thread, |r| r.add_unit(&out.annex.law_name, &out.annex.annex_label));
    }
    Ok(out)
}

pub async fn compute(
    State(state): State<AppState>,
    body: Result<Json<ComputeRequest>, JsonRejection>,
) -> Result<Json<Computation>, ApiError> {
    let req = json_body(body)?;
    if req.elapsed.is_some_and(|e| e < 0) {
        return Err(ApiError::bad_request("인자 오류: elapsed는 0 이상"));
    }
    let q = AnnexCompute {
        node_id: required(&req.node_id, "node_id")?,
        column: present(req.column),
        elapsed: req.elapsed,
    };
    let c = corpus(&state)?;
    run(&state, retrieval::annex_compute(c.pool(), &q)).await
}
