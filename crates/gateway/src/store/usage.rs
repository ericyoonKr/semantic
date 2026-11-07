use sqlx::{AssertSqlSafe, Row};
use uuid::Uuid;

use crate::error::GatewayError;
use crate::store::{AuthStore, db};

const TODAY: &str = "(now() AT TIME ZONE 'Asia/Seoul')::date";

impl AuthStore {
    pub async fn tokens_today(&self, user_id: Uuid) -> Result<i64, GatewayError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT COALESCE((SELECT tokens FROM {}.usage WHERE user_id = $1 AND day = {TODAY}), 0)::bigint AS tokens",
            self.schema
        )))
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        row.try_get("tokens").map_err(db)
    }

    pub async fn add_tokens(&self, user_id: Uuid, tokens: i64) -> Result<(), GatewayError> {
        if tokens <= 0 {
            return Ok(());
        }
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO {s}.usage (user_id, day, tokens) VALUES ($1, {TODAY}, $2)
             ON CONFLICT (user_id, day) DO UPDATE SET tokens = {s}.usage.tokens + EXCLUDED.tokens",
            s = self.schema
        )))
        .bind(user_id)
        .bind(tokens)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}
