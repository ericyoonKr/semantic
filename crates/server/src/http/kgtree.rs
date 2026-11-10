use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, State};
use kg::{Truth, judge};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::http::kg::{record, resolve};
use crate::session::SessionId;
use crate::state::AppState;
use crate::tasks;

const MAX_TREE_NODES: usize = 500;

#[derive(Debug, Deserialize)]
pub struct TreeRequest {
    pub graph: String,
    pub root: String,
    #[serde(default)]
    pub states: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct TreeNodeView {
    pub node: String,
    pub name: String,
    pub value: &'static str,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TreeReply {
    pub graph: String,
    pub root: String,
    pub value: &'static str,
    pub nodes: Vec<TreeNodeView>,
    pub open_leaves: Vec<TreeNodeView>,
    pub ignored_states: Vec<String>,
}

pub async fn judge_tree(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    Json(req): Json<TreeRequest>,
) -> Result<Json<TreeReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let root = resolve(&state, &sid, &req.graph, &req.root).await?;
    let mut states = BTreeMap::new();
    let mut ignored = Vec::new();
    for (short, value) in &req.states {
        let truth = Truth::parse(value);
        match (resolve(&state, &sid, &req.graph, short).await, truth) {
            (Ok(target), Some(t)) => {
                states.insert(target.id, t);
            }
            _ => ignored.push(short.clone()),
        }
    }
    ignored.sort();
    let kg = state.kg.clone();
    let graph = req.graph.clone();
    let root_id = root.id.clone();
    let (tree, outcome) = tasks::blocking(move || {
        let tree = kg.tree(&graph, &root_id, MAX_TREE_NODES)?;
        let outcome = judge(&tree, &states);
        Ok::<_, kg::KgError>((tree, outcome))
    })
    .await??;
    let root_value = outcome.value.as_str();
    let sha = state.kg.sha256(&req.graph).unwrap_or_default().to_string();
    let graph = req.graph.clone();
    let (nodes, open_leaves) = record(&state, &sid, move |ids| {
        let mut short = |id: &str| ids.assign(&graph, id, &sha);
        let nodes: Vec<TreeNodeView> = outcome
            .nodes
            .iter()
            .map(|n| TreeNodeView {
                node: short(&n.id),
                name: n.name.clone(),
                value: n.value.as_str(),
                blocked_by: n.blocked_by.iter().map(|b| short(b)).collect(),
            })
            .collect();
        let open: Vec<TreeNodeView> = outcome
            .open_leaves
            .iter()
            .filter_map(|id| tree.nodes.get(id))
            .map(|n| TreeNodeView {
                node: short(&n.id),
                name: n.name.clone(),
                value: Truth::Unknown.as_str(),
                blocked_by: Vec::new(),
            })
            .collect();
        (nodes, open)
    })
    .await?;
    Ok(Json(TreeReply {
        graph: req.graph,
        root: req.root,
        value: root_value,
        nodes,
        open_leaves,
        ignored_states: ignored,
    }))
}
