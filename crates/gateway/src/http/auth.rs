use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::audit::Event;
use crate::auth::token::{clear_cookie, session_cookie};
use crate::error::GatewayError;
use crate::http::guard::Signed;
use crate::state::AppState;
use crate::store::login::Attempt;

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
    let session = match state.store.attempt(&body.login_id, &body.password).await? {
        Attempt::Signed(s) => s,
        Attempt::Failed { user_id, locked_now } => {
            tracing::info!(locked_now, "auth.login_failed");
            let fail = Event::new("login_fail");
            state.audit.spawn(match user_id {
                Some(u) => fail.user(u),
                None => fail,
            });
            if let (Some(u), true) = (user_id, locked_now) {
                state.audit.spawn(Event::new("lock").user(u));
            }
            return Err(GatewayError::LoginFailed);
        }
        Attempt::Locked { user_id } => {
            tracing::info!("auth.login_while_locked");
            state.audit.spawn(Event::new("login_fail").user(user_id).detail(json!({ "while_locked": true })));
            return Err(GatewayError::LoginFailed);
        }
    };
    tracing::info!("auth.login");
    state.audit.spawn(Event::new("login").user(session.user_id));
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
