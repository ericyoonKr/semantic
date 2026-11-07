use std::time::Duration;

use gateway::GatewayError;
use gateway::auth::password::HashCost;
use gateway::store::{AuthPolicy, AuthStore};
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
    let t = store.add_thread(a).await?;
    assert!(store.owns(a, t).await?);
    assert!(!store.owns(b, t).await?);
    assert_eq!(store.threads(a).await?, vec![t]);
    assert!(store.threads(b).await?.is_empty());
    drop_schema(&pool, &schema).await;
    Ok(())
}
