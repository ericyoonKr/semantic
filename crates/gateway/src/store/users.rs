use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, Row};
use uuid::Uuid;

use crate::auth::password::{HashCost, hash_signup_password};
use crate::error::GatewayError;
use crate::store::{AuthStore, db};

const REQUESTED_AT: &str = "to_char(requested_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserStatus {
    Pending,
    Active,
    Disabled,
}

impl UserStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            UserStatus::Pending => "pending",
            UserStatus::Active => "active",
            UserStatus::Disabled => "disabled",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            UserStatus::Pending => "대기",
            UserStatus::Active => "활성",
            UserStatus::Disabled => "꺼짐",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(UserStatus::Pending),
            "active" => Some(UserStatus::Active),
            "disabled" => Some(UserStatus::Disabled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRow {
    pub user_id: Uuid,
    pub login_id: String,
    pub display_name: String,
    pub status: UserStatus,
    pub is_master: bool,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub login_id: String,
    pub display_name: String,
    pub is_master: bool,
}

fn user_row(r: &PgRow) -> Result<UserRow, GatewayError> {
    let status: String = r.try_get("status").map_err(db)?;
    Ok(UserRow {
        user_id: r.try_get("user_id").map_err(db)?,
        login_id: r.try_get("login_id").map_err(db)?,
        display_name: r.try_get("display_name").map_err(db)?,
        status: UserStatus::parse(&status).ok_or(GatewayError::Internal)?,
        is_master: r.try_get("is_master").map_err(db)?,
        requested_at: r.try_get("requested_at").map_err(db)?,
    })
}

impl AuthStore {
    pub async fn signup_user(&self, login_id: &str, display_name: &str, password: &str, cost: HashCost) -> Result<Uuid, GatewayError> {
        let hash = hash_signup_password(password, cost)?;
        self.insert_user(login_id, &hash, display_name, UserStatus::Pending).await
    }

    pub async fn user_id(&self, login_id: &str) -> Result<Option<Uuid>, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!("SELECT user_id FROM {}.app_user WHERE login_id = $1", self.schema)))
            .bind(login_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.map(|r| r.try_get("user_id").map_err(db)).transpose()
    }

    pub async fn find_user(&self, login_id: &str) -> Result<Option<UserRow>, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT user_id, login_id, display_name, status, is_master, {REQUESTED_AT} AS requested_at FROM {}.app_user WHERE login_id = $1",
            self.schema
        )))
        .bind(login_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(user_row).transpose()
    }

    pub async fn list_users(&self) -> Result<Vec<UserRow>, GatewayError> {
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT user_id, login_id, display_name, status, is_master, {REQUESTED_AT} AS requested_at
             FROM {}.app_user ORDER BY (status <> 'pending'), requested_at, login_id",
            self.schema
        )))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(user_row).collect()
    }

    pub async fn profile(&self, user_id: Uuid) -> Result<Profile, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!("SELECT login_id, display_name, is_master FROM {}.app_user WHERE user_id = $1", self.schema)))
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(GatewayError::Unauthenticated)?;
        Ok(Profile {
            login_id: row.try_get("login_id").map_err(db)?,
            display_name: row.try_get("display_name").map_err(db)?,
            is_master: row.try_get("is_master").map_err(db)?,
        })
    }

    pub async fn is_master(&self, user_id: Uuid) -> Result<bool, GatewayError> {
        Ok(self.profile(user_id).await?.is_master)
    }

    pub async fn approve_user(&self, login_id: &str) -> Result<bool, GatewayError> {
        let done = sqlx::query(AssertSqlSafe(format!("UPDATE {}.app_user SET status = 'active', disabled = false WHERE login_id = $1", self.schema)))
            .bind(login_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn reject_user(&self, login_id: &str) -> Result<bool, GatewayError> {
        let done = sqlx::query(AssertSqlSafe(format!("DELETE FROM {}.app_user WHERE login_id = $1", self.schema)))
            .bind(login_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn set_master(&self, login_id: &str) -> Result<bool, GatewayError> {
        let done = sqlx::query(AssertSqlSafe(format!("UPDATE {}.app_user SET is_master = true WHERE login_id = $1", self.schema)))
            .bind(login_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected() == 1)
    }
}
