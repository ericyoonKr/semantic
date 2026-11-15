use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::audit::Event;
use crate::auth::password::{HashCost, check_signup_policy};
use crate::auth::token::{clear_cookie, session_cookie};
use crate::error::GatewayError;
use crate::http::guard::Signed;
use crate::state::AppState;
use crate::store::login::Attempt;

pub const LOGIN_ID_MIN_CHARS: usize = 4;
pub const LOGIN_ID_MAX_CHARS: usize = 32;
pub const NAME_MAX_CHARS: usize = 40;

#[derive(Deserialize)]
pub struct LoginBody {
    login_id: String,
    password: String,
}

#[derive(Deserialize)]
pub struct SignupBody {
    login_id: String,
    password: String,
    name: String,
}

pub fn valid_login_id(id: &str) -> bool {
    let count = id.chars().count();
    (LOGIN_ID_MIN_CHARS..=LOGIN_ID_MAX_CHARS).contains(&count)
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
}

pub fn valid_name(name: &str) -> bool {
    let count = name.chars().count();
    (1..=NAME_MAX_CHARS).contains(&count) && !name.chars().any(char::is_control)
}

pub async fn signup(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<SignupBody>,
) -> Result<Response, GatewayError> {
    if !state.signup_limiter.allow(addr.ip()) {
        return Err(GatewayError::TooMany);
    }
    if !valid_login_id(&body.login_id) {
        return Err(GatewayError::Invalid("아이디는 4~32자의 영문 소문자, 숫자, _ . - 만 쓸 수 있습니다"));
    }
    check_signup_policy(&body.password)?;
    let name = body.name.trim();
    if !valid_name(name) {
        return Err(GatewayError::Invalid("이름은 1~40자여야 합니다"));
    }
    let user_id = state.store.signup_user(&body.login_id, name, &body.password, HashCost::default()).await?;
    tracing::info!("auth.signup");
    state.audit.spawn(Event::new("signup").user(user_id));
    Ok((StatusCode::CREATED, Json(json!({ "login_id": body.login_id, "status": "pending" }))).into_response())
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
        Attempt::Pending { user_id } => {
            tracing::info!("auth.login_while_pending");
            state.audit.spawn(Event::new("login_fail").user(user_id).detail(json!({ "pending": true })));
            return Err(GatewayError::Pending);
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
    let profile = state.store.profile(signed.user_id).await?;
    Ok(Json(json!({ "login_id": profile.login_id, "name": profile.display_name, "is_master": profile.is_master })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_id_rule() {
        assert!(valid_login_id("abcd"));
        assert!(valid_login_id("user.name_01-x"));
        assert!(!valid_login_id("abc"));
        assert!(!valid_login_id("Upper"));
        assert!(!valid_login_id("한글아이디"));
        assert!(!valid_login_id(&"a".repeat(33)));
        assert!(valid_login_id(&"a".repeat(32)));
    }

    #[test]
    fn name_rule() {
        assert!(valid_name("홍길동"));
        assert!(!valid_name(""));
        assert!(!valid_name(&"가".repeat(41)));
        assert!(valid_name(&"가".repeat(40)));
        assert!(!valid_name("줄\n바꿈"));
    }
}
