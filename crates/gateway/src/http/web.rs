use std::path::PathBuf;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

async fn no_source_maps(req: Request, next: Next) -> Response {
    if req.uri().path().ends_with(".map") {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(req).await
}

pub fn routes(dir: PathBuf) -> Router<AppState> {
    let index = dir.join("index.html");
    Router::new()
        .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
        .layer(middleware::from_fn(no_source_maps))
}
