#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use gateway::audit::AuditStore;
use gateway::memory::MemoryGuard;
use gateway::quota::AskQuota;
use gateway::auth::password::HashCost;
use gateway::config::Config;
use gateway::http::router;
use gateway::state::{AppState, LoginLimiter};
use gateway::store::{AuthPolicy, AuthStore};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

pub const PASSWORD: &str = "correct horse battery";
pub const KEY: &str = "test-internal-key";

pub fn cheap() -> HashCost {
    HashCost { memory_kib: 8 * 1024, iterations: 1, parallelism: 1 }
}

pub fn keyed(headers: &HeaderMap) -> bool {
    headers.get("x-internal-key").and_then(|v| v.to_str().ok()) == Some(KEY)
        && headers.get("x-user-id").and_then(|v| v.to_str().ok()).and_then(|v| Uuid::parse_str(v).ok()).is_some()
}

pub async fn fake_agent() -> Option<SocketAddr> {
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
        .route("/session/{id}/ask", post(fake_ask))
        .route("/session/{id}", axum::routing::delete(|| async { StatusCode::NO_CONTENT }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Some(addr)
}

pub async fn fake_ask(headers: HeaderMap, axum::Json(body): axum::Json<Value>) -> axum::response::Response {
    use axum::response::IntoResponse;
    if !keyed(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let slow = body.get("question").and_then(Value::as_str) == Some("slow");
    let text = concat!(
        "event: Status\ndata: {\"phase\": \"start\", \"tool\": \"search_laws\"}\n\n",
        "event: Token\ndata: {\"text\": \"<think>internal plan</think>\"}\n\n",
        "event: Token\ndata: {\"text\": \"visible answer\"}\n\n",
        "event: Audit\ndata: {\"kind\": \"guard_block\", \"detail\": {\"guard\": \"repeat\", \"tool\": \"search_laws\"}}\n\n",
        "event: Usage\ndata: {\"tokens\": 1234, \"calls\": 3}\n\n",
        "event: Done\ndata: {\"request_id\": \"r\"}\n\n",
    );
    let stream = futures::stream::once(async move {
        if slow {
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
        Ok::<_, std::io::Error>(text.to_string())
    });
    axum::response::Response::new(axum::body::Body::from_stream(stream))
}

pub trait Parts {
    fn into_response_parts(self) -> (StatusCode, axum::Json<Value>);
}

impl Parts for (StatusCode, axum::Json<Value>) {
    fn into_response_parts(self) -> (StatusCode, axum::Json<Value>) {
        self
    }
}

pub struct Fixture {
    pub store: AuthStore,
    pub audit: AuditStore,
    pub a_id: Uuid,
    pub pool: PgPool,
    pub schema: String,
    pub base: String,
    pub http: reqwest::Client,
}

pub async fn fixture() -> Option<Fixture> {
    let url = std::env::var("GATEWAY_TEST_DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    let schema = format!("auth_test_{}", Uuid::new_v4().simple());
    let store = AuthStore::open(pool.clone(), &schema, AuthPolicy::default()).await.ok()?;
    let a_id = store.add_user("grader_a", PASSWORD, cheap()).await.ok()?;
    store.add_user("grader_b", PASSWORD, cheap()).await.ok()?;
    let agent = fake_agent().await?;
    let cfg = Config {
        bind: "127.0.0.1:0".parse().ok()?,
        app_db_url: String::new(),
        auth_schema: schema.clone(),
        audit_schema: format!("{schema}_audit"),
        agent_url: format!("http://{agent}"),
        internal_key: KEY.into(),
        web_dir: None,
        ask_deadline: Duration::from_secs(10),
        call_deadline: Duration::from_secs(10),
        upload_limit_bytes: 1024 * 1024,
        login_per_minute: 10,
        hidden_spans: vec![("<think>".into(), "</think>".into())],
        daily_token_limit: 3_000_000,
        ask_wait: Duration::from_millis(300),
        ask_global: 4,
        memory_floor_bytes: 0,
        memory_every: Duration::from_secs(30),
    };
    let audit = AuditStore::open(pool.clone(), &cfg.audit_schema).await.ok()?;
    let state = AppState {
        limiter: Arc::new(LoginLimiter::new(cfg.login_per_minute)),
        quota: Arc::new(AskQuota::new(cfg.ask_global, cfg.ask_wait)),
        memory: Arc::new(MemoryGuard::new(cfg.memory_floor_bytes)),
        cfg: Arc::new(cfg),
        store: store.clone(),
        audit: audit.clone(),
        http: reqwest::Client::new(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(state).into_make_service_with_connect_info::<SocketAddr>()).await;
    });
    Some(Fixture { store, audit, a_id, pool, schema, base: format!("http://{addr}"), http: reqwest::Client::new() })
}

pub async fn drop_schema(pool: &PgPool, schema: &str) {
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE"))).execute(pool).await;
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema}_audit CASCADE"))).execute(pool).await;
}

pub struct Signed {
    pub cookie: String,
    pub csrf: String,
}

pub async fn login(f: &Fixture, id: &str, password: &str) -> Result<(StatusCode, Option<Signed>), reqwest::Error> {
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

pub async fn new_session(f: &Fixture, s: &Signed) -> Result<String, Box<dyn std::error::Error>> {
    let created: Value = f.http.post(format!("{}/session", f.base)).header("cookie", &s.cookie).header("x-csrf-token", &s.csrf).send().await?.json().await?;
    Ok(created.get("session_id").and_then(Value::as_str).ok_or("no session id")?.to_string())
}

pub async fn ask(f: &Fixture, s: &Signed, sid: &str, question: &str) -> Result<reqwest::Response, reqwest::Error> {
    f.http
        .post(format!("{}/session/{sid}/ask", f.base))
        .header("cookie", &s.cookie)
        .header("x-csrf-token", &s.csrf)
        .json(&json!({ "question": question }))
        .send()
        .await
}
