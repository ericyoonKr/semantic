use axum::Json;
use axum::extract::State;

use crate::rules::CatalogItem;
use crate::state::AppState;

pub async fn catalog(State(state): State<AppState>) -> Json<Vec<CatalogItem>> {
    Json(state.rules.catalog().to_vec())
}
