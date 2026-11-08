use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::http::kg::resolve;
use crate::kgview::stale_text;
use crate::session::SessionId;
use crate::state::AppState;
use crate::tasks;

#[derive(Debug, Deserialize)]
pub struct StateCheckRequest {
    pub graph: String,
    pub node: String,
    pub value: String,
}

#[derive(Debug, Serialize, Default)]
pub struct StateCheckReply {
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub graph: String,
    pub node: String,
    pub name: String,
    pub sha256: String,
    pub state_kind: String,
    pub allowed: Vec<String>,
}

pub async fn check(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(req): Json<StateCheckRequest>,
) -> Result<Json<StateCheckReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let target = resolve(&state, &sid, &req.graph, &req.node).await?;
    let mut reply = StateCheckReply {
        graph: req.graph.clone(),
        node: req.node.clone(),
        sha256: state.kg.sha256(&req.graph).unwrap_or_default().to_string(),
        ..StateCheckReply::default()
    };
    let Some(spec) = state.kg.state_spec(&req.graph).cloned() else {
        reply.reason = Some("이 그래프는 노드에 적을 상태 값을 정하지 않았습니다".into());
        return Ok(Json(reply));
    };
    reply.state_kind = spec.kind.clone();
    reply.allowed = spec.values.clone();
    let kg = state.kg.clone();
    let graph = req.graph.clone();
    let id = target.id.clone();
    let hit = tasks::blocking(move || kg.node(&graph, &id)).await??;
    let Some(hit) = hit else {
        reply.reason = Some(stale_text().into());
        return Ok(Json(reply));
    };
    reply.name = hit.name;
    let value = req.value.trim();
    if !spec.values.iter().any(|v| v == value) {
        reply.reason = Some(format!(
            "허용된 값이 아닙니다 — 허용된 값: {}",
            spec.values.join(", ")
        ));
        return Ok(Json(reply));
    }
    reply.accepted = true;
    Ok(Json(reply))
}
