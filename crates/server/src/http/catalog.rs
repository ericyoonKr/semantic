use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::Value;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct GraphLine {
    pub graph: String,
    pub shape: String,
    pub nodes: u64,
    pub measurement: Value,
}

#[derive(Debug, Serialize)]
pub struct KgCatalog {
    pub graphs: Vec<GraphLine>,
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
            measurement: e.measurement,
        })
        .collect();
    Json(KgCatalog { graphs })
}
