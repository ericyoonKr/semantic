pub mod annex;
pub mod catalog;
pub mod files;
pub mod funcall;
pub mod health;
pub mod judge;
pub mod kg;
pub mod kgstate;
pub mod kgtree;
pub mod middleware;
pub mod rules;
pub mod scope;
pub mod search;
pub mod sessions;
pub mod versions;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{delete, get, post};
use tower_http::catch_panic::CatchPanicLayer;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let limits = &state.cfg.limits;
    let upload_limit = DefaultBodyLimit::max(limits.upload_body_bytes);
    Router::new()
        .route("/health", get(health::health))
        .route(
            "/sessions/{sid}/files",
            post(files::upload).layer(upload_limit).get(files::list),
        )
        .route(
            "/sessions/{sid}/files/{file_id}/original",
            get(files::original),
        )
        .route("/sessions/{sid}", delete(sessions::delete))
        .route("/kg/catalog", get(catalog::kg_catalog))
        .route("/sessions/{sid}/kg/find", post(kg::find))
        .route("/sessions/{sid}/kg/expand", post(kg::expand))
        .route("/sessions/{sid}/kg/state-check", post(kgstate::check))
        .route("/sessions/{sid}/kg/tree", post(kgtree::judge_tree))
        .route("/catalog", get(rules::catalog))
        .route("/sessions/{sid}/judge", post(judge::judge))
        .route("/search/laws", post(search::search_laws))
        .route("/search/cases", post(search::search_cases))
        .route("/laws/unit", get(search::law_unit))
        .route("/laws/annexes", get(search::law_annexes))
        .route("/answer/check", post(funcall::answer_check))
        .route("/sessions/{sid}/next-step", post(funcall::next_step))
        .route("/laws/scope", post(scope::law_scope))
        .route("/cases/{decision_id}", get(search::case))
        .route("/citations", get(search::citations))
        .route("/versions/resolve", post(versions::resolve))
        .route("/versions/citations", post(versions::citations))
        .route("/versions/trace", post(versions::trace))
        .route("/annex/lookup", post(annex::lookup))
        .route("/annex/compute", post(annex::compute))
        .layer(DefaultBodyLimit::max(limits.json_body_bytes))
        .layer(from_fn_with_state(state.clone(), middleware::internal_key))
        .layer(CatchPanicLayer::custom(middleware::panic_response))
        .layer(from_fn(middleware::request_id))
        .layer(from_fn(middleware::strip_server_headers))
        .with_state(state)
}
