use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, PgPool, Row};
use uuid::Uuid;

use crate::error::GatewayError;
use crate::store::schema::valid_schema_name;

pub const DETAIL_MAX_BYTES: usize = 2048;
pub const AGENT_KINDS: [&str; 2] = ["guard_block", "verdict"];

#[derive(Debug, Clone, Default)]
pub struct Event {
    pub user_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub request_id: Option<String>,
    pub kind: String,
    pub detail: Value,
}

impl Event {
    pub fn new(kind: &str) -> Self {
        Self { kind: kind.to_string(), detail: json!({}), ..Self::default() }
    }

    pub fn user(mut self, user: Uuid) -> Self {
        self.user_id = Some(user);
        self
    }

    pub fn thread(mut self, thread: Uuid) -> Self {
        self.thread_id = Some(thread);
        self
    }

    pub fn request(mut self, request_id: &str) -> Self {
        self.request_id = Some(request_id.to_string());
        self
    }

    pub fn detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }
}

pub fn bounded(detail: Value) -> Value {
    let detail = if detail.is_object() { detail } else { json!({}) };
    if detail.to_string().len() > DETAIL_MAX_BYTES { json!({ "oversize": true }) } else { detail }
}

fn ddl(schema: &str) -> Vec<String> {
    vec![
        format!("CREATE SCHEMA IF NOT EXISTS {schema}"),
        format!(
            "CREATE TABLE IF NOT EXISTS {schema}.event (
                at timestamptz NOT NULL DEFAULT now(),
                user_id uuid,
                thread_id uuid,
                request_id text,
                kind text NOT NULL,
                detail jsonb NOT NULL DEFAULT '{{}}'::jsonb
            )"
        ),
        format!("CREATE INDEX IF NOT EXISTS event_at ON {schema}.event (at)"),
    ]
}

#[derive(Clone)]
pub struct AuditStore {
    pool: PgPool,
    schema: String,
}

fn db(_: sqlx::Error) -> GatewayError {
    GatewayError::Internal
}

impl AuditStore {
    pub async fn open(pool: PgPool, schema: &str) -> Result<Self, GatewayError> {
        if !valid_schema_name(schema) {
            return Err(GatewayError::Config("AUDIT_SCHEMA".into()));
        }
        for stmt in ddl(schema) {
            sqlx::query(AssertSqlSafe(stmt)).execute(&pool).await.map_err(db)?;
        }
        Ok(Self { pool, schema: schema.to_string() })
    }

    pub async fn write(&self, event: Event) -> Result<(), GatewayError> {
        sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO {}.event (user_id, thread_id, request_id, kind, detail) VALUES ($1, $2, $3, $4, $5)",
            self.schema
        )))
        .bind(event.user_id)
        .bind(event.thread_id)
        .bind(event.request_id)
        .bind(event.kind)
        .bind(bounded(event.detail))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    pub fn spawn(&self, event: Event) {
        let store = self.clone();
        tokio::spawn(async move {
            let kind = event.kind.clone();
            if let Err(e) = store.write(event).await {
                tracing::warn!(error = %e, kind = %kind, "audit.write_failed");
            }
        });
    }

    pub async fn purge_older_than_year(&self) -> Result<u64, GatewayError> {
        let done = sqlx::query(AssertSqlSafe(format!("DELETE FROM {}.event WHERE at < now() - interval '1 year'", self.schema)))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected())
    }

    pub async fn kinds_for(&self, request_id: &str) -> Result<Vec<(String, Value)>, GatewayError> {
        let rows = sqlx::query(AssertSqlSafe(format!("SELECT kind, detail FROM {}.event WHERE request_id = $1 ORDER BY at", self.schema)))
            .bind(request_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(|r| Ok((r.try_get("kind").map_err(db)?, r.try_get("detail").map_err(db)?))).collect()
    }

    pub async fn kinds_of_user(&self, user: Uuid) -> Result<Vec<String>, GatewayError> {
        let rows = sqlx::query(AssertSqlSafe(format!("SELECT kind FROM {}.event WHERE user_id = $1 ORDER BY at", self.schema)))
            .bind(user)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(|r| r.try_get("kind").map_err(db)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversize_or_non_object_detail_is_replaced() {
        assert_eq!(bounded(json!({ "guard": "repeat" })), json!({ "guard": "repeat" }));
        assert_eq!(bounded(json!("text")), json!({}));
        let big = json!({ "x": "a".repeat(DETAIL_MAX_BYTES) });
        assert_eq!(bounded(big), json!({ "oversize": true }));
    }
}
