use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::auth::token::{clear_cookie, session_cookie};
use crate::error::GatewayError;
use crate::http::guard::Signed;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginBody {
    login_id: String,
    password: String,
}

fn with_cookie(mut res: Response, cookie: String) -> Result<Response, GatewayError> {
    let value = HeaderValue::from_str(&cookie).map_err(|_| GatewayError::Internal)?;
    res.headers_mut().insert(SET_COOKIE, value);
    Ok(res)
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginBody>,
) -> Result<Response, GatewayError> {
    if !state.limiter.allow(addr.ip()) {
        return Err(GatewayError::TooMany);
    }
    let session = match state.store.login(&body.login_id, &body.password).await {
        Ok(s) => s,
        Err(e @ (GatewayError::LoginFailed | GatewayError::Locked)) => {
            tracing::info!(kind = %e, "auth.login_failed");
            return Err(GatewayError::LoginFailed);
        }
        Err(e) => return Err(e),
    };
    tracing::info!("auth.login");
    let res = Json(json!({ "csrf_token": session.csrf_token })).into_response();
    with_cookie(res, session_cookie(&session.token, 12 * 60 * 60))
}

pub async fn logout(State(state): State<AppState>, signed: Signed) -> Result<Response, GatewayError> {
    state.store.logout(&signed.token).await?;
    with_cookie(StatusCode::NO_CONTENT.into_response(), clear_cookie())
}

pub async fn me(State(state): State<AppState>, signed: Signed) -> Result<Json<serde_json::Value>, GatewayError> {
    let login_id = state.store.login_id_of(signed.user_id).await?;
    Ok(Json(json!({ "login_id": login_id })))
}
