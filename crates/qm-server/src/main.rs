use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use figment::{
    providers::{Env, Serialized},
    Figment,
};
use qm_api::{
    rate_limit::{parse_trusted_proxy_cidrs, ClientIpMode},
    ApiConfig, AppState, RegistrationMode,
};
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
    rate_limit_client_ip_mode: String,
    rate_limit_trusted_proxy_cidrs: Option<String>,
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
    auth_session_sweep_interval_seconds: u64,
    auth_session_sweep_trigger_secret: Option<String>,
    expiry_reminders_enabled: bool,
    expiry_reminder_lead_days: i64,
    expiry_reminder_fire_hour: u32,
    expiry_reminder_fire_minute: u32,
    expiry_reminder_sweep_interval_seconds: u64,
    expiry_reminder_trigger_secret: Option<String>,
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
            rate_limit_client_ip_mode: "socket".into(),
            rate_limit_trusted_proxy_cidrs: None,
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
            auth_session_sweep_interval_seconds: 0,
            auth_session_sweep_trigger_secret: None,
            expiry_reminders_enabled: false,
            expiry_reminder_lead_days: 1,
            expiry_reminder_fire_hour: 9,
            expiry_reminder_fire_minute: 0,
            expiry_reminder_sweep_interval_seconds: 0,
            expiry_reminder_trigger_secret: None,
        }
    }
}

#[derive(Debug)]
struct LoadedConfig {
    bind: SocketAddr,
    database_url: String,
    log_format: LogFormat,
    api_config: Arc<ApiConfig>,
    auth_session_sweep_interval: Option<Duration>,
    expiry_reminder_sweep_interval: Option<Duration>,
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

fn build_config(raw: RawConfig) -> anyhow::Result<LoadedConfig> {
    let bind = raw.bind.parse().context("parsing bind address")?;
    let log_format = LogFormat::from_str(&raw.log_format).map_err(anyhow::Error::msg)?;
    let public_base_url = normalize_public_base_url(raw.public_base_url)?;
    let rate_limit_client_ip_mode =
        ClientIpMode::from_str(&raw.rate_limit_client_ip_mode).map_err(anyhow::Error::msg)?;
    let trusted_proxy_cidrs = match raw.rate_limit_trusted_proxy_cidrs.as_deref() {
        Some(value) => parse_trusted_proxy_cidrs(value).map_err(anyhow::Error::msg)?,
        None => Vec::new(),
    };
    if rate_limit_client_ip_mode == ClientIpMode::XForwardedFor && trusted_proxy_cidrs.is_empty() {
        anyhow::bail!(
            "QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS is required when QM_RATE_LIMIT_CLIENT_IP_MODE=x-forwarded-for"
        );
    }

    let auth_session_sweep_trigger_secret = normalize_optional_secret(
        raw.auth_session_sweep_trigger_secret,
        "QM_AUTH_SESSION_SWEEP_TRIGGER_SECRET",
    )?;
    let expiry_reminder_trigger_secret = normalize_optional_secret(
        raw.expiry_reminder_trigger_secret,
        "QM_EXPIRY_REMINDER_TRIGGER_SECRET",
    )?;

    if raw.expiry_reminder_fire_hour > 23 {
        anyhow::bail!("QM_EXPIRY_REMINDER_FIRE_HOUR must be between 0 and 23");
    }
    if raw.expiry_reminder_fire_minute > 59 {
        anyhow::bail!("QM_EXPIRY_REMINDER_FIRE_MINUTE must be between 0 and 59");
    }
    if raw.expiry_reminder_lead_days < 0 {
        anyhow::bail!("QM_EXPIRY_REMINDER_LEAD_DAYS must be >= 0");
    }

    let api_config = Arc::new(ApiConfig {
        registration_mode: RegistrationMode::from_str(&raw.registration_mode)
            .map_err(anyhow::Error::msg)?,
        access_token_ttl_seconds: raw.access_token_ttl_seconds,
        refresh_token_ttl_seconds: raw.refresh_token_ttl_seconds,
        off_positive_ttl_days: raw.off_positive_ttl_days,
        off_negative_ttl_days: raw.off_negative_ttl_days,
        off_api_base_url: raw.off_api_base_url,
        public_base_url,
        rate_limit_client_ip_mode,
        rate_limit_trusted_proxy_cidrs: trusted_proxy_cidrs,
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
        auth_session_sweep_trigger_secret,
        expiry_reminder_policy: qm_db::reminders::ExpiryReminderPolicy {
            enabled: raw.expiry_reminders_enabled,
            lead_days: raw.expiry_reminder_lead_days,
            fire_hour: raw.expiry_reminder_fire_hour,
            fire_minute: raw.expiry_reminder_fire_minute,
        },
        expiry_reminder_trigger_secret,
    });

    Ok(LoadedConfig {
        bind,
        database_url: raw.database_url,
        log_format,
        api_config,
        auth_session_sweep_interval: (raw.auth_session_sweep_interval_seconds > 0)
            .then(|| Duration::from_secs(raw.auth_session_sweep_interval_seconds)),
        expiry_reminder_sweep_interval: (raw.expiry_reminder_sweep_interval_seconds > 0)
            .then(|| Duration::from_secs(raw.expiry_reminder_sweep_interval_seconds)),
    })
}

fn normalize_public_base_url(raw: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };

    let url = reqwest::Url::parse(&raw).context("parsing QM_PUBLIC_BASE_URL")?;
    if url.scheme() != "https" {
        anyhow::bail!("QM_PUBLIC_BASE_URL must use https");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("QM_PUBLIC_BASE_URL must not include user info");
    }
    if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
        anyhow::bail!("QM_PUBLIC_BASE_URL must be an origin without path, query, or fragment");
    }

    if url.host_str().is_none() {
        anyhow::bail!("QM_PUBLIC_BASE_URL must be an origin URL");
    }

    Ok(Some(url.origin().ascii_serialization()))
}

fn normalize_optional_secret(
    raw: Option<String>,
    env_name: &str,
) -> anyhow::Result<Option<String>> {
    match raw {
        Some(value) if value.trim().is_empty() => {
            anyhow::bail!("{env_name} must not be blank when set")
        }
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let raw = load_config()?;
    let loaded = build_config(raw)?;
    init_tracing(loaded.log_format);
    tracing::info!(
        bind = %loaded.bind,
        database_url = %loaded.database_url,
        rate_limit_client_ip_mode = %loaded.api_config.rate_limit_client_ip_mode.as_str(),
        "starting qm-server"
    );

    let db = Database::connect(&loaded.database_url)
        .await
        .context("connecting to database")?;
    db.migrate().await.context("running migrations")?;

    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(loaded.api_config.off_timeout)
        .build()
        .context("building HTTP client")?;

    let state = AppState {
        db,
        config: loaded.api_config.clone(),
        http,
        off_breaker: Arc::new(qm_api::openfoodfacts::OffCircuitBreaker::default()),
        rate_limiters: Arc::new(qm_api::rate_limit::RateLimiters::new(&loaded.api_config)),
    };

    if let Some(interval) = loaded.auth_session_sweep_interval {
        tokio::spawn(spawn_auth_session_sweeper(state.db.clone(), interval));
    }
    if let Some(interval) = loaded.expiry_reminder_sweep_interval {
        tokio::spawn(spawn_expiry_reminder_sweeper(
            state.db.clone(),
            loaded.api_config.expiry_reminder_policy.clone(),
            interval,
        ));
    }

    let app = qm_api::router(state);

    let listener = tokio::net::TcpListener::bind(loaded.bind)
        .await
        .with_context(|| format!("binding {}", loaded.bind))?;
    tracing::info!(addr = %loaded.bind, "listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serving HTTP")?;

    Ok(())
}

async fn spawn_auth_session_sweeper(db: Database, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;

    loop {
        ticker.tick().await;
        match qm_db::auth_sessions::delete_stale_sessions(
            &db,
            &qm_db::now_utc_rfc3339(),
            qm_db::auth_sessions::STALE_SESSION_SWEEP_BATCH_SIZE,
        )
        .await
        {
            Ok(deleted) => {
                tracing::info!(deleted_sessions = deleted, "completed auth session sweep")
            }
            Err(err) => tracing::error!(?err, "auth session sweep failed"),
        }
    }
}

async fn spawn_expiry_reminder_sweeper(
    db: Database,
    policy: qm_db::reminders::ExpiryReminderPolicy,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;

    loop {
        ticker.tick().await;
        match qm_db::reminders::reconcile_all(&db, &policy).await {
            Ok(stats) => tracing::info!(
                inserted = stats.inserted,
                deleted = stats.deleted,
                "completed expiry reminder sweep"
            ),
            Err(err) => tracing::error!(?err, "expiry reminder sweep failed"),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_https_public_base_url_to_origin() {
        let normalized =
            normalize_public_base_url(Some("https://quartermaster.example.com/".into())).unwrap();
        assert_eq!(
            normalized.as_deref(),
            Some("https://quartermaster.example.com")
        );
    }

    #[test]
    fn rejects_non_origin_public_base_url() {
        let err = normalize_public_base_url(Some(
            "https://quartermaster.example.com/join?invite=abc".into(),
        ))
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("QM_PUBLIC_BASE_URL must be an origin without path, query, or fragment"));
    }

    #[test]
    fn requires_trusted_proxy_cidrs_for_forwarded_mode() {
        let err = build_config(RawConfig {
            rate_limit_client_ip_mode: "x-forwarded-for".into(),
            ..RawConfig::default()
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("QM_RATE_LIMIT_TRUSTED_PROXY_CIDRS is required"));
    }

    #[test]
    fn parses_sweep_interval_and_secret() {
        let loaded = build_config(RawConfig {
            auth_session_sweep_interval_seconds: 60,
            auth_session_sweep_trigger_secret: Some("secret".into()),
            ..RawConfig::default()
        })
        .unwrap();
        assert_eq!(
            loaded.auth_session_sweep_interval,
            Some(Duration::from_secs(60))
        );
        assert_eq!(
            loaded
                .api_config
                .auth_session_sweep_trigger_secret
                .as_deref(),
            Some("secret")
        );
    }
}
