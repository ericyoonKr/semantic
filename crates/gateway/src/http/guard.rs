use axum::extract::FromRequestParts;
use axum::http::Method;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::auth::token::{read_cookie, same, token_hash};
use crate::error::GatewayError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct Signed {
    pub user_id: Uuid,
    pub token: String,
}

impl FromRequestParts<AppState> for Signed {
    type Rejection = GatewayError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let cookie = parts.headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok()).unwrap_or("");
        let token = read_cookie(cookie).ok_or(GatewayError::Unauthenticated)?.to_string();
        let active = state.store.session(&token).await?;
        if parts.method != Method::GET && parts.method != Method::HEAD {
            let sent = parts.headers.get("x-csrf-token").and_then(|v| v.to_str().ok()).ok_or(GatewayError::Csrf)?;
            if !same(&token_hash(sent), &active.csrf_token_hash) {
                return Err(GatewayError::Csrf);
            }
        }
        Ok(Signed { user_id: active.user_id, token })
    }
}

#[derive(Debug, Clone)]
pub struct Master {
    pub signed: Signed,
}

impl FromRequestParts<AppState> for Master {
    type Rejection = GatewayError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let signed = Signed::from_request_parts(parts, state).await?;
        if !state.store.is_master(signed.user_id).await? {
            return Err(GatewayError::Forbidden);
        }
        Ok(Master { signed })
    }
}

pub fn thread_id(raw: &str) -> Result<Uuid, GatewayError> {
    Uuid::parse_str(raw).map_err(|_| GatewayError::NotFound)
}
