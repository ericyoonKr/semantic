use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use kg::{
    Direction, ExpandQuery, FindBy, FindQuery, MAX_LIMIT, Reason, ShortIds, ShortTarget, Status,
};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::kgids;
use crate::kgview::{ExpandView, FindView, GraphRef, Outcome, expand_view, find_view, stale_text};
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
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExpandRequest {
    pub graph: String,
    pub node: String,
    pub direction: Direction,
    pub relation: Option<String>,
    pub depth: Option<u8>,
    pub limit: Option<usize>,
    pub as_of: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FindReply {
    pub graph: String,
    #[serde(flatten)]
    pub view: FindView,
}

#[derive(Debug, Serialize)]
pub struct ExpandReply {
    pub graph: String,
    #[serde(flatten)]
    pub view: ExpandView,
}

pub(crate) fn present(v: &Option<String>) -> Option<&str> {
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

pub(crate) async fn resolve(
    state: &AppState,
    sid: &SessionId,
    graph: &str,
    node: &str,
) -> Result<ShortTarget, ApiError> {
    let db = state.db.clone();
    let lookup_sid = sid.clone();
    let ids = tasks::blocking(move || kgids::load(&db, &lookup_sid)).await??;
    ids.resolve(node)
        .filter(|t| t.graph == graph)
        .cloned()
        .ok_or_else(|| {
            ApiError::bad_request("알 수 없는 노드 번호입니다 — kg_find로 먼저 찾아 주세요")
        })
}

fn sha_of(state: &AppState, graph: &str) -> String {
    state.kg.sha256(graph).unwrap_or_default().to_string()
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
    let law = present(&req.law).map(str::to_string);
    let unit = present(&req.unit).map(str::to_string);
    if text.is_none() && law_key.is_none() && (law.is_none() || unit.is_none()) {
        return Err(ApiError::bad_request(
            "인자 오류: text, law_key, 또는 law와 unit이 필요합니다",
        ));
    }
    let kg = state.kg.clone();
    let graph = req.graph.clone();
    let kind = present(&req.kind).map(str::to_string);
    let as_of = present(&req.as_of).map(str::to_string);
    let sha = sha_of(&state, &req.graph);
    let work = async {
        let found = tasks::blocking(move || {
            let by = match (&text, &law_key, &law, &unit) {
                (Some(t), _, _, _) => FindBy::Text(t),
                (None, Some(k), _, _) => FindBy::LawKey(k),
                (None, None, Some(l), Some(u)) => FindBy::Article { law: l, unit: u },
                _ => return Err(kg::KgError::Argument("text")),
            };
            let q = FindQuery {
                by,
                kind: kind.as_deref(),
                limit,
                as_of: as_of.as_deref(),
            };
            kg.find(&graph, &q)
        })
        .await??;
        let graph = req.graph.clone();
        record(&state, &sid, move |ids| {
            let g = GraphRef {
                name: &graph,
                sha256: &sha,
            };
            find_view(&g, &found, ids)
        })
        .await
    };
    let view = with_deadline(state.cfg.limits.kg_deadline, work).await?;
    Ok(Json(FindReply {
        graph: req.graph,
        view,
    }))
}

fn stale_view(node: &str, direction: Direction) -> ExpandView {
    ExpandView {
        outcome: Outcome {
            status: Status::Unknown,
            reason: Some(stale_text().to_string()),
            basis: Vec::new(),
            as_of: None,
        },
        origin: node.to_string(),
        direction,
        composition_mode: None,
        refused_paths: 0,
        edges: Vec::new(),
    }
}

pub async fn expand(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(req): Json<ExpandRequest>,
) -> Result<Json<ExpandReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let target = resolve(&state, &sid, &req.graph, &req.node).await?;
    let kg = state.kg.clone();
    let graph = req.graph.clone();
    let relation = present(&req.relation).map(str::to_string);
    let as_of = present(&req.as_of).map(str::to_string);
    let direction = req.direction;
    let depth = req.depth.unwrap_or(1);
    let limit = req.limit.unwrap_or(MAX_LIMIT);
    let sha = sha_of(&state, &req.graph);
    let changed = target.sha256 != sha;
    let node = req.node.clone();
    let work = async {
        let ex = tasks::blocking(move || {
            let q = ExpandQuery {
                relation: relation.as_deref(),
                direction,
                depth,
                limit,
                as_of: as_of.as_deref(),
            };
            kg.expand(&graph, &target.id, &q)
        })
        .await??;
        if changed && ex.reason == Some(Reason::NodeNotInGraph) {
            return Ok(stale_view(&node, direction));
        }
        let graph = req.graph.clone();
        record(&state, &sid, move |ids| {
            let g = GraphRef {
                name: &graph,
                sha256: &sha,
            };
            expand_view(&g, &ex, direction, ids)
        })
        .await
    };
    let view = with_deadline(state.cfg.limits.kg_deadline, work).await?;
    Ok(Json(ExpandReply {
        graph: req.graph,
        view,
    }))
}
