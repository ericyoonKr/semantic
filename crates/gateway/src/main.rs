use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use gateway::audit::AuditStore;
use gateway::auth::password::HashCost;
use gateway::config::Config;
use gateway::http::router;
use gateway::memory::MemoryGuard;
use gateway::quota::AskQuota;
use gateway::state::{AppState, LoginLimiter};
use gateway::store::{AuthPolicy, AuthStore};
use gateway::GatewayError;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Serve,
    User {
        #[command(subcommand)]
        action: UserAction,
    },
}

#[derive(Subcommand)]
enum UserAction {
    Add { login_id: String },
    Disable { login_id: String },
}

async fn open_pool(cfg: &Config) -> Result<PgPool, GatewayError> {
    PgPoolOptions::new().max_connections(4).connect(&cfg.app_db_url).await.map_err(|_| GatewayError::Upstream)
}

async fn open_store(cfg: &Config) -> Result<AuthStore, GatewayError> {
    AuthStore::open(open_pool(cfg).await?, &cfg.auth_schema, AuthPolicy::default()).await
}

fn every<F, Fut>(period: Duration, job: F)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(period);
        loop {
            tick.tick().await;
            job().await;
        }
    });
}

fn read_new_password() -> Result<String, GatewayError> {
    let first = rpassword::prompt_password("비밀번호: ").map_err(|_| GatewayError::Internal)?;
    let second = rpassword::prompt_password("다시 입력: ").map_err(|_| GatewayError::Internal)?;
    if first != second {
        return Err(GatewayError::BadRequest);
    }
    Ok(first)
}

async fn serve(cfg: Config) -> Result<(), GatewayError> {
    if !cfg.bind.ip().is_loopback() {
        return Err(GatewayError::Config("GATEWAY_BIND".into()));
    }
    let pool = open_pool(&cfg).await?;
    let store = AuthStore::open(pool.clone(), &cfg.auth_schema, AuthPolicy::default()).await?;
    let audit = AuditStore::open(pool, &cfg.audit_schema).await?;
    let purge = store.clone();
    every(Duration::from_secs(3600), move || {
        let purge = purge.clone();
        async move {
            if let Err(e) = purge.purge_expired().await {
                tracing::warn!(error = %e, "auth.purge_failed");
            }
        }
    });
    let old_audit = audit.clone();
    every(Duration::from_secs(24 * 3600), move || {
        let old_audit = old_audit.clone();
        async move {
            match old_audit.purge_older_than_year().await {
                Ok(n) => tracing::info!(rows = n, "audit.purged"),
                Err(e) => tracing::warn!(error = %e, "audit.purge_failed"),
            }
        }
    });
    let memory = Arc::new(MemoryGuard::new(cfg.memory_floor_bytes));
    memory.spawn_watch(cfg.memory_every);
    let http = reqwest::Client::builder().build().map_err(|_| GatewayError::Internal)?;
    let state = AppState {
        limiter: Arc::new(LoginLimiter::new(cfg.login_per_minute)),
        quota: Arc::new(AskQuota::new(cfg.ask_global, cfg.ask_wait, cfg.ask_queue_wait)),
        cfg: Arc::new(cfg.clone()),
        store,
        audit,
        memory,
        http,
    };
    let listener = tokio::net::TcpListener::bind(cfg.bind).await.map_err(|_| GatewayError::Internal)?;
    tracing::info!(addr = %cfg.bind, "gateway.listen");
    axum::serve(listener, router(state).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|_| GatewayError::Internal)
}

async fn run() -> Result<(), GatewayError> {
    let cfg = Config::from_env()?;
    match Cli::parse().command.unwrap_or(Command::Serve) {
        Command::Serve => serve(cfg).await,
        Command::User { action: UserAction::Add { login_id } } => {
            let password = read_new_password()?;
            open_store(&cfg).await?.add_user(&login_id, &password, HashCost::default()).await?;
            println!("사용자를 만들었습니다: {login_id}");
            Ok(())
        }
        Command::User { action: UserAction::Disable { login_id } } => {
            let done = open_store(&cfg).await?.disable_user(&login_id).await?;
            println!("{}", if done { "사용자를 껐습니다" } else { "그런 사용자가 없습니다" });
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() {
    let data_dir = std::env::var_os("DATA_DIR").filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    gateway::logs::init(data_dir.as_deref());
    if let Err(e) = run().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
