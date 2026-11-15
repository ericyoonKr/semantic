use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit::Event;
use crate::error::GatewayError;
use crate::http::guard::Master;
use crate::state::AppState;
use crate::store::users::UserRow;

fn user_json(u: &UserRow) -> Value {
    json!({ "login_id": u.login_id, "name": u.display_name, "status": u.status.as_str(), "requested_at": u.requested_at })
}

pub async fn list(State(state): State<AppState>, _master: Master) -> Result<Json<Value>, GatewayError> {
    let users = state.store.list_users().await?;
    Ok(Json(Value::Array(users.iter().map(user_json).collect())))
}

async fn target(state: &AppState, master: &Master, login_id: &str, self_allowed: bool) -> Result<Uuid, GatewayError> {
    let id = state.store.user_id(login_id).await?.ok_or(GatewayError::NoSuchUser)?;
    if !self_allowed && id == master.signed.user_id {
        return Err(GatewayError::Invalid("마스터 자신은 끄거나 지울 수 없습니다"));
    }
    Ok(id)
}

fn record(state: &AppState, kind: &str, target: Uuid, master: &Master, login_id: &str) {
    tracing::info!(kind, "admin.user");
    state.audit.spawn(Event::new(kind).user(target).detail(json!({ "by": master.signed.user_id.to_string(), "login_id": login_id })));
}

pub async fn approve(State(state): State<AppState>, master: Master, Path(login_id): Path<String>) -> Result<StatusCode, GatewayError> {
    let id = target(&state, &master, &login_id, true).await?;
    if !state.store.approve_user(&login_id).await? {
        return Err(GatewayError::NoSuchUser);
    }
    record(&state, "approve", id, &master, &login_id);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn reject(State(state): State<AppState>, master: Master, Path(login_id): Path<String>) -> Result<StatusCode, GatewayError> {
    let id = target(&state, &master, &login_id, false).await?;
    if !state.store.reject_user(&login_id).await? {
        return Err(GatewayError::NoSuchUser);
    }
    record(&state, "reject", id, &master, &login_id);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn disable(State(state): State<AppState>, master: Master, Path(login_id): Path<String>) -> Result<StatusCode, GatewayError> {
    let id = target(&state, &master, &login_id, false).await?;
    if !state.store.disable_user(&login_id).await? {
        return Err(GatewayError::NoSuchUser);
    }
    record(&state, "disable", id, &master, &login_id);
    Ok(StatusCode::NO_CONTENT)
}
