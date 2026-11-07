use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use kg::{Direction, FindBy, MAX_LIMIT, NodeHit, ShortIds};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::kgids;
use crate::kgview::{ExpandView, NodeView, expand_view, node_view};
use crate::session::SessionId;
use crate::state::AppState;
use crate::tasks;

#[derive(Debug, Deserialize)]
pub struct FindRequest {
    pub graph: String,
    pub text: Option<String>,
    pub law_key: Option<String>,
    pub law: Option<String>,
    pub unit: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ExpandRequest {
    pub graph: String,
    pub node: String,
    pub direction: Direction,
    pub relation: Option<String>,
    pub depth: Option<u8>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct FindReply {
    pub graph: String,
    pub nodes: Vec<NodeView>,
}

#[derive(Debug, Serialize)]
pub struct ExpandReply {
    pub graph: String,
    #[serde(flatten)]
    pub view: ExpandView,
}

fn present(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

async fn with_deadline<T>(
    limit: Duration,
    work: impl Future<Output = Result<T, ApiError>>,
) -> Result<T, ApiError> {
    tokio::time::timeout(limit, work)
        .await
        .unwrap_or_else(|_| Err(ApiError::timeout()))
}

async fn record<T: Send + 'static>(
    state: &AppState,
    sid: &SessionId,
    build: impl FnOnce(&mut ShortIds) -> T + Send + 'static,
) -> Result<T, ApiError> {
    let _guard = state.kg_ids.lock(sid).await;
    let db = state.db.clone();
    let sid = sid.clone();
    tasks::blocking(move || {
        let mut ids = kgids::load(&db, &sid)?;
        let out = build(&mut ids);
        kgids::save(&db, &sid, &ids)?;
        Ok(out)
    })
    .await?
}

pub async fn find(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(req): Json<FindRequest>,
) -> Result<Json<FindReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let limit = req.limit.unwrap_or(MAX_LIMIT);
    let text = present(&req.text).map(str::to_string);
    let law_key = present(&req.law_key).map(str::to_string);
    if text.is_none() && law_key.is_none() {
        return Err(
            if present(&req.law).is_some() || present(&req.unit).is_some() {
                ApiError::bad_request("law와 unit으로 찾으려면 법령 판본 키(law_key)가 필요합니다")
            } else {
                ApiError::bad_request("인자 오류: text 또는 law_key가 필요합니다")
            },
        );
    }
    let kg = state.kg.clone();
    let graph = req.graph.clone();
    let kind = present(&req.kind).map(str::to_string);
    let work = async {
        let hits: Vec<NodeHit> = tasks::blocking(move || {
            let by = match (&text, &law_key) {
                (Some(t), _) => FindBy::Text(t),
                (None, Some(k)) => FindBy::LawKey(k),
                (None, None) => return Err(kg::KgError::Argument("text")),
            };
            kg.find(&graph, by, kind.as_deref(), limit)
        })
        .await??;
        let graph = req.graph.clone();
        record(&state, &sid, move |ids| {
            hits.iter().map(|h| node_view(&graph, h, ids)).collect()
        })
        .await
    };
    let nodes = with_deadline(state.cfg.limits.kg_deadline, work).await?;
    Ok(Json(FindReply {
        graph: req.graph,
        nodes,
    }))
}

pub async fn expand(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(req): Json<ExpandRequest>,
) -> Result<Json<ExpandReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let db = state.db.clone();
    let lookup_sid = sid.clone();
    let ids = tasks::blocking(move || kgids::load(&db, &lookup_sid)).await??;
    let target = ids
        .resolve(&req.node)
        .filter(|t| t.graph == req.graph)
        .cloned()
        .ok_or_else(|| {
            ApiError::bad_request("알 수 없는 노드 번호입니다 — kg_find로 먼저 찾아 주세요")
        })?;
    let kg = state.kg.clone();
    let graph = req.graph.clone();
    let relation = present(&req.relation).map(str::to_string);
    let direction = req.direction;
    let depth = req.depth.unwrap_or(1);
    let limit = req.limit.unwrap_or(MAX_LIMIT);
    let work = async {
        let ex = tasks::blocking(move || {
            kg.expand(
                &graph,
                &target.id,
                relation.as_deref(),
                direction,
                depth,
                limit,
            )
        })
        .await??;
        let graph = req.graph.clone();
        record(&state, &sid, move |ids| {
            expand_view(&graph, &ex, direction, ids)
        })
        .await
    };
    let view = with_deadline(state.cfg.limits.kg_deadline, work).await?;
    Ok(Json(ExpandReply {
        graph: req.graph,
        view,
    }))
}
