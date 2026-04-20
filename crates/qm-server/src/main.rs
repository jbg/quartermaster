use std::{net::SocketAddr, str::FromStr, sync::Arc};

use anyhow::Context;
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use qm_api::{ApiConfig, AppState, RegistrationMode};
use qm_db::Database;
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawConfig {
    bind: String,
    database_url: String,
    registration_mode: String,
    access_token_ttl_seconds: i64,
    refresh_token_ttl_seconds: i64,
}

impl Default for RawConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".into(),
            database_url: "sqlite://data.db?mode=rwc".into(),
            registration_mode: "first_run_only".into(),
            access_token_ttl_seconds: 30 * 60,
            refresh_token_ttl_seconds: 60 * 24 * 60 * 60,
        }
    }
}

fn load_config() -> anyhow::Result<RawConfig> {
    Figment::from(Serialized::defaults(RawConfig::default()))
        .merge(Env::prefixed("QM_"))
        .extract()
        .context("loading config")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")))
        .with(fmt::layer())
        .init();

    let raw = load_config()?;
    tracing::info!(bind = %raw.bind, database_url = %raw.database_url, "starting qm-server");

    let db = Database::connect(&raw.database_url)
        .await
        .context("connecting to database")?;
    db.migrate().await.context("running migrations")?;

    let api_config = ApiConfig {
        registration_mode: RegistrationMode::from_str(&raw.registration_mode)
            .map_err(anyhow::Error::msg)?,
        access_token_ttl_seconds: raw.access_token_ttl_seconds,
        refresh_token_ttl_seconds: raw.refresh_token_ttl_seconds,
    };

    let state = AppState {
        db,
        config: Arc::new(api_config),
    };

    let app = qm_api::router(state).layer(TraceLayer::new_for_http());

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
