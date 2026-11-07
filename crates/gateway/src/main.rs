use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use gateway::auth::password::HashCost;
use gateway::config::Config;
use gateway::http::router;
use gateway::state::{AppState, LoginLimiter};
use gateway::store::{AuthPolicy, AuthStore};
use gateway::GatewayError;
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

async fn open_store(cfg: &Config) -> Result<AuthStore, GatewayError> {
    let pool = PgPoolOptions::new().max_connections(4).connect(&cfg.app_db_url).await.map_err(|_| GatewayError::Upstream)?;
    AuthStore::open(pool, &cfg.auth_schema, AuthPolicy::default()).await
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
    let store = open_store(&cfg).await?;
    let purge = store.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = purge.purge_expired().await {
                tracing::warn!(error = %e, "auth.purge_failed");
            }
        }
    });
    let http = reqwest::Client::builder().build().map_err(|_| GatewayError::Internal)?;
    let state = AppState { limiter: Arc::new(LoginLimiter::new(cfg.login_per_minute)), cfg: Arc::new(cfg.clone()), store, http };
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
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    if let Err(e) = run().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
