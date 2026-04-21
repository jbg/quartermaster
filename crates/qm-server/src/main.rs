use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use qm_api::{ApiConfig, AppState, RegistrationMode};
use qm_db::Database;
use serde::{Deserialize, Serialize};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const USER_AGENT: &str = concat!(
    "Quartermaster/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/jbg/quartermaster)",
);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawConfig {
    bind: String,
    database_url: String,
    log_format: String,
    registration_mode: String,
    access_token_ttl_seconds: i64,
    refresh_token_ttl_seconds: i64,
    off_positive_ttl_days: i64,
    off_negative_ttl_days: i64,
}

impl Default for RawConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".into(),
            database_url: "sqlite://data.db?mode=rwc".into(),
            log_format: "text".into(),
            registration_mode: "first_run_only".into(),
            access_token_ttl_seconds: 30 * 60,
            refresh_token_ttl_seconds: 60 * 24 * 60 * 60,
            off_positive_ttl_days: 30,
            off_negative_ttl_days: 7,
        }
    }
}

fn load_config() -> anyhow::Result<RawConfig> {
    Figment::from(Serialized::defaults(RawConfig::default()))
        .merge(Env::prefixed("QM_"))
        .extract()
        .context("loading config")
}

#[derive(Clone, Copy, Debug)]
enum LogFormat {
    Text,
    Json,
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(format!("unknown log_format: {other}")),
        }
    }
}

fn init_tracing(log_format: LogFormat) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));

    match log_format {
        LogFormat::Text => tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .init(),
        LogFormat::Json => tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().json())
            .init(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let raw = load_config()?;
    init_tracing(LogFormat::from_str(&raw.log_format).map_err(anyhow::Error::msg)?);
    tracing::info!(bind = %raw.bind, database_url = %raw.database_url, "starting qm-server");

    let db = Database::connect(&raw.database_url)
        .await
        .context("connecting to database")?;
    db.migrate().await.context("running migrations")?;

    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(5))
        .build()
        .context("building HTTP client")?;

    let api_config = ApiConfig {
        registration_mode: RegistrationMode::from_str(&raw.registration_mode)
            .map_err(anyhow::Error::msg)?,
        access_token_ttl_seconds: raw.access_token_ttl_seconds,
        refresh_token_ttl_seconds: raw.refresh_token_ttl_seconds,
        off_positive_ttl_days: raw.off_positive_ttl_days,
        off_negative_ttl_days: raw.off_negative_ttl_days,
    };

    let state = AppState {
        db,
        config: Arc::new(api_config),
        http,
    };

    let app = qm_api::router(state);

    let addr: SocketAddr = raw.bind.parse().context("parsing bind address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serving HTTP")?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut sig) = signal(SignalKind::terminate()) {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
