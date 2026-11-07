use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use gateway::GatewayError;
use gateway::auth::password::HashCost;
use gateway::config::Config;
use gateway::http::router;
use gateway::state::{AppState, LoginLimiter};
use gateway::store::{AuthPolicy, AuthStore};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery";
const KEY: &str = "test-internal-key";

fn cheap() -> HashCost {
    HashCost { memory_kib: 8 * 1024, iterations: 1, parallelism: 1 }
}

fn keyed(headers: &HeaderMap) -> bool {
    headers.get("x-internal-key").and_then(|v| v.to_str().ok()) == Some(KEY)
        && headers.get("x-user-id").and_then(|v| v.to_str().ok()).and_then(|v| Uuid::parse_str(v).ok()).is_some()
}

async fn fake_agent() -> Option<SocketAddr> {
    let app = Router::new()
        .route(
            "/session",
            post(|headers: HeaderMap| async move {
                if !keyed(&headers) {
                    return (StatusCode::UNAUTHORIZED, axum::Json(json!({}))).into_response_parts();
                }
                (StatusCode::OK, axum::Json(json!({ "session_id": Uuid::new_v4().to_string() }))).into_response_parts()
            }),
        )
        .route(
            "/session/{id}/ask",
            post(|headers: HeaderMap| async move {
                if !keyed(&headers) {
                    return (StatusCode::UNAUTHORIZED, String::new());
                }
                let body = concat!(
                    "event: Status\ndata: {\"phase\": \"start\", \"tool\": \"search_laws\"}\n\n",
                    "event: Token\ndata: {\"text\": \"<think>internal plan</think>\"}\n\n",
                    "event: Token\ndata: {\"text\": \"visible answer\"}\n\n",
                    "event: Done\ndata: {\"request_id\": \"r\"}\n\n",
                );
                (StatusCode::OK, body.to_string())
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Some(addr)
}

trait Parts {
    fn into_response_parts(self) -> (StatusCode, axum::Json<Value>);
}

impl Parts for (StatusCode, axum::Json<Value>) {
    fn into_response_parts(self) -> (StatusCode, axum::Json<Value>) {
        self
    }
}

struct Fixture {
    pool: PgPool,
    schema: String,
    base: String,
    http: reqwest::Client,
}

async fn fixture() -> Option<Fixture> {
    let url = std::env::var("GATEWAY_TEST_DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    let schema = format!("auth_test_{}", Uuid::new_v4().simple());
    let store = AuthStore::open(pool.clone(), &schema, AuthPolicy::default()).await.ok()?;
    store.add_user("grader_a", PASSWORD, cheap()).await.ok()?;
    store.add_user("grader_b", PASSWORD, cheap()).await.ok()?;
    let agent = fake_agent().await?;
    let cfg = Config {
        bind: "127.0.0.1:0".parse().ok()?,
        app_db_url: String::new(),
        auth_schema: schema.clone(),
        agent_url: format!("http://{agent}"),
        internal_key: KEY.into(),
        web_dir: None,
        ask_deadline: Duration::from_secs(10),
        call_deadline: Duration::from_secs(10),
        upload_limit_bytes: 1024 * 1024,
        login_per_minute: 10,
        hidden_spans: vec![("<think>".into(), "</think>".into())],
    };
    let state = AppState {
        limiter: Arc::new(LoginLimiter::new(cfg.login_per_minute)),
        cfg: Arc::new(cfg),
        store,
        http: reqwest::Client::new(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(state).into_make_service_with_connect_info::<SocketAddr>()).await;
    });
    Some(Fixture { pool, schema, base: format!("http://{addr}"), http: reqwest::Client::new() })
}

async fn drop_schema(pool: &PgPool, schema: &str) {
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE"))).execute(pool).await;
}

struct Signed {
    cookie: String,
    csrf: String,
}

async fn login(f: &Fixture, id: &str, password: &str) -> Result<(StatusCode, Option<Signed>), reqwest::Error> {
    let res = f.http.post(format!("{}/api/auth/login", f.base)).json(&json!({ "login_id": id, "password": password })).send().await?;
    let status = StatusCode::from_u16(res.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let cookie = res
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::to_string);
    let body: Value = res.json().await.unwrap_or(Value::Null);
    let csrf = body.get("csrf_token").and_then(Value::as_str).map(str::to_string);
    Ok((status, cookie.zip(csrf).map(|(cookie, csrf)| Signed { cookie, csrf })))
}

#[tokio::test]
async fn login_owner_check_csrf_and_filtered_stream() -> Result<(), Box<dyn std::error::Error>> {
    let Some(f) = fixture().await else { return Ok(()) };
    let (bad, none) = login(&f, "grader_a", "wrong password!!").await?;
    assert_eq!(bad, StatusCode::UNAUTHORIZED);
    assert!(none.is_none());
    let (ok, a) = login(&f, "grader_a", PASSWORD).await?;
    assert_eq!(ok, StatusCode::OK);
    let a = a.ok_or("no session")?;
    assert!(a.cookie.starts_with("__Host-sid="));

    let anon = f.http.get(format!("{}/api/sessions", f.base)).send().await?;
    assert_eq!(anon.status().as_u16(), 401);

    let no_csrf = f.http.post(format!("{}/session", f.base)).header("cookie", &a.cookie).send().await?;
    assert_eq!(no_csrf.status().as_u16(), 403);

    let created: Value = f
        .http
        .post(format!("{}/session", f.base))
        .header("cookie", &a.cookie)
        .header("x-csrf-token", &a.csrf)
        .send()
        .await?
        .json()
        .await?;
    let sid = created.get("session_id").and_then(Value::as_str).ok_or("no session id")?.to_string();

    let listed: Value = f.http.get(format!("{}/api/sessions", f.base)).header("cookie", &a.cookie).send().await?.json().await?;
    assert_eq!(listed, json!([{ "session_id": sid }]));

    let res = f
        .http
        .post(format!("{}/session/{sid}/ask", f.base))
        .header("cookie", &a.cookie)
        .header("x-csrf-token", &a.csrf)
        .json(&json!({ "question": "q" }))
        .send()
        .await?;
    assert_eq!(res.status().as_u16(), 200);
    assert_eq!(res.headers().get("x-content-type-options").and_then(|v| v.to_str().ok()), Some("nosniff"));
    let stream = res.text().await?;
    assert!(stream.contains("visible answer"));
    assert!(!stream.contains("internal plan"));
    assert!(!stream.contains("search_laws"));

    let (_, b) = login(&f, "grader_b", PASSWORD).await?;
    let b = b.ok_or("no session")?;
    let foreign = f
        .http
        .post(format!("{}/session/{sid}/ask", f.base))
        .header("cookie", &b.cookie)
        .header("x-csrf-token", &b.csrf)
        .json(&json!({ "question": "q" }))
        .send()
        .await?;
    assert_eq!(foreign.status().as_u16(), 404);

    let me: Value = f.http.get(format!("{}/api/auth/me", f.base)).header("cookie", &a.cookie).send().await?.json().await?;
    assert_eq!(me, json!({ "login_id": "grader_a" }));
    let out = f
        .http
        .post(format!("{}/api/auth/logout", f.base))
        .header("cookie", &a.cookie)
        .header("x-csrf-token", &a.csrf)
        .send()
        .await?;
    assert_eq!(out.status().as_u16(), 204);
    let after = f.http.get(format!("{}/api/auth/me", f.base)).header("cookie", &a.cookie).send().await?;
    assert_eq!(after.status().as_u16(), 401);

    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}

#[tokio::test]
async fn eleventh_login_in_a_minute_is_refused() -> Result<(), GatewayError> {
    let Some(f) = fixture().await else { return Ok(()) };
    let mut statuses = Vec::new();
    for _ in 0..11 {
        let (status, _) = login(&f, "grader_a", "wrong password!!").await.map_err(|_| GatewayError::Internal)?;
        statuses.push(status);
    }
    assert!(statuses[..10].iter().all(|s| *s == StatusCode::UNAUTHORIZED));
    assert_eq!(statuses[10], StatusCode::TOO_MANY_REQUESTS);
    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}
