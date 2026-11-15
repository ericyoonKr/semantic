pub mod admin;
pub mod auth;
pub mod chat;
pub mod files;
pub mod guard;
pub mod sides;
pub mod web;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue};
use axum::routing::{delete, get, post};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::state::AppState;

fn header(name: &'static str, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(HeaderName::from_static(name), HeaderValue::from_static(value))
}

fn default_header(name: &'static str, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(HeaderName::from_static(name), HeaderValue::from_static(value))
}

pub fn router(state: AppState) -> Router {
    let limit = state.cfg.upload_limit_bytes;
    let api = Router::new()
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/signup", post(auth::signup))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/admin/users", get(admin::list))
        .route("/api/admin/users/{login_id}/approve", post(admin::approve))
        .route("/api/admin/users/{login_id}/reject", post(admin::reject))
        .route("/api/admin/users/{login_id}/disable", post(admin::disable))
        .route("/api/sessions", get(chat::list))
        .route("/session", post(chat::create))
        .route("/session/{id}/ask", post(chat::ask))
        .route("/session/{id}/retry", post(chat::retry))
        .route("/session/{id}/upload", post(chat::upload))
        .route("/session/{id}/history", get(chat::history))
        .route("/session/{id}/live", get(chat::live))
        .route("/session/{id}/files", get(chat::files))
        .route("/session/{id}/files/{file_id}", get(chat::file_text))
        .route("/session/{id}/files/{file_id}/original", get(files::original))
        .route("/session/{id}", delete(chat::remove).patch(chat::rename))
        .route("/prefs", delete(chat::remove_prefs))
        .layer(DefaultBodyLimit::max(limit));
    let app = match state.cfg.web_dir.clone() {
        Some(dir) => api.merge(web::routes(dir)),
        None => api,
    };
    app.with_state(state)
        .layer(CatchPanicLayer::new())
        .layer(header("x-content-type-options", "nosniff"))
        .layer(header("referrer-policy", "no-referrer"))
        .layer(default_header("x-frame-options", "DENY"))
        .layer(default_header("content-security-policy", "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; frame-ancestors 'none'"))
}
