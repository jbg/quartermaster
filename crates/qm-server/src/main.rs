use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use ::metrics::counter;
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

mod metrics;
mod push;

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
    ios_team_id: Option<String>,
    ios_bundle_id: Option<String>,
    auth_session_sweep_interval_seconds: u64,
    auth_session_sweep_trigger_secret: Option<String>,
    smoke_seed_trigger_secret: Option<String>,
    expiry_reminders_enabled: bool,
    expiry_reminder_lead_days: i64,
    expiry_reminder_fire_hour: u32,
    expiry_reminder_fire_minute: u32,
    expiry_reminder_sweep_interval_seconds: u64,
    expiry_reminder_trigger_secret: Option<String>,
    push_worker_enabled: bool,
    push_worker_poll_interval_seconds: u64,
    push_worker_batch_size: i64,
    push_worker_claim_ttl_seconds: u64,
    push_worker_retry_backoff_seconds: u64,
    apns_enabled: bool,
    apns_environment: String,
    apns_topic: Option<String>,
    apns_auth_token: Option<String>,
    apns_key_id: Option<String>,
    apns_team_id: Option<String>,
    apns_private_key_path: Option<String>,
    apns_private_key: Option<String>,
    apns_base_url: Option<String>,
    fcm_enabled: bool,
    fcm_project_id: Option<String>,
    fcm_service_account_json_path: Option<String>,
    fcm_service_account_json: Option<String>,
    fcm_base_url: Option<String>,
    fcm_token_url: Option<String>,
    metrics_enabled: bool,
    metrics_bind: String,
    metrics_trigger_secret: Option<String>,
    web_dist_dir: Option<String>,
    web_auth_allowed_origins: Option<String>,
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
            ios_team_id: None,
            ios_bundle_id: None,
            auth_session_sweep_interval_seconds: 0,
            auth_session_sweep_trigger_secret: None,
            smoke_seed_trigger_secret: None,
            expiry_reminders_enabled: false,
            expiry_reminder_lead_days: 1,
            expiry_reminder_fire_hour: 9,
            expiry_reminder_fire_minute: 0,
            expiry_reminder_sweep_interval_seconds: 0,
            expiry_reminder_trigger_secret: None,
            push_worker_enabled: false,
            push_worker_poll_interval_seconds: 30,
            push_worker_batch_size: 25,
            push_worker_claim_ttl_seconds: 60,
            push_worker_retry_backoff_seconds: 300,
            apns_enabled: false,
            apns_environment: "sandbox".into(),
            apns_topic: None,
            apns_auth_token: None,
            apns_key_id: None,
            apns_team_id: None,
            apns_private_key_path: None,
            apns_private_key: None,
            apns_base_url: None,
            fcm_enabled: false,
            fcm_project_id: None,
            fcm_service_account_json_path: None,
            fcm_service_account_json: None,
            fcm_base_url: None,
            fcm_token_url: None,
            metrics_enabled: false,
            metrics_bind: "127.0.0.1:9091".into(),
            metrics_trigger_secret: None,
            web_dist_dir: Some("web/build".into()),
            web_auth_allowed_origins: None,
        }
    }
}

#[derive(Clone, Debug)]
struct MetricsConfig {
    enabled: bool,
    bind: SocketAddr,
    trigger_secret: Option<Arc<String>>,
    handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
}

#[derive(Debug)]
struct LoadedConfig {
    bind: SocketAddr,
    database_url: String,
    log_format: LogFormat,
    api_config: Arc<ApiConfig>,
    auth_session_sweep_interval: Option<Duration>,
    expiry_reminder_sweep_interval: Option<Duration>,
    push_worker_enabled: bool,
    push_worker_config: push::PushWorkerConfig,
    apns_config: push::ApnsConfig,
    fcm_config: push::FcmConfig,
    metrics_config: MetricsConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessMode {
    Serve,
    PushWorker,
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

impl FromStr for ProcessMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "serve" => Ok(Self::Serve),
            "push-worker" => Ok(Self::PushWorker),
            other => Err(format!("unknown process mode: {other}")),
        }
    }
}

fn parse_process_mode(args: &[String]) -> anyhow::Result<ProcessMode> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--mode" => {
                let value = iter.next().context("missing value after --mode")?;
                return ProcessMode::from_str(value).map_err(anyhow::Error::msg);
            }
            "serve" => return Ok(ProcessMode::Serve),
            "push-worker" => return Ok(ProcessMode::PushWorker),
            _ => {}
        }
    }
    Ok(ProcessMode::Serve)
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
    let smoke_seed_trigger_secret = normalize_optional_secret(
        raw.smoke_seed_trigger_secret,
        "QM_SMOKE_SEED_TRIGGER_SECRET",
    )?;
    let ios_release_identity = normalize_ios_release_identity(raw.ios_team_id, raw.ios_bundle_id)?;
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
    if raw.push_worker_batch_size <= 0 {
        anyhow::bail!("QM_PUSH_WORKER_BATCH_SIZE must be >= 1");
    }

    let apns_environment =
        push::ApnsEnvironment::from_str(&raw.apns_environment).map_err(anyhow::Error::msg)?;
    let apns_topic = match raw.apns_topic {
        Some(value) if value.trim().is_empty() => {
            anyhow::bail!("QM_APNS_TOPIC must not be blank when set")
        }
        Some(value) => Some(value),
        None => None,
    };
    let apns_auth_token = normalize_optional_secret(raw.apns_auth_token, "QM_APNS_AUTH_TOKEN")?;
    let apns_key_id = normalize_optional_secret(raw.apns_key_id, "QM_APNS_KEY_ID")?;
    let apns_team_id = normalize_optional_secret(raw.apns_team_id, "QM_APNS_TEAM_ID")?;
    let apns_private_key_path =
        normalize_optional_secret(raw.apns_private_key_path, "QM_APNS_PRIVATE_KEY_PATH")?;
    let apns_private_key = normalize_optional_secret(raw.apns_private_key, "QM_APNS_PRIVATE_KEY")?;
    let apns_jwt = build_apns_jwt_config(
        apns_auth_token.as_ref(),
        apns_key_id,
        apns_team_id,
        apns_private_key_path,
        apns_private_key,
    )?;
    let fcm_project_id = normalize_optional_secret(raw.fcm_project_id, "QM_FCM_PROJECT_ID")?;
    let fcm_service_account_json_path = normalize_optional_secret(
        raw.fcm_service_account_json_path,
        "QM_FCM_SERVICE_ACCOUNT_JSON_PATH",
    )?;
    let fcm_service_account_json =
        normalize_optional_secret(raw.fcm_service_account_json, "QM_FCM_SERVICE_ACCOUNT_JSON")?;
    if fcm_service_account_json_path.is_some() && fcm_service_account_json.is_some() {
        anyhow::bail!(
            "QM_FCM_SERVICE_ACCOUNT_JSON and QM_FCM_SERVICE_ACCOUNT_JSON_PATH must not both be set"
        );
    }
    let metrics_trigger_secret =
        normalize_optional_secret(raw.metrics_trigger_secret, "QM_METRICS_TRIGGER_SECRET")?;
    let web_dist_dir = raw
        .web_dist_dir
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    let metrics_bind = raw
        .metrics_bind
        .parse()
        .context("parsing QM_METRICS_BIND")?;
    if raw.metrics_enabled && metrics_trigger_secret.is_none() {
        anyhow::bail!("QM_METRICS_TRIGGER_SECRET is required when QM_METRICS_ENABLED=true");
    }
    let metrics_handle = if raw.metrics_enabled {
        Some(metrics::init_recorder()?)
    } else {
        None
    };
    let web_auth_allowed_origins =
        normalize_web_auth_allowed_origins(raw.web_auth_allowed_origins)?;

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
        ios_release_identity,
        auth_session_sweep_trigger_secret,
        expiry_reminder_policy: qm_db::reminders::ExpiryReminderPolicy {
            enabled: raw.expiry_reminders_enabled,
            lead_days: raw.expiry_reminder_lead_days,
            fire_hour: raw.expiry_reminder_fire_hour,
            fire_minute: raw.expiry_reminder_fire_minute,
        },
        expiry_reminder_trigger_secret,
        smoke_seed_trigger_secret,
        web_dist_dir,
        web_auth_allowed_origins,
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
        push_worker_enabled: raw.push_worker_enabled,
        push_worker_config: push::PushWorkerConfig {
            poll_interval: Duration::from_secs(raw.push_worker_poll_interval_seconds.max(1)),
            batch_size: raw.push_worker_batch_size,
            claim_ttl: Duration::from_secs(raw.push_worker_claim_ttl_seconds.max(1)),
            retry_backoff: Duration::from_secs(raw.push_worker_retry_backoff_seconds.max(1)),
        },
        apns_config: push::ApnsConfig {
            enabled: raw.apns_enabled,
            environment: apns_environment,
            topic: apns_topic,
            auth_token: apns_auth_token,
            jwt: apns_jwt,
            base_url: raw.apns_base_url,
        },
        fcm_config: push::FcmConfig::new(
            raw.fcm_enabled,
            fcm_project_id,
            fcm_service_account_json_path,
            fcm_service_account_json,
            raw.fcm_base_url,
            raw.fcm_token_url,
        ),
        metrics_config: MetricsConfig {
            enabled: raw.metrics_enabled,
            bind: metrics_bind,
            trigger_secret: metrics_trigger_secret.map(Arc::new),
            handle: metrics_handle,
        },
    })
}

fn normalize_web_auth_allowed_origins(raw: Option<String>) -> anyhow::Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let url = reqwest::Url::parse(value).context("parsing QM_WEB_AUTH_ALLOWED_ORIGINS")?;
            if url.scheme() != "https" {
                anyhow::bail!("QM_WEB_AUTH_ALLOWED_ORIGINS entries must use https");
            }
            if !url.username().is_empty() || url.password().is_some() {
                anyhow::bail!("QM_WEB_AUTH_ALLOWED_ORIGINS entries must not include user info");
            }
            if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
                anyhow::bail!(
                    "QM_WEB_AUTH_ALLOWED_ORIGINS entries must be origins without path, query, or fragment"
                );
            }
            if url.host_str().is_none() {
                anyhow::bail!("QM_WEB_AUTH_ALLOWED_ORIGINS entries must be origin URLs");
            }
            Ok(url.origin().ascii_serialization())
        })
        .collect()
}

fn normalize_public_base_url(raw: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };

    let url = reqwest::Url::parse(&raw).context("parsing QM_PUBLIC_BASE_URL")?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("QM_PUBLIC_BASE_URL must use http or https");
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

fn normalize_ios_release_identity(
    team_id: Option<String>,
    bundle_id: Option<String>,
) -> anyhow::Result<Option<qm_api::IosReleaseIdentity>> {
    match (
        normalize_optional_secret(team_id, "QM_IOS_TEAM_ID")?,
        normalize_optional_secret(bundle_id, "QM_IOS_BUNDLE_ID")?,
    ) {
        (None, None) => Ok(None),
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!("QM_IOS_TEAM_ID and QM_IOS_BUNDLE_ID must be set together")
        }
        (Some(team_id), Some(bundle_id)) => qm_api::IosReleaseIdentity::new(team_id, bundle_id)
            .map(Some)
            .map_err(anyhow::Error::msg),
    }
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

fn build_apns_jwt_config(
    auth_token: Option<&String>,
    key_id: Option<String>,
    team_id: Option<String>,
    private_key_path: Option<String>,
    private_key: Option<String>,
) -> anyhow::Result<Option<push::ApnsJwtConfig>> {
    if auth_token.is_some() {
        return Ok(None);
    }
    if private_key_path.is_some() && private_key.is_some() {
        anyhow::bail!("QM_APNS_PRIVATE_KEY and QM_APNS_PRIVATE_KEY_PATH must not both be set");
    }
    let any_jwt = key_id.is_some()
        || team_id.is_some()
        || private_key_path.is_some()
        || private_key.is_some();
    if !any_jwt {
        return Ok(None);
    }
    let key_id = key_id.context("QM_APNS_KEY_ID is required for APNs JWT auth")?;
    let team_id = team_id.context("QM_APNS_TEAM_ID is required for APNs JWT auth")?;
    let private_key = match (private_key, private_key_path) {
        (Some(value), None) => value,
        (None, Some(path)) => std::fs::read_to_string(&path)
            .with_context(|| format!("reading APNs private key from {path}"))?,
        (None, None) => anyhow::bail!(
            "QM_APNS_PRIVATE_KEY or QM_APNS_PRIVATE_KEY_PATH is required for APNs JWT auth"
        ),
        (Some(_), Some(_)) => unreachable!(),
    };
    Ok(Some(push::ApnsJwtConfig::new(key_id, team_id, private_key)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let process_mode = parse_process_mode(&args)?;
    let raw = load_config()?;
    let loaded = build_config(raw)?;
    init_tracing(loaded.log_format);
    tracing::info!(
        mode = ?process_mode,
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

    if process_mode == ProcessMode::PushWorker
        && !loaded.apns_config.is_ready()
        && !loaded.fcm_config.is_ready()
    {
        anyhow::bail!(
            "push-worker mode requires at least one configured push provider (APNs or FCM)"
        );
    }

    let state = AppState {
        db: db.clone(),
        config: loaded.api_config.clone(),
        http: http.clone(),
        off_breaker: Arc::new(qm_api::openfoodfacts::OffCircuitBreaker::default()),
        rate_limiters: Arc::new(qm_api::rate_limit::RateLimiters::new(&loaded.api_config)),
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut tasks = Vec::new();
    if process_mode == ProcessMode::Serve {
        if let Some(interval) = loaded.auth_session_sweep_interval {
            tasks.push(tokio::spawn(spawn_auth_session_sweeper(
                state.db.clone(),
                interval,
                shutdown_rx.clone(),
            )));
        }
        if let Some(interval) = loaded.expiry_reminder_sweep_interval {
            tasks.push(tokio::spawn(spawn_expiry_reminder_sweeper(
                state.db.clone(),
                loaded.api_config.expiry_reminder_policy.clone(),
                interval,
                shutdown_rx.clone(),
            )));
        }
        if loaded.push_worker_enabled
            && (loaded.apns_config.is_ready() || loaded.fcm_config.is_ready())
        {
            tasks.push(tokio::spawn(push::run_push_worker(
                state.db.clone(),
                http.clone(),
                loaded.apns_config.clone(),
                loaded.fcm_config.clone(),
                loaded.push_worker_config.clone(),
                loaded.metrics_config.handle.clone(),
                shutdown_rx.clone(),
            )));
        }

        let mut app = qm_api::router(state);
        if loaded.metrics_config.enabled {
            app = app.merge(metrics::internal_router(
                loaded
                    .metrics_config
                    .handle
                    .clone()
                    .expect("metrics handle"),
                loaded
                    .metrics_config
                    .trigger_secret
                    .clone()
                    .expect("metrics secret"),
                false,
            ));
        }
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
    } else {
        let worker = tokio::spawn(push::run_push_worker(
            db.clone(),
            http.clone(),
            loaded.apns_config.clone(),
            loaded.fcm_config.clone(),
            loaded.push_worker_config.clone(),
            loaded.metrics_config.handle.clone(),
            shutdown_rx.clone(),
        ));
        let worker_http = if loaded.metrics_config.enabled {
            Some(tokio::spawn(run_worker_http_server(
                loaded.metrics_config.clone(),
                shutdown_rx.clone(),
            )))
        } else {
            None
        };
        tokio::select! {
            result = worker => {
                result.context("joining push worker task")?;
            }
            result = async {
                if let Some(task) = worker_http {
                    task.await.context("joining worker HTTP task")?
                } else {
                    Ok(())
                }
            } => {
                result?;
            }
            _ = shutdown_signal() => {}
        }
    }

    let _ = shutdown_tx.send(true);
    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

async fn run_worker_http_server(
    metrics_config: MetricsConfig,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let router = metrics::internal_router(
        metrics_config.handle.expect("metrics handle"),
        metrics_config
            .trigger_secret
            .expect("metrics trigger secret"),
        true,
    );
    let listener = tokio::net::TcpListener::bind(metrics_config.bind)
        .await
        .with_context(|| format!("binding {}", metrics_config.bind))?;
    tracing::info!(addr = %metrics_config.bind, "worker metrics HTTP listening");
    axum::serve(listener, router.into_make_service())
        .with_graceful_shutdown(wait_for_shutdown(shutdown))
        .await
        .context("serving worker metrics HTTP")
}

async fn spawn_auth_session_sweeper(
    db: Database,
    interval: Duration,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match qm_db::auth_sessions::delete_stale_sessions(
                    &db,
                    &qm_db::now_utc_rfc3339(),
                    qm_db::auth_sessions::STALE_SESSION_SWEEP_BATCH_SIZE,
                )
                .await
                {
                    Ok(deleted) => {
                        counter!("qm_auth_session_sweeps_total", "surface" => "scheduled", "outcome" => "success")
                            .increment(1);
                        counter!("qm_auth_session_swept_sessions_total", "surface" => "scheduled")
                            .increment(deleted);
                        tracing::info!(deleted_sessions = deleted, "completed auth session sweep")
                    }
                    Err(err) => {
                        counter!("qm_auth_session_sweeps_total", "surface" => "scheduled", "outcome" => "failure")
                            .increment(1);
                        tracing::error!(?err, "auth session sweep failed")
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    tracing::info!("auth session sweeper shutting down");
                    break;
                }
            }
        }
    }
}

async fn spawn_expiry_reminder_sweeper(
    db: Database,
    policy: qm_db::reminders::ExpiryReminderPolicy,
    interval: Duration,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match qm_db::reminders::reconcile_all(&db, &policy).await {
                    Ok(stats) => {
                        counter!("qm_expiry_reminder_sweeps_total", "surface" => "scheduled", "outcome" => "success")
                            .increment(1);
                        counter!("qm_expiry_reminder_sweep_inserted_total", "surface" => "scheduled")
                            .increment(stats.inserted);
                        counter!("qm_expiry_reminder_sweep_deleted_total", "surface" => "scheduled")
                            .increment(stats.deleted);
                        tracing::info!(
                            inserted = stats.inserted,
                            deleted = stats.deleted,
                            "completed expiry reminder sweep"
                        )
                    }
                    Err(err) => {
                        counter!("qm_expiry_reminder_sweeps_total", "surface" => "scheduled", "outcome" => "failure")
                            .increment(1);
                        tracing::error!(?err, "expiry reminder sweep failed")
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    tracing::info!("expiry reminder sweeper shutting down");
                    break;
                }
            }
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

async fn wait_for_shutdown(mut shutdown: tokio::sync::watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow() {
            return;
        }
        if shutdown.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_public_base_url_to_origin() {
        let normalized =
            normalize_public_base_url(Some("http://quartermaster.local:8080/".into())).unwrap();
        assert_eq!(
            normalized.as_deref(),
            Some("http://quartermaster.local:8080")
        );

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
    fn normalizes_web_auth_allowed_origins() {
        let origins = normalize_web_auth_allowed_origins(Some(
            "https://web.example.com, https://admin.example.com/".into(),
        ))
        .unwrap();
        assert_eq!(
            origins,
            vec![
                "https://web.example.com".to_owned(),
                "https://admin.example.com".to_owned()
            ]
        );
    }

    #[test]
    fn rejects_non_https_web_auth_allowed_origins() {
        let err =
            normalize_web_auth_allowed_origins(Some("http://web.example.com".into())).unwrap_err();
        assert!(err
            .to_string()
            .contains("QM_WEB_AUTH_ALLOWED_ORIGINS entries must use https"));
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
            smoke_seed_trigger_secret: Some("smoke-secret".into()),
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
        assert_eq!(
            loaded.api_config.smoke_seed_trigger_secret.as_deref(),
            Some("smoke-secret")
        );
    }

    #[test]
    fn requires_metrics_secret_when_metrics_are_enabled() {
        let err = build_config(RawConfig {
            metrics_enabled: true,
            ..RawConfig::default()
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("QM_METRICS_TRIGGER_SECRET is required"));
    }

    #[test]
    fn requires_complete_ios_identity_pair() {
        let err = build_config(RawConfig {
            ios_team_id: Some("42J2SSX5SM".into()),
            ..RawConfig::default()
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("QM_IOS_TEAM_ID and QM_IOS_BUNDLE_ID must be set together"));
    }

    #[test]
    fn parses_ios_release_identity() {
        let loaded = build_config(RawConfig {
            ios_team_id: Some("42J2SSX5SM".into()),
            ios_bundle_id: Some("com.example.quartermaster".into()),
            ..RawConfig::default()
        })
        .unwrap();
        let identity = loaded.api_config.ios_release_identity.as_ref().unwrap();
        assert_eq!(identity.team_id(), "42J2SSX5SM");
        assert_eq!(identity.bundle_id(), "com.example.quartermaster");
    }

    #[test]
    fn rejects_multiple_fcm_service_account_sources() {
        let err = build_config(RawConfig {
            fcm_service_account_json_path: Some("/run/secrets/fcm.json".into()),
            fcm_service_account_json: Some("{}".into()),
            ..RawConfig::default()
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("QM_FCM_SERVICE_ACCOUNT_JSON and QM_FCM_SERVICE_ACCOUNT_JSON_PATH"));
    }

    #[test]
    fn apns_bearer_token_takes_precedence_over_jwt_config() {
        let loaded = build_config(RawConfig {
            apns_auth_token: Some("operator-token".into()),
            apns_key_id: Some("KEYID12345".into()),
            apns_team_id: Some("TEAMID1234".into()),
            apns_private_key: Some("unused".into()),
            ..RawConfig::default()
        })
        .unwrap();
        assert_eq!(
            loaded.apns_config.auth_token.as_deref(),
            Some("operator-token")
        );
        assert!(loaded.apns_config.jwt.is_none());
    }

    #[test]
    fn builds_apns_jwt_config_from_inline_private_key() {
        let loaded = build_config(RawConfig {
            apns_key_id: Some("KEYID12345".into()),
            apns_team_id: Some("TEAMID1234".into()),
            apns_private_key: Some("private-key".into()),
            ..RawConfig::default()
        })
        .unwrap();
        let jwt = loaded.apns_config.jwt.as_ref().unwrap();
        assert_eq!(jwt.key_id, "KEYID12345");
        assert_eq!(jwt.team_id, "TEAMID1234");
    }
}
