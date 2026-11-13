use std::time::Duration;

use gateway::GatewayError;
use gateway::auth::password::HashCost;
use gateway::audit::{AuditStore, Event};
use gateway::store::login::Attempt;
use gateway::store::{AuthPolicy, AuthStore};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery";

fn cheap() -> HashCost {
    HashCost { memory_kib: 8 * 1024, iterations: 1, parallelism: 1 }
}

async fn fixture(policy: AuthPolicy) -> Option<(PgPool, String, AuthStore)> {
    let url = std::env::var("GATEWAY_TEST_DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    let schema = format!("auth_test_{}", Uuid::new_v4().simple());
    let store = AuthStore::open(pool.clone(), &schema, policy).await.ok()?;
    Some((pool, schema, store))
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE"))).execute(pool).await;
}

#[tokio::test]
async fn login_session_logout() -> Result<(), GatewayError> {
    let Some((pool, schema, store)) = fixture(AuthPolicy::default()).await else { return Ok(()) };
    let user = store.add_user("grader1", PASSWORD, cheap()).await?;
    let s = store.login("grader1", PASSWORD).await?;
    assert_eq!(s.user_id, user);
    let active = store.session(&s.token).await?;
    assert_eq!(active.user_id, user);
    let again = store.login("grader1", PASSWORD).await?;
    assert_ne!(again.token, s.token);
    store.logout(&s.token).await?;
    assert!(matches!(store.session(&s.token).await, Err(GatewayError::Unauthenticated)));
    assert!(store.session(&again.token).await.is_ok());
    drop_schema(&pool, &schema).await;
    Ok(())
}

#[tokio::test]
async fn five_failures_lock_even_correct_password() -> Result<(), GatewayError> {
    let Some((pool, schema, store)) = fixture(AuthPolicy::default()).await else { return Ok(()) };
    store.add_user("grader2", PASSWORD, cheap()).await?;
    for _ in 0..5 {
        assert!(matches!(store.login("grader2", "wrong password!!").await, Err(GatewayError::LoginFailed)));
    }
    assert!(matches!(store.login("grader2", PASSWORD).await, Err(GatewayError::Locked)));
    assert!(matches!(store.login("nobody", PASSWORD).await, Err(GatewayError::LoginFailed)));
    drop_schema(&pool, &schema).await;
    Ok(())
}

#[tokio::test]
async fn expired_session_is_not_found_and_purged() -> Result<(), GatewayError> {
    let policy = AuthPolicy { idle: Duration::from_secs(1), absolute: Duration::from_secs(60), ..AuthPolicy::default() };
    let Some((pool, schema, store)) = fixture(policy).await else { return Ok(()) };
    store.add_user("grader3", PASSWORD, cheap()).await?;
    let s = store.login("grader3", PASSWORD).await?;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(matches!(store.session(&s.token).await, Err(GatewayError::Unauthenticated)));
    assert_eq!(store.purge_expired().await?, 1);
    drop_schema(&pool, &schema).await;
    Ok(())
}

#[tokio::test]
async fn thread_ownership_is_per_user() -> Result<(), GatewayError> {
    let Some((pool, schema, store)) = fixture(AuthPolicy::default()).await else { return Ok(()) };
    let a = store.add_user("grader4", PASSWORD, cheap()).await?;
    let b = store.add_user("grader5", PASSWORD, cheap()).await?;
    let t = Uuid::new_v4();
    store.add_thread(a, t).await?;
    assert!(store.owns(a, t).await?);
    assert!(!store.owns(b, t).await?);
    assert_eq!(store.threads(a).await?, vec![(t, None)]);
    assert!(store.threads(b).await?.is_empty());
    assert!(!store.rename_thread(b, t, "남의 대화").await?);
    assert!(store.rename_thread(a, t, "국가배상 질문").await?);
    assert_eq!(store.threads(a).await?, vec![(t, Some("국가배상 질문".to_string()))]);
    store.remove_thread(b, t).await?;
    assert!(store.owns(a, t).await?);
    store.remove_thread(a, t).await?;
    assert!(!store.owns(a, t).await?);
    assert_eq!(store.login_id_of(a).await?, "grader4");
    assert!(store.disable_user("grader4").await?);
    assert!(matches!(store.login("grader4", PASSWORD).await, Err(GatewayError::LoginFailed)));
    drop_schema(&pool, &schema).await;
    Ok(())
}

#[tokio::test]
async fn fifth_failure_reports_lock_and_usage_adds_up() -> Result<(), GatewayError> {
    let Some((pool, schema, store)) = fixture(AuthPolicy::default()).await else { return Ok(()) };
    let user = store.add_user("grader6", PASSWORD, cheap()).await?;
    let mut locks = Vec::new();
    for _ in 0..5 {
        match store.attempt("grader6", "wrong password!!").await? {
            Attempt::Failed { user_id, locked_now } => {
                assert_eq!(user_id, Some(user));
                locks.push(locked_now);
            }
            other => return Err(GatewayError::Config(format!("{other:?}"))),
        }
    }
    assert_eq!(locks, vec![false, false, false, false, true]);
    assert!(matches!(store.attempt("grader6", PASSWORD).await?, Attempt::Locked { .. }));
    assert!(matches!(store.attempt("nobody", PASSWORD).await?, Attempt::Failed { user_id: None, locked_now: false }));
    assert_eq!(store.tokens_today(user).await?, 0);
    store.add_tokens(user, 1000).await?;
    store.add_tokens(user, 234).await?;
    store.add_tokens(user, -5).await?;
    assert_eq!(store.tokens_today(user).await?, 1234);
    drop_schema(&pool, &schema).await;
    Ok(())
}

#[tokio::test]
async fn audit_rows_keep_only_given_fields_and_old_rows_go() -> Result<(), GatewayError> {
    let Some((pool, schema, _store)) = fixture(AuthPolicy::default()).await else { return Ok(()) };
    let audit_schema = format!("{schema}_audit");
    let audit = AuditStore::open(pool.clone(), &audit_schema).await?;
    let user = Uuid::new_v4();
    audit.write(Event::new("ask").user(user).thread(Uuid::new_v4()).request("rid-a")).await?;
    audit.write(Event::new("guard_block").user(user).request("rid-a").detail(json!({ "guard": "repeat", "tool": "search_laws" }))).await?;
    audit.write(Event::new("verdict").user(user).request("rid-a").detail(json!({ "x": "a".repeat(5000) }))).await?;
    let rows = audit.kinds_for("rid-a").await?;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].1, json!({ "guard": "repeat", "tool": "search_laws" }));
    assert_eq!(rows[2].1, json!({ "oversize": true }));
    sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE {audit_schema}.event SET at = now() - interval '366 days' WHERE kind = 'ask'")))
        .execute(&pool)
        .await
        .map_err(|_| GatewayError::Internal)?;
    assert_eq!(audit.purge_older_than_year().await?, 1);
    assert_eq!(audit.kinds_of_user(user).await?, vec!["guard_block".to_string(), "verdict".to_string()]);
    drop_schema(&pool, &audit_schema).await;
    drop_schema(&pool, &schema).await;
    Ok(())
}
