use axum::Json;
use axum::extract::State;
use kg::{DisabledBundle, StateSpec};
use serde::Serialize;
use serde_json::Value;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct GraphLine {
    pub graph: String,
    pub shape: String,
    pub nodes: u64,
    pub edges: u64,
    pub node_kinds: Vec<String>,
    pub relations: Vec<String>,
    pub compositions: u64,
    pub composition_mode: Option<String>,
    pub state: Option<StateSpec>,
    pub dated: u64,
    pub article_index: u64,
    pub sha256: String,
    pub measurement: Value,
}

#[derive(Debug, Serialize)]
pub struct KgCatalog {
    pub graphs: Vec<GraphLine>,
    pub disabled: Vec<DisabledBundle>,
}

pub async fn kg_catalog(State(state): State<AppState>) -> Json<KgCatalog> {
    let graphs = state
        .kg
        .catalog()
        .into_iter()
        .map(|e| GraphLine {
            graph: e.graph,
            shape: e.shape,
            nodes: e.nodes,
            edges: e.edges,
            node_kinds: e.node_kinds,
            relations: e.relations,
            compositions: e.compositions,
            composition_mode: e.composition_mode,
            state: e.state,
            dated: e.dated,
            article_index: e.article_index,
            sha256: e.sha256,
            measurement: e.measurement,
        })
        .collect();
    Json(KgCatalog {
        graphs,
        disabled: state.kg.disabled().to_vec(),
    })
}
