pub mod login;
pub mod schema;
pub mod usage;
pub mod users;

use std::time::Duration;

use sqlx::{AssertSqlSafe, PgPool};
use sqlx::Row;
use uuid::Uuid;

use crate::auth::password::{HashCost, hash_password};
use crate::auth::token::{new_token, token_hash};
use crate::error::GatewayError;

#[derive(Debug, Clone, Copy)]
pub struct AuthPolicy {
    pub max_failures: i32,
    pub lock_for: Duration,
    pub idle: Duration,
    pub absolute: Duration,
}

impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            max_failures: 5,
            lock_for: Duration::from_secs(15 * 60),
            idle: Duration::from_secs(30 * 60),
            absolute: Duration::from_secs(12 * 60 * 60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub token: String,
    pub csrf_token: String,
    pub user_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub user_id: Uuid,
    pub csrf_token_hash: String,
}

#[derive(Clone)]
pub struct AuthStore {
    pool: PgPool,
    schema: String,
    policy: AuthPolicy,
}

pub(crate) fn db(_: sqlx::Error) -> GatewayError {
    GatewayError::Internal
}

pub(crate) fn insert_error(e: sqlx::Error) -> GatewayError {
    match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => GatewayError::Duplicate,
        _ => GatewayError::Internal,
    }
}

pub(crate) fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

impl AuthStore {
    pub async fn open(pool: PgPool, schema: &str, policy: AuthPolicy) -> Result<Self, GatewayError> {
        if !schema::valid_schema_name(schema) {
            return Err(GatewayError::Internal);
        }
        for stmt in schema::ddl(schema) {
            sqlx::query(AssertSqlSafe(stmt)).execute(&pool).await.map_err(db)?;
        }
        Ok(Self { pool, schema: schema.to_string(), policy })
    }

    pub async fn add_user(&self, login_id: &str, password: &str, cost: HashCost) -> Result<Uuid, GatewayError> {
        let hash = hash_password(password, cost)?;
        self.insert_user(login_id, &hash, "", users::UserStatus::Active).await
    }

    pub(crate) async fn insert_user(&self, login_id: &str, hash: &str, display_name: &str, status: users::UserStatus) -> Result<Uuid, GatewayError> {
        let id = Uuid::new_v4();
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO {}.app_user (user_id, login_id, password_hash, display_name, status) VALUES ($1, $2, $3, $4, $5)",
            self.schema
        )))
        .bind(id)
        .bind(login_id)
        .bind(hash)
        .bind(display_name)
        .bind(status.as_str())
        .execute(&self.pool)
        .await
        .map_err(insert_error)?;
        Ok(id)
    }

    pub(crate) async fn new_session(&self, user_id: Uuid) -> Result<NewSession, GatewayError> {
        let token = new_token();
        let csrf_token = new_token();
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO {}.login_session (token_hash, user_id, csrf_token_hash, expires_at)
             VALUES ($1, $2, $3, now() + make_interval(secs => $4))",
            self.schema
        )))
        .bind(token_hash(&token))
        .bind(user_id)
        .bind(token_hash(&csrf_token))
        .bind(secs(self.policy.absolute))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(NewSession { token, csrf_token, user_id })
    }

    pub async fn session(&self, token: &str) -> Result<ActiveSession, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "UPDATE {}.login_session SET last_seen_at = now()
             WHERE token_hash = $1 AND expires_at > now() AND last_seen_at > now() - make_interval(secs => $2)
             RETURNING user_id, csrf_token_hash",
            self.schema
        )))
        .bind(token_hash(token))
        .bind(secs(self.policy.idle))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let row = row.ok_or(GatewayError::Unauthenticated)?;
        Ok(ActiveSession {
            user_id: row.try_get("user_id").map_err(db)?,
            csrf_token_hash: row.try_get::<String, _>("csrf_token_hash").map_err(db)?.trim().to_string(),
        })
    }

    pub async fn logout(&self, token: &str) -> Result<(), GatewayError> {
        sqlx::query(AssertSqlSafe(format!("DELETE FROM {}.login_session WHERE token_hash = $1", self.schema)))
            .bind(token_hash(token))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    pub async fn purge_expired(&self) -> Result<u64, GatewayError> {
        let done = sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM {}.login_session WHERE expires_at <= now() OR last_seen_at <= now() - make_interval(secs => $1)",
            self.schema
        )))
        .bind(secs(self.policy.idle))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(done.rows_affected())
    }

    pub async fn add_thread(&self, user_id: Uuid, thread_id: Uuid) -> Result<(), GatewayError> {
        sqlx::query(AssertSqlSafe(format!("INSERT INTO {}.thread_owner (thread_id, user_id) VALUES ($1, $2)", self.schema)))
            .bind(thread_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    pub async fn remove_thread(&self, user_id: Uuid, thread_id: Uuid) -> Result<(), GatewayError> {
        sqlx::query(AssertSqlSafe(format!("DELETE FROM {}.thread_owner WHERE thread_id = $1 AND user_id = $2", self.schema)))
            .bind(thread_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    pub async fn touch_thread(&self, thread_id: Uuid) -> Result<(), GatewayError> {
        sqlx::query(AssertSqlSafe(format!("UPDATE {}.thread_owner SET updated_at = now() WHERE thread_id = $1", self.schema)))
            .bind(thread_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    pub async fn login_id_of(&self, user_id: Uuid) -> Result<String, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!("SELECT login_id FROM {}.app_user WHERE user_id = $1", self.schema)))
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or(GatewayError::Unauthenticated)?;
        row.try_get("login_id").map_err(db)
    }

    pub async fn disable_user(&self, login_id: &str) -> Result<bool, GatewayError> {
        let done = sqlx::query(AssertSqlSafe(format!("UPDATE {}.app_user SET disabled = true, status = 'disabled' WHERE login_id = $1", self.schema)))
            .bind(login_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM {}.login_session WHERE user_id IN (SELECT user_id FROM {}.app_user WHERE login_id = $1)",
            self.schema, self.schema
        )))
        .bind(login_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn owns(&self, user_id: Uuid, thread_id: Uuid) -> Result<bool, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!("SELECT 1 FROM {}.thread_owner WHERE thread_id = $1 AND user_id = $2", self.schema)))
            .bind(thread_id)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        Ok(row.is_some())
    }

    pub async fn threads(&self, user_id: Uuid) -> Result<Vec<(Uuid, Option<String>)>, GatewayError> {
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT thread_id, title FROM {}.thread_owner WHERE user_id = $1 ORDER BY updated_at DESC",
            self.schema
        )))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(|r| Ok((r.try_get("thread_id").map_err(db)?, r.try_get("title").map_err(db)?))).collect()
    }

    pub async fn rename_thread(&self, user_id: Uuid, thread_id: Uuid, title: &str) -> Result<bool, GatewayError> {
        let done = sqlx::query(AssertSqlSafe(format!("UPDATE {}.thread_owner SET title = $3 WHERE thread_id = $1 AND user_id = $2", self.schema)))
            .bind(thread_id)
            .bind(user_id)
            .bind(title)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn name_if_untitled(&self, thread_id: Uuid, title: &str) -> Result<(), GatewayError> {
        sqlx::query(AssertSqlSafe(format!("UPDATE {}.thread_owner SET title = $2 WHERE thread_id = $1 AND title IS NULL", self.schema)))
            .bind(thread_id)
            .bind(title)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}
