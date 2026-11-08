use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use chrono::NaiveDate;
use retrieval::{
    CaseDoc, CaseHit, CaseQuery, Citations, CitationsBy, Corpus, LawHit, LawQuery, LawUnit,
    RetrievalError, SearchResult,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

const DEFAULT_K: usize = 8;
const MAX_K: usize = 80;

#[derive(Debug, Deserialize)]
pub struct LawSearchRequest {
    pub query: String,
    pub k: Option<usize>,
    pub as_of: Option<NaiveDate>,
    pub law_type: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct CaseSearchRequest {
    pub query: String,
    pub k: Option<usize>,
    pub field: Option<Vec<String>>,
    pub court_level: Option<Vec<String>>,
    pub decided_from: Option<NaiveDate>,
    pub decided_to: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct UnitParams {
    pub law: String,
    pub unit: String,
    pub as_of: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct CaseParams {
    pub sections: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CitationParams {
    pub decision_id: Option<String>,
    pub law: Option<String>,
    pub unit: Option<String>,
}

pub(crate) fn json_body<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    body.map(|Json(v)| v)
        .map_err(|e| ApiError::bad_request(format!("인자 오류: {}", e.body_text())))
}

fn query_params<T>(q: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    q.map(|Query(v)| v)
        .map_err(|e| ApiError::bad_request(format!("인자 오류: {}", e.body_text())))
}

pub(crate) fn corpus(state: &AppState) -> Result<&Corpus, ApiError> {
    state.corpus.as_ref().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "검색 실패: 법령·판례 DB가 설정되지 않았습니다",
        )
    })
}

pub(crate) fn api_error(e: RetrievalError) -> ApiError {
    match e {
        RetrievalError::Argument(what) => ApiError::bad_request(format!("인자 오류: {what}")),
        RetrievalError::NotFound("law unit") => ApiError::bad_request("그 조문을 찾지 못했습니다"),
        RetrievalError::NotFound("decision") => ApiError::bad_request("그 판례를 찾지 못했습니다"),
        RetrievalError::NotFound("annex") => ApiError::bad_request("그 기준일에 그 별표를 찾지 못했습니다"),
        RetrievalError::NotFound("annex row") => ApiError::bad_request("그 별표 행을 찾지 못했습니다"),
        RetrievalError::NotFound(_) => ApiError::bad_request("찾지 못했습니다"),
        other => {
            tracing::error!(error = %other, "corpus query failed");
            ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "검색 실패: 법령·판례 DB 오류",
            )
        }
    }
}

pub(crate) async fn run<T>(
    state: &AppState,
    work: impl Future<Output = Result<T, RetrievalError>>,
) -> Result<Json<T>, ApiError> {
    match tokio::time::timeout(state.cfg.corpus.search_deadline, work).await {
        Ok(Ok(v)) => Ok(Json(v)),
        Ok(Err(e)) => Err(api_error(e)),
        Err(_) => Err(ApiError::timeout()),
    }
}

fn top_k(k: Option<usize>) -> Result<usize, ApiError> {
    match k.unwrap_or(DEFAULT_K) {
        n @ 1..=MAX_K => Ok(n),
        _ => Err(ApiError::bad_request(format!("인자 오류: k는 1부터 {MAX_K}까지")))
    }
}

fn query_text(q: &str) -> Result<String, ApiError> {
    let t = q.trim();
    if t.is_empty() {
        return Err(ApiError::bad_request("인자 오류: query가 비어 있습니다"));
    }
    Ok(t.to_string())
}

fn list(v: Option<Vec<String>>) -> Option<Vec<String>> {
    let items: Vec<String> = v
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    (!items.is_empty()).then_some(items)
}

fn present(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub async fn search_laws(
    State(state): State<AppState>,
    body: Result<Json<LawSearchRequest>, JsonRejection>,
) -> Result<Json<SearchResult<LawHit>>, ApiError> {
    let req = json_body(body)?;
    let q = LawQuery {
        text: query_text(&req.query)?,
        k: top_k(req.k)?,
        as_of: req.as_of,
        law_type: list(req.law_type),
    };
    let c = corpus(&state)?;
    run(&state, retrieval::search_laws(c, &q)).await
}

pub async fn search_cases(
    State(state): State<AppState>,
    body: Result<Json<CaseSearchRequest>, JsonRejection>,
) -> Result<Json<SearchResult<CaseHit>>, ApiError> {
    let req = json_body(body)?;
    let q = CaseQuery {
        text: query_text(&req.query)?,
        k: top_k(req.k)?,
        field: list(req.field),
        court_level: list(req.court_level),
        decided_from: req.decided_from,
        decided_to: req.decided_to,
    };
    let c = corpus(&state)?;
    run(&state, retrieval::search_cases(c, &q)).await
}

pub async fn law_unit(
    State(state): State<AppState>,
    params: Result<Query<UnitParams>, QueryRejection>,
) -> Result<Json<LawUnit>, ApiError> {
    let p = query_params(params)?;
    let c = corpus(&state)?;
    run(&state, retrieval::read_law_unit(c.pool(), &p.law, &p.unit, p.as_of)).await
}

fn decision_id(raw: &str) -> Result<i64, ApiError> {
    raw.trim()
        .parse()
        .map_err(|_| ApiError::bad_request("인자 오류: decision_id"))
}

pub async fn case(
    State(state): State<AppState>,
    Path(id): Path<String>,
    params: Result<Query<CaseParams>, QueryRejection>,
) -> Result<Json<CaseDoc>, ApiError> {
    let id = decision_id(&id)?;
    let p = query_params(params)?;
    let sections = list(
        p.sections
            .map(|s| s.split(',').map(str::to_string).collect()),
    );
    let c = corpus(&state)?;
    run(&state, retrieval::read_case(c.pool(), id, sections.as_deref())).await
}

pub async fn citations(
    State(state): State<AppState>,
    params: Result<Query<CitationParams>, QueryRejection>,
) -> Result<Json<Citations>, ApiError> {
    let p = query_params(params)?;
    let by = match (present(p.decision_id), present(p.law), present(p.unit)) {
        (Some(id), _, _) => CitationsBy::Decision(decision_id(&id)?),
        (None, Some(law), Some(unit)) => CitationsBy::LawUnit { law, unit },
        _ => {
            return Err(ApiError::bad_request(
                "인자 오류: decision_id 또는 law와 unit이 필요합니다",
            ));
        }
    };
    let c = corpus(&state)?;
    run(&state, retrieval::citations(c.pool(), &by, state.cfg.corpus.citations_max)).await
}
