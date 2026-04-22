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
    off_api_base_url: String,
    public_base_url: Option<String>,
    trust_proxy_headers: bool,
    rate_limit_auth_per_minute: u32,
    rate_limit_auth_burst: u32,
    rate_limit_barcode_per_minute: u32,
    rate_limit_barcode_burst: u32,
    rate_limit_history_per_minute: u32,
    rate_limit_history_burst: u32,
    off_timeout_seconds: u64,
    off_max_retries: u32,
    off_retry_base_delay_ms: u64,
    off_circuit_breaker_failure_threshold: u32,
    off_circuit_breaker_open_seconds: u64,
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
            off_api_base_url: "https://world.openfoodfacts.org/api/v2/product".into(),
            public_base_url: None,
            trust_proxy_headers: false,
            rate_limit_auth_per_minute: 10,
            rate_limit_auth_burst: 5,
            rate_limit_barcode_per_minute: 60,
            rate_limit_barcode_burst: 20,
            rate_limit_history_per_minute: 120,
            rate_limit_history_burst: 40,
            off_timeout_seconds: 5,
            off_max_retries: 2,
            off_retry_base_delay_ms: 200,
            off_circuit_breaker_failure_threshold: 5,
            off_circuit_breaker_open_seconds: 60,
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
        .timeout(Duration::from_secs(raw.off_timeout_seconds))
        .build()
        .context("building HTTP client")?;

    let api_config = ApiConfig {
        registration_mode: RegistrationMode::from_str(&raw.registration_mode)
            .map_err(anyhow::Error::msg)?,
        access_token_ttl_seconds: raw.access_token_ttl_seconds,
        refresh_token_ttl_seconds: raw.refresh_token_ttl_seconds,
        off_positive_ttl_days: raw.off_positive_ttl_days,
        off_negative_ttl_days: raw.off_negative_ttl_days,
        off_api_base_url: raw.off_api_base_url,
        public_base_url: raw.public_base_url,
        trust_proxy_headers: raw.trust_proxy_headers,
        rate_limit_auth: qm_api::RateLimitConfig {
            requests_per_minute: raw.rate_limit_auth_per_minute,
            burst: raw.rate_limit_auth_burst,
        },
        rate_limit_barcode: qm_api::RateLimitConfig {
            requests_per_minute: raw.rate_limit_barcode_per_minute,
            burst: raw.rate_limit_barcode_burst,
        },
        rate_limit_history: qm_api::RateLimitConfig {
            requests_per_minute: raw.rate_limit_history_per_minute,
            burst: raw.rate_limit_history_burst,
        },
        off_timeout: Duration::from_secs(raw.off_timeout_seconds),
        off_max_retries: raw.off_max_retries,
        off_retry_base_delay: Duration::from_millis(raw.off_retry_base_delay_ms),
        off_circuit_breaker_failure_threshold: raw.off_circuit_breaker_failure_threshold,
        off_circuit_breaker_open_for: Duration::from_secs(raw.off_circuit_breaker_open_seconds),
    };
    let api_config = Arc::new(api_config);

    let state = AppState {
        db,
        config: api_config.clone(),
        http,
        off_breaker: Arc::new(qm_api::openfoodfacts::OffCircuitBreaker::default()),
        rate_limiters: Arc::new(qm_api::rate_limit::RateLimiters::new(&api_config)),
    };

    let app = qm_api::router(state);

    let addr: SocketAddr = raw.bind.parse().context("parsing bind address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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
