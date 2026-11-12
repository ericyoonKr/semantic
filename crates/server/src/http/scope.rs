use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::http::search::{corpus, json_body, run};
use crate::state::AppState;

const MAX_NAMES: usize = 10;

#[derive(Debug, Deserialize)]
pub struct LawScopeRequest {
    pub names: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LawScope {
    pub law_ids: Vec<String>,
}

fn names(raw: Vec<String>) -> Result<Vec<String>, ApiError> {
    let names: Vec<String> = raw.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if names.is_empty() || names.len() > MAX_NAMES {
        return Err(ApiError::bad_request(format!("인자 오류: names는 1개부터 {MAX_NAMES}개까지")));
    }
    Ok(names)
}

pub async fn law_scope(
    State(state): State<AppState>,
    body: Result<Json<LawScopeRequest>, JsonRejection>,
) -> Result<Json<LawScope>, ApiError> {
    let names = names(json_body(body)?.names)?;
    let c = corpus(&state)?;
    run(&state, async { Ok(LawScope { law_ids: retrieval::named_laws(c.pool(), &names).await? }) }).await
}
