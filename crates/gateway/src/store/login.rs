use sqlx::{AssertSqlSafe, Row};
use uuid::Uuid;

use crate::auth::password::verify_password;
use crate::error::GatewayError;
use crate::store::{AuthStore, NewSession, db, secs};

#[derive(Debug)]
pub enum Attempt {
    Signed(NewSession),
    Failed { user_id: Option<Uuid>, locked_now: bool },
    Locked { user_id: Uuid },
    Pending { user_id: Uuid },
}

impl AuthStore {
    pub async fn login(&self, login_id: &str, password: &str) -> Result<NewSession, GatewayError> {
        match self.attempt(login_id, password).await? {
            Attempt::Signed(s) => Ok(s),
            Attempt::Failed { .. } => Err(GatewayError::LoginFailed),
            Attempt::Locked { .. } => Err(GatewayError::Locked),
            Attempt::Pending { .. } => Err(GatewayError::Pending),
        }
    }

    pub async fn attempt(&self, login_id: &str, password: &str) -> Result<Attempt, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT user_id, password_hash, disabled, status, (locked_until IS NOT NULL AND locked_until > now()) AS locked
             FROM {}.app_user WHERE login_id = $1",
            self.schema
        )))
        .bind(login_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(row) = row else {
            return Ok(Attempt::Failed { user_id: None, locked_now: false });
        };
        let user_id: Uuid = row.try_get("user_id").map_err(db)?;
        let stored: String = row.try_get("password_hash").map_err(db)?;
        let status: String = row.try_get("status").map_err(db)?;
        let disabled: bool = row.try_get::<bool, _>("disabled").map_err(db)? || status == "disabled";
        let locked: bool = row.try_get("locked").map_err(db)?;
        if locked {
            return Ok(Attempt::Locked { user_id });
        }
        if disabled || !verify_password(password, &stored) {
            let locked_now = self.record_failure(user_id).await?;
            return Ok(Attempt::Failed { user_id: Some(user_id), locked_now });
        }
        if status == "pending" {
            return Ok(Attempt::Pending { user_id });
        }
        sqlx::query(AssertSqlSafe(format!("UPDATE {}.app_user SET failed_count = 0, locked_until = NULL WHERE user_id = $1", self.schema)))
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(Attempt::Signed(self.new_session(user_id).await?))
    }

    async fn record_failure(&self, user_id: Uuid) -> Result<bool, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "UPDATE {}.app_user SET failed_count = failed_count + 1,
               locked_until = CASE WHEN failed_count + 1 >= $2 THEN now() + make_interval(secs => $3) ELSE locked_until END
             WHERE user_id = $1
             RETURNING failed_count >= $2 AS locked_now",
            self.schema
        )))
        .bind(user_id)
        .bind(self.policy.max_failures)
        .bind(secs(self.policy.lock_for))
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        row.try_get("locked_now").map_err(db)
    }
}
