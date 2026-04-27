use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use jiff::{Timestamp, ToSpan};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use metrics_exporter_prometheus::PrometheusHandle;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{watch, Mutex};
use tracing::{debug, error, info, warn};

use crate::metrics;
use qm_db::{reminders, time, Database};

const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
const FCM_TOKEN_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApnsEnvironment {
    Sandbox,
    Production,
}

impl std::str::FromStr for ApnsEnvironment {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "sandbox" => Ok(Self::Sandbox),
            "production" => Ok(Self::Production),
            other => Err(format!("unknown apns environment: {other}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApnsConfig {
    pub enabled: bool,
    pub environment: ApnsEnvironment,
    pub topic: Option<String>,
    pub auth_token: Option<String>,
    pub jwt: Option<ApnsJwtConfig>,
    pub base_url: Option<String>,
}

impl ApnsConfig {
    pub fn is_ready(&self) -> bool {
        self.enabled && self.topic.is_some() && (self.auth_token.is_some() || self.jwt.is_some())
    }

    pub fn endpoint(&self) -> &'static str {
        match self.environment {
            ApnsEnvironment::Sandbox => "https://api.sandbox.push.apple.com",
            ApnsEnvironment::Production => "https://api.push.apple.com",
        }
    }

    pub fn base_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or_else(|| self.endpoint())
    }
}

#[derive(Clone, Debug)]
pub struct ApnsJwtConfig {
    pub key_id: String,
    pub team_id: String,
    pub private_key: String,
    auth_cache: Arc<Mutex<Option<CachedApnsAuthToken>>>,
}

impl ApnsJwtConfig {
    pub fn new(key_id: String, team_id: String, private_key: String) -> Self {
        Self {
            key_id,
            team_id,
            private_key,
            auth_cache: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FcmConfig {
    pub enabled: bool,
    pub project_id: Option<String>,
    pub service_account_json_path: Option<String>,
    pub service_account_json: Option<String>,
    pub base_url: Option<String>,
    pub token_url: Option<String>,
    auth_cache: Arc<Mutex<Option<CachedFcmAccessToken>>>,
}

impl FcmConfig {
    pub fn new(
        enabled: bool,
        project_id: Option<String>,
        service_account_json_path: Option<String>,
        service_account_json: Option<String>,
        base_url: Option<String>,
        token_url: Option<String>,
    ) -> Self {
        Self {
            enabled,
            project_id,
            service_account_json_path,
            service_account_json,
            base_url,
            token_url,
            auth_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.enabled
            && self.project_id.is_some()
            && (self.service_account_json_path.is_some() || self.service_account_json.is_some())
    }

    pub fn base_url(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or("https://fcm.googleapis.com")
    }

    pub fn token_url(&self) -> &str {
        self.token_url
            .as_deref()
            .unwrap_or("https://oauth2.googleapis.com/token")
    }

    fn project_id(&self) -> anyhow::Result<&str> {
        self.project_id
            .as_deref()
            .context("QM_FCM_PROJECT_ID is required when QM_FCM_ENABLED=true")
    }

    fn service_account_path(&self) -> anyhow::Result<PathBuf> {
        self.service_account_json_path
            .as_ref()
            .map(PathBuf::from)
            .context("QM_FCM_SERVICE_ACCOUNT_JSON_PATH is required when QM_FCM_ENABLED=true")
    }
}

#[derive(Clone, Debug)]
pub struct PushWorkerConfig {
    pub poll_interval: Duration,
    pub batch_size: i64,
    pub claim_ttl: Duration,
    pub retry_backoff: Duration,
}

#[derive(Debug)]
struct PushSendOutcome {
    channel: String,
    status: &'static str,
    metric_outcome: &'static str,
    provider_message_id: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    next_retry_at: Option<String>,
    transport_error: bool,
}

#[derive(Debug, Default)]
struct ChannelStats {
    successful_attempts: u64,
    retryable_attempts: u64,
    permanent_attempts: u64,
    transport_failures: u64,
}

#[derive(Debug, Deserialize)]
struct ApnsErrorBody {
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FcmTokenResponse {
    access_token: String,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct FcmSuccessBody {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FcmErrorResponse {
    error: Option<FcmErrorEnvelope>,
}

#[derive(Debug, Deserialize)]
struct FcmErrorEnvelope {
    code: Option<i64>,
    status: Option<String>,
    message: Option<String>,
    details: Option<Vec<FcmErrorDetail>>,
}

#[derive(Debug, Deserialize)]
struct FcmErrorDetail {
    #[serde(rename = "@type")]
    type_url: Option<String>,
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServiceAccountJson {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
}

#[derive(Clone, Debug)]
struct CachedFcmAccessToken {
    access_token: String,
    refresh_at: Instant,
}

#[derive(Clone, Debug)]
struct CachedApnsAuthToken {
    auth_token: String,
    refresh_at: Instant,
}

#[derive(Debug, Serialize)]
struct ApnsJwtClaims<'a> {
    iss: &'a str,
    iat: usize,
}

#[derive(Debug, Serialize)]
struct FcmJwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    exp: usize,
    iat: usize,
}

pub async fn run_push_worker(
    db: Database,
    http: reqwest::Client,
    apns: ApnsConfig,
    fcm: FcmConfig,
    worker: PushWorkerConfig,
    _metrics_handle: Option<PrometheusHandle>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(worker.poll_interval);
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                metrics::record_cycle_started();
                if let Err(err) = run_push_cycle(&db, &http, &apns, &fcm, &worker).await {
                    metrics::record_cycle_failed();
                    error!(?err, "push worker cycle failed");
                }
            }
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    info!("push worker shutting down");
                    break;
                }
            }
        }
    }
}

pub async fn run_push_cycle(
    db: &Database,
    http: &reqwest::Client,
    apns: &ApnsConfig,
    fcm: &FcmConfig,
    worker: &PushWorkerConfig,
) -> anyhow::Result<()> {
    if !apns.is_ready() && !fcm.is_ready() {
        debug!("push worker skipped: no push providers are configured");
        return Ok(());
    }

    let cycle_started = Instant::now();
    let now = Timestamp::now();
    let now_rfc3339 = time::format_timestamp(now);
    let before_summary = metrics::refresh_delivery_gauges(db, &now_rfc3339).await?;
    let retry_at = time::format_timestamp(
        now.checked_add(
            i64::try_from(worker.retry_backoff.as_secs())
                .unwrap_or(i64::MAX)
                .seconds(),
        )
        .context("computing push retry time")?,
    );
    let claim_until = time::format_timestamp(
        now.checked_add(
            i64::try_from(worker.claim_ttl.as_secs())
                .unwrap_or(i64::MAX)
                .seconds(),
        )
        .context("computing push claim expiry")?,
    );

    let expired = reminders::expire_stale_push_claims(db, &now_rfc3339, &retry_at).await?;
    let claimed =
        reminders::claim_due_push_work(db, &now_rfc3339, worker.batch_size, &claim_until).await?;
    metrics::record_expired_claims(expired);
    metrics::record_claimed(claimed.items.len() as u64);
    metrics::record_claim_conflicts(claimed.claim_conflicts);

    let mut channel_stats = BTreeMap::<String, ChannelStats>::new();

    for item in claimed.items {
        let send_started = Instant::now();
        let outcome = match item.channel.as_str() {
            reminders::CHANNEL_APNS => match send_apns(http, apns, &item, &retry_at).await {
                Ok(outcome) => outcome,
                Err(err) => transport_failure_outcome(&item.channel, err, &retry_at),
            },
            reminders::CHANNEL_FCM => match send_fcm(http, fcm, &item, &retry_at).await {
                Ok(outcome) => outcome,
                Err(err) => transport_failure_outcome(&item.channel, err, &retry_at),
            },
            other => {
                warn!(
                    reminder_id = %item.reminder_id,
                    device_id = %item.device_row_id,
                    channel = other,
                    "skipping push delivery for unsupported provider"
                );
                continue;
            }
        };

        metrics::record_send_duration(&outcome.channel, send_started.elapsed().as_secs_f64());
        metrics::record_attempt(&outcome.channel, outcome.metric_outcome);

        let stats = channel_stats.entry(outcome.channel.clone()).or_default();
        if outcome.transport_error {
            metrics::record_transport_failure(&outcome.channel);
            stats.transport_failures += 1;
        }
        match outcome.status {
            reminders::DELIVERY_STATUS_SUCCEEDED => stats.successful_attempts += 1,
            reminders::DELIVERY_STATUS_FAILED_RETRYABLE => stats.retryable_attempts += 1,
            reminders::DELIVERY_STATUS_FAILED_PERMANENT => stats.permanent_attempts += 1,
            _ => {}
        }

        reminders::complete_push_attempt(
            db,
            &item,
            &reminders::PushDeliveryResult {
                channel: outcome.channel,
                status: outcome.status,
                finished_at: time::format_timestamp(Timestamp::now()),
                next_retry_at: outcome.next_retry_at,
                provider_message_id: outcome.provider_message_id,
                error_code: outcome.error_code,
                error_message: outcome.error_message,
            },
        )
        .await?;
    }

    let after_summary =
        metrics::refresh_delivery_gauges(db, &time::format_timestamp(Timestamp::now())).await?;
    metrics::record_cycle_duration(cycle_started.elapsed().as_secs_f64());
    metrics::record_last_cycle_completed(unix_timestamp_seconds());

    let claimed_count: u64 = channel_stats
        .values()
        .map(|stats| {
            stats.successful_attempts
                + stats.retryable_attempts
                + stats.permanent_attempts
                + stats.transport_failures
        })
        .sum();
    info!(
        due_before = before_summary.due_count,
        due_after = after_summary.due_count,
        retry_due_after = after_summary.retry_due_count,
        active_claims_after = after_summary.active_claim_count,
        failed_retryable_after = after_summary.failed_retryable_count,
        failed_permanent_after = after_summary.failed_permanent_count,
        invalid_tokens_after = after_summary.invalid_token_count,
        expired_claims = expired,
        claim_conflicts = claimed.claim_conflicts,
        claimed = claimed_count,
        channel_stats = ?channel_stats,
        "push worker cycle completed"
    );

    Ok(())
}

fn transport_failure_outcome(channel: &str, err: anyhow::Error, retry_at: &str) -> PushSendOutcome {
    warn!(?err, channel, "push send failed before response");
    PushSendOutcome {
        channel: channel.to_owned(),
        status: reminders::DELIVERY_STATUS_FAILED_RETRYABLE,
        metric_outcome: "failed_retryable",
        provider_message_id: None,
        error_code: Some("transport_error".into()),
        error_message: Some(err.to_string()),
        next_retry_at: Some(retry_at.to_owned()),
        transport_error: true,
    }
}

async fn send_apns(
    http: &reqwest::Client,
    apns: &ApnsConfig,
    item: &reminders::PushWorkItem,
    retry_at: &str,
) -> anyhow::Result<PushSendOutcome> {
    if !apns.is_ready() {
        anyhow::bail!("APNs is not configured");
    }

    let url = format!("{}/3/device/{}", apns.base_url(), item.device_token);
    let mut request = http
        .post(url)
        .header("apns-push-type", "alert")
        .header("content-type", "application/json");
    if let Some(topic) = &apns.topic {
        request = request.header("apns-topic", topic);
    }
    if let Some(token) = &apns.auth_token {
        request = request.bearer_auth(token);
    } else if let Some(jwt) = &apns.jwt {
        request = request.bearer_auth(apns_auth_token(jwt).await?);
    }

    let alert = if let Some(expires_on) = item.expires_on.as_deref() {
        json!({
            "title-loc-key": "EXPIRY_REMINDER_TITLE",
            "title-loc-args": [item.product_name, item.location_name],
            "loc-key": "EXPIRY_REMINDER_BODY",
            "loc-args": [item.quantity, item.unit, expires_on],
        })
    } else {
        json!({
            "title-loc-key": "EXPIRY_REMINDER_TITLE",
            "title-loc-args": [item.product_name, item.location_name],
            "loc-key": "EXPIRY_REMINDER_BODY_NO_DATE",
            "loc-args": [item.quantity, item.unit],
        })
    };
    let payload = json!({
        "aps": {
            "alert": alert,
            "sound": "default",
        },
        "reminder_id": item.reminder_id,
        "batch_id": item.batch_id,
        "product_id": item.product_id,
        "location_id": item.location_id,
        "kind": item.kind,
        "product_name": item.product_name,
        "location_name": item.location_name,
        "quantity": item.quantity,
        "unit": item.unit,
        "expires_on": item.expires_on,
    });
    let response = request
        .json(&payload)
        .send()
        .await
        .context("sending APNs request")?;
    let apns_id = response
        .headers()
        .get("apns-id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Ok(classify_apns_response(
        status, apns_id, body, retry_at, item,
    ))
}

async fn send_fcm(
    http: &reqwest::Client,
    fcm: &FcmConfig,
    item: &reminders::PushWorkItem,
    retry_at: &str,
) -> anyhow::Result<PushSendOutcome> {
    if !fcm.is_ready() {
        anyhow::bail!("FCM is not configured");
    }

    let access_token = fcm_access_token(http, fcm).await?;
    let project_id = fcm.project_id()?;
    let url = format!(
        "{}/v1/projects/{project_id}/messages:send",
        fcm.base_url().trim_end_matches('/')
    );
    let payload = json!({
        "message": {
            "token": item.device_token,
            "android": {
                "priority": "HIGH",
                "notification": {
                    "channel_id": "expiry_reminders"
                }
            },
            "data": {
                "reminder_id": item.reminder_id.to_string(),
                "batch_id": item.batch_id.to_string(),
                "product_id": item.product_id.to_string(),
                "location_id": item.location_id.to_string(),
                "kind": item.kind.clone(),
                "product_name": item.product_name.clone(),
                "location_name": item.location_name.clone(),
                "quantity": item.quantity.clone(),
                "unit": item.unit.clone(),
                "expires_on": item.expires_on.clone().unwrap_or_default()
            }
        }
    });
    let response = http
        .post(url)
        .bearer_auth(access_token)
        .json(&payload)
        .send()
        .await
        .context("sending FCM request")?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Ok(classify_fcm_response(status, body, retry_at))
}

async fn fcm_access_token(http: &reqwest::Client, fcm: &FcmConfig) -> anyhow::Result<String> {
    let mut cache = fcm.auth_cache.lock().await;
    if let Some(cached) = cache.as_ref() {
        if Instant::now() < cached.refresh_at {
            return Ok(cached.access_token.clone());
        }
    }

    let credentials = read_service_account_json(fcm).await?;
    let audience = credentials
        .token_uri
        .clone()
        .unwrap_or_else(|| fcm.token_url().to_owned());
    let assertion = build_fcm_jwt_assertion(&credentials, &audience)?;
    let response = http
        .post(fcm.token_url())
        .form(&[
            ("grant_type", FCM_TOKEN_GRANT_TYPE),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .await
        .context("requesting FCM access token")?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "FCM token exchange failed with HTTP {}: {}",
            status.as_u16(),
            body
        );
    }
    let parsed: FcmTokenResponse =
        serde_json::from_str(&body).context("parsing FCM token response")?;
    let refresh_in = parsed.expires_in.saturating_sub(60).max(1) as u64;
    let access_token = parsed.access_token;
    *cache = Some(CachedFcmAccessToken {
        access_token: access_token.clone(),
        refresh_at: Instant::now() + Duration::from_secs(refresh_in),
    });
    Ok(access_token)
}

async fn apns_auth_token(jwt: &ApnsJwtConfig) -> anyhow::Result<String> {
    let mut cache = jwt.auth_cache.lock().await;
    if let Some(cached) = cache.as_ref() {
        if Instant::now() < cached.refresh_at {
            return Ok(cached.auth_token.clone());
        }
    }

    let issued_at = unix_timestamp_seconds() as usize;
    let token = build_apns_jwt(jwt, issued_at)?;
    *cache = Some(CachedApnsAuthToken {
        auth_token: token.clone(),
        refresh_at: Instant::now() + Duration::from_secs(50 * 60),
    });
    Ok(token)
}

fn build_apns_jwt(jwt: &ApnsJwtConfig, issued_at: usize) -> anyhow::Result<String> {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(jwt.key_id.clone());
    let claims = ApnsJwtClaims {
        iss: &jwt.team_id,
        iat: issued_at,
    };
    let key =
        EncodingKey::from_ec_pem(jwt.private_key.as_bytes()).context("loading APNs private key")?;
    jsonwebtoken::encode(&header, &claims, &key).context("encoding APNs JWT")
}

async fn read_service_account_json(fcm: &FcmConfig) -> anyhow::Result<ServiceAccountJson> {
    if let Some(raw) = &fcm.service_account_json {
        return serde_json::from_str(raw).context("parsing FCM service account JSON");
    }
    let path = fcm.service_account_path()?;
    let raw = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("reading FCM service account JSON from {}", path.display()))?;
    serde_json::from_str(&raw).context("parsing FCM service account JSON")
}

fn build_fcm_jwt_assertion(
    credentials: &ServiceAccountJson,
    audience: &str,
) -> anyhow::Result<String> {
    let issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as usize;
    let claims = FcmJwtClaims {
        iss: &credentials.client_email,
        scope: FCM_SCOPE,
        aud: audience,
        iat: issued_at,
        exp: issued_at + 3600,
    };
    let key = EncodingKey::from_rsa_pem(credentials.private_key.as_bytes())
        .context("loading FCM service account private key")?;
    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key)
        .context("encoding FCM JWT assertion")
}

fn unix_timestamp_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

fn classify_apns_response(
    status: StatusCode,
    apns_id: Option<String>,
    body: String,
    retry_at: &str,
    item: &reminders::PushWorkItem,
) -> PushSendOutcome {
    if status.is_success() {
        info!(
            reminder_id = %item.reminder_id,
            device_id = %item.device_row_id,
            apns_id = apns_id.as_deref().unwrap_or(""),
            "push delivery succeeded"
        );
        return PushSendOutcome {
            channel: reminders::CHANNEL_APNS.into(),
            status: reminders::DELIVERY_STATUS_SUCCEEDED,
            metric_outcome: "succeeded",
            provider_message_id: apns_id,
            error_code: None,
            error_message: None,
            next_retry_at: None,
            transport_error: false,
        };
    }

    let parsed = serde_json::from_str::<ApnsErrorBody>(&body).ok();
    let error_code = parsed
        .as_ref()
        .and_then(|value| value.reason.clone())
        .unwrap_or_else(|| format!("http_{}", status.as_u16()));
    let permanent = matches!(status.as_u16(), 400 | 403 | 404 | 410);
    let error_message = if body.is_empty() { None } else { Some(body) };
    PushSendOutcome {
        channel: reminders::CHANNEL_APNS.into(),
        status: if permanent {
            reminders::DELIVERY_STATUS_FAILED_PERMANENT
        } else {
            reminders::DELIVERY_STATUS_FAILED_RETRYABLE
        },
        metric_outcome: if permanent {
            "failed_permanent"
        } else {
            "failed_retryable"
        },
        provider_message_id: apns_id,
        error_code: Some(error_code),
        error_message,
        next_retry_at: if permanent {
            None
        } else {
            Some(retry_at.to_owned())
        },
        transport_error: false,
    }
}

fn classify_fcm_response(status: StatusCode, body: String, retry_at: &str) -> PushSendOutcome {
    if status.is_success() {
        let provider_message_id = serde_json::from_str::<FcmSuccessBody>(&body)
            .ok()
            .and_then(|value| value.name);
        return PushSendOutcome {
            channel: reminders::CHANNEL_FCM.into(),
            status: reminders::DELIVERY_STATUS_SUCCEEDED,
            metric_outcome: "succeeded",
            provider_message_id,
            error_code: None,
            error_message: None,
            next_retry_at: None,
            transport_error: false,
        };
    }

    let parsed = serde_json::from_str::<FcmErrorResponse>(&body).ok();
    let normalized = normalize_fcm_error(status, parsed.as_ref());
    let permanent = matches!(normalized.as_str(), "invalid_token" | "unregistered")
        || matches!(status.as_u16(), 400 | 403 | 404);
    let error_message = parsed
        .as_ref()
        .and_then(|value| value.error.as_ref())
        .and_then(|value| value.message.clone())
        .or_else(|| if body.is_empty() { None } else { Some(body) });
    PushSendOutcome {
        channel: reminders::CHANNEL_FCM.into(),
        status: if permanent {
            reminders::DELIVERY_STATUS_FAILED_PERMANENT
        } else {
            reminders::DELIVERY_STATUS_FAILED_RETRYABLE
        },
        metric_outcome: if permanent {
            "failed_permanent"
        } else {
            "failed_retryable"
        },
        provider_message_id: None,
        error_code: Some(normalized),
        error_message,
        next_retry_at: if permanent {
            None
        } else {
            Some(retry_at.to_owned())
        },
        transport_error: false,
    }
}

fn normalize_fcm_error(status: StatusCode, parsed: Option<&FcmErrorResponse>) -> String {
    let Some(envelope) = parsed.and_then(|value| value.error.as_ref()) else {
        return format!("http_{}", status.as_u16());
    };

    if let Some(details) = &envelope.details {
        for detail in details {
            if detail
                .type_url
                .as_deref()
                .is_some_and(|value| value.contains("FcmError"))
            {
                match detail.error_code.as_deref() {
                    Some("UNREGISTERED") => return "unregistered".into(),
                    Some("INVALID_ARGUMENT") => return "invalid_token".into(),
                    Some(code) => return code.to_ascii_lowercase(),
                    None => {}
                }
            }
        }
    }

    match envelope.status.as_deref() {
        Some("UNAUTHENTICATED") | Some("INTERNAL") | Some("UNAVAILABLE") => {
            format!("http_{}", status.as_u16())
        }
        Some("NOT_FOUND") => "unregistered".into(),
        Some("INVALID_ARGUMENT") => "invalid_token".into(),
        Some(other) => other.to_ascii_lowercase(),
        None => envelope
            .code
            .map(|code| format!("http_{code}"))
            .unwrap_or_else(|| format!("http_{}", status.as_u16())),
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode as AxumStatusCode},
    };
    use axum::{
        extract::{Path, State},
        http::HeaderMap,
        response::IntoResponse,
        routing::post,
        Router,
    };
    use base64::Engine;
    use serde_json::Value;
    use sqlx::Row;
    use std::{collections::VecDeque, sync::Arc};
    use tokio::sync::{oneshot, Mutex};
    use tower::util::ServiceExt;
    use uuid::Uuid;

    use super::*;
    use qm_api::{ApiConfig, AppState};
    use qm_db::{
        auth_sessions,
        devices::{self, DeviceUpsert},
        households, locations, memberships, products, stock, users,
    };

    async fn setup_push_fixture() -> (Database, Uuid, Uuid, Uuid, Uuid) {
        let db = qm_db::test_support::sqlite().await.into_db();
        let household = households::create(&db, "Home", "UTC").await.unwrap();
        locations::seed_defaults(&db, household.id).await.unwrap();
        let pantry = locations::list_for_household(&db, household.id)
            .await
            .unwrap()
            .into_iter()
            .find(|row| row.kind == "pantry")
            .unwrap()
            .id;
        let user = users::create(&db, "alice", Some("alice@example.com"), "hash")
            .await
            .unwrap();
        memberships::insert(&db, household.id, user.id, "admin")
            .await
            .unwrap();
        let product = products::create_manual(
            &db,
            household.id,
            "Milk",
            None,
            "volume",
            Some("ml"),
            None,
            None,
        )
        .await
        .unwrap();
        (db, household.id, user.id, pantry, product.id)
    }

    async fn seed_due_reminder(
        db: &Database,
        household_id: Uuid,
        user_id: Uuid,
        pantry: Uuid,
        product_id: Uuid,
        token: &str,
        platform: &str,
        device_id: &str,
    ) -> Uuid {
        let batch = stock::create(
            db,
            household_id,
            product_id,
            pantry,
            "1",
            "ml",
            Some("2999-01-03"),
            None,
            None,
            user_id,
            Some(&qm_db::reminders::ExpiryReminderPolicy {
                enabled: true,
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE stock_reminder SET fire_at = ? WHERE batch_id = ?")
            .bind("2000-01-01T00:00:00.000Z")
            .bind(batch.id.to_string())
            .execute(&db.pool)
            .await
            .unwrap();
        let session_id = Uuid::now_v7();
        auth_sessions::upsert(db, session_id, user_id, Some(household_id))
            .await
            .unwrap();
        devices::upsert(
            db,
            &DeviceUpsert {
                user_id,
                session_id,
                device_id: device_id.into(),
                platform: platform.into(),
                push_token: Some(token.into()),
                push_authorization: "authorized".into(),
                app_version: Some("0.1".into()),
            },
        )
        .await
        .unwrap();
        batch.id
    }

    fn worker_config() -> PushWorkerConfig {
        PushWorkerConfig {
            poll_interval: Duration::from_secs(1),
            batch_size: 10,
            claim_ttl: Duration::from_secs(60),
            retry_backoff: Duration::from_secs(300),
        }
    }

    fn test_ec_private_key() -> &'static str {
        "-----BEGIN PRIVATE KEY-----\n\
         MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg/C2dD7bdjAs0Ej3R\n\
         z7QcUPqN0Koqig35tv/q8Ozdu3KhRANCAAQ+r0jjCurTGYgQsy8YSs2pgOEuuE9u\n\
         +NQBWq5/ZD3+zfy0M0PLvlgQM+9IUlbTlcpCdQvey5MS6T4pyYsV2Mu7\n\
         -----END PRIVATE KEY-----\n"
    }

    fn apns_config(base_url: String) -> ApnsConfig {
        ApnsConfig {
            enabled: true,
            environment: ApnsEnvironment::Sandbox,
            topic: Some("com.example.quartermaster".into()),
            auth_token: Some("token".into()),
            jwt: None,
            base_url: Some(base_url),
        }
    }

    fn disabled_apns_config() -> ApnsConfig {
        ApnsConfig {
            enabled: false,
            environment: ApnsEnvironment::Sandbox,
            topic: None,
            auth_token: None,
            jwt: None,
            base_url: None,
        }
    }

    async fn fcm_config(base_url: String) -> FcmConfig {
        let config = FcmConfig::new(
            true,
            Some("quartermaster-test".into()),
            Some("/tmp/unused-service-account.json".into()),
            None,
            Some(base_url),
            Some("http://127.0.0.1:9/token".into()),
        );
        *config.auth_cache.lock().await = Some(CachedFcmAccessToken {
            access_token: "ya29.cached".into(),
            refresh_at: Instant::now() + Duration::from_secs(60),
        });
        config
    }

    #[derive(Clone, Debug)]
    struct CapturedProviderRequest {
        channel: &'static str,
        path: String,
        authorization: Option<String>,
        body: Value,
    }

    #[derive(Clone, Debug)]
    struct FakeProviderResponse {
        status: StatusCode,
        body: Value,
        provider_message_id: Option<String>,
    }

    #[derive(Clone, Default)]
    struct FakeProviderState {
        apns_responses: Arc<Mutex<VecDeque<FakeProviderResponse>>>,
        fcm_responses: Arc<Mutex<VecDeque<FakeProviderResponse>>>,
        captures: Arc<Mutex<Vec<CapturedProviderRequest>>>,
    }

    struct FakeProviderServer {
        state: FakeProviderState,
        base_url: String,
        shutdown: Option<oneshot::Sender<()>>,
        task: tokio::task::JoinHandle<()>,
    }

    impl FakeProviderServer {
        async fn start(
            apns_responses: Vec<FakeProviderResponse>,
            fcm_responses: Vec<FakeProviderResponse>,
        ) -> Self {
            let state = FakeProviderState {
                apns_responses: Arc::new(Mutex::new(VecDeque::from(apns_responses))),
                fcm_responses: Arc::new(Mutex::new(VecDeque::from(fcm_responses))),
                captures: Arc::new(Mutex::new(Vec::new())),
            };
            let router = Router::new()
                .route("/3/device/{token}", post(fake_apns))
                .route("/v1/projects/{project_id}/messages:send", post(fake_fcm))
                .with_state(state.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let task = tokio::spawn(async move {
                let _ = axum::serve(listener, router)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                    })
                    .await;
            });
            Self {
                state,
                base_url,
                shutdown: Some(shutdown_tx),
                task,
            }
        }

        async fn captures(&self) -> Vec<CapturedProviderRequest> {
            self.state.captures.lock().await.clone()
        }
    }

    impl Drop for FakeProviderServer {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
            self.task.abort();
        }
    }

    async fn fake_apns(
        Path(token): Path<String>,
        State(state): State<FakeProviderState>,
        headers: HeaderMap,
        axum::Json(body): axum::Json<Value>,
    ) -> impl IntoResponse {
        state.captures.lock().await.push(CapturedProviderRequest {
            channel: reminders::CHANNEL_APNS,
            path: format!("/3/device/{token}"),
            authorization: headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
            body,
        });
        let response =
            state
                .apns_responses
                .lock()
                .await
                .pop_front()
                .unwrap_or(FakeProviderResponse {
                    status: StatusCode::OK,
                    body: json!({}),
                    provider_message_id: Some("default-apns-id".into()),
                });
        let mut builder = axum::http::Response::builder().status(response.status);
        if let Some(message_id) = response.provider_message_id {
            builder = builder.header("apns-id", message_id);
        }
        builder
            .body(axum::Json(response.body).into_response().into_body())
            .unwrap()
    }

    async fn fake_fcm(
        Path(project_id): Path<String>,
        State(state): State<FakeProviderState>,
        headers: HeaderMap,
        axum::Json(body): axum::Json<Value>,
    ) -> impl IntoResponse {
        state.captures.lock().await.push(CapturedProviderRequest {
            channel: reminders::CHANNEL_FCM,
            path: format!("/v1/projects/{project_id}/messages:send"),
            authorization: headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
            body,
        });
        let response =
            state
                .fcm_responses
                .lock()
                .await
                .pop_front()
                .unwrap_or(FakeProviderResponse {
                    status: StatusCode::OK,
                    body: json!({"name": "projects/quartermaster-test/messages/default"}),
                    provider_message_id: None,
                });
        (response.status, axum::Json(response.body))
    }

    async fn metrics_snapshot() -> String {
        metrics::init_recorder().unwrap().render()
    }

    fn dummy_item(channel: &str) -> reminders::PushWorkItem {
        reminders::PushWorkItem {
            attempt_id: Uuid::now_v7(),
            channel: channel.into(),
            reminder_id: Uuid::now_v7(),
            household_id: Uuid::now_v7(),
            batch_id: Uuid::now_v7(),
            product_id: Uuid::now_v7(),
            location_id: Uuid::now_v7(),
            kind: reminders::KIND_EXPIRY.into(),
            expires_on: Some("2999-01-03".into()),
            product_name: "Milk".into(),
            location_name: "Pantry".into(),
            quantity: "1".into(),
            unit: "carton".into(),
            device_row_id: Uuid::now_v7(),
            device_token: "token".into(),
        }
    }

    #[tokio::test]
    async fn classify_apns_response_marks_success() {
        let dummy = dummy_item(reminders::CHANNEL_APNS);
        let outcome = classify_apns_response(
            StatusCode::OK,
            Some("test-apns-id".into()),
            String::new(),
            "2000-01-01T00:05:00.000Z",
            &dummy,
        );
        assert_eq!(outcome.channel, reminders::CHANNEL_APNS);
        assert_eq!(outcome.status, reminders::DELIVERY_STATUS_SUCCEEDED);
        assert_eq!(outcome.metric_outcome, "succeeded");
        assert_eq!(outcome.provider_message_id.as_deref(), Some("test-apns-id"));
    }

    #[tokio::test]
    async fn classify_fcm_response_marks_invalid_token_as_permanent() {
        let outcome = classify_fcm_response(
            StatusCode::BAD_REQUEST,
            json!({
                "error": {
                    "code": 400,
                    "status": "INVALID_ARGUMENT",
                    "message": "bad token",
                    "details": [
                        {
                            "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError",
                            "errorCode": "INVALID_ARGUMENT"
                        }
                    ]
                }
            })
            .to_string(),
            "2000-01-01T00:05:00.000Z",
        );
        assert_eq!(outcome.channel, reminders::CHANNEL_FCM);
        assert_eq!(outcome.status, reminders::DELIVERY_STATUS_FAILED_PERMANENT);
        assert_eq!(outcome.error_code.as_deref(), Some("invalid_token"));
    }

    #[tokio::test]
    async fn classify_fcm_response_marks_retryable_failure() {
        let outcome = classify_fcm_response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
                "error": {
                    "code": 503,
                    "status": "UNAVAILABLE",
                    "message": "retry later"
                }
            })
            .to_string(),
            "2000-01-01T00:05:00.000Z",
        );
        assert_eq!(outcome.status, reminders::DELIVERY_STATUS_FAILED_RETRYABLE);
        assert_eq!(
            outcome.next_retry_at.as_deref(),
            Some("2000-01-01T00:05:00.000Z")
        );
    }

    #[tokio::test]
    async fn fcm_access_token_reuses_cached_token() {
        let fcm = FcmConfig::new(
            true,
            Some("quartermaster-test".into()),
            Some("/tmp/unused-service-account.json".into()),
            None,
            None,
            None,
        );
        *fcm.auth_cache.lock().await = Some(CachedFcmAccessToken {
            access_token: "ya29.cached".into(),
            refresh_at: Instant::now() + Duration::from_secs(60),
        });

        let http = reqwest::Client::new();
        let first = fcm_access_token(&http, &fcm).await.unwrap();
        let second = fcm_access_token(&http, &fcm).await.unwrap();
        assert_eq!(first, "ya29.cached");
        assert_eq!(second, "ya29.cached");
    }

    #[test]
    fn build_apns_jwt_sets_expected_header_and_claims() {
        let jwt = ApnsJwtConfig::new(
            "KEYID12345".into(),
            "TEAMID1234".into(),
            test_ec_private_key().into(),
        );
        let token = build_apns_jwt(&jwt, 1_776_000_000).unwrap();
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.alg, Algorithm::ES256);
        assert_eq!(header.kid.as_deref(), Some("KEYID12345"));

        let payload = token.split('.').nth(1).unwrap();
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .unwrap();
        let claims: Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(claims["iss"], "TEAMID1234");
        assert_eq!(claims["iat"], 1_776_000_000);
    }

    #[tokio::test]
    async fn apns_auth_token_reuses_cached_token() {
        let jwt = ApnsJwtConfig::new(
            "KEYID12345".into(),
            "TEAMID1234".into(),
            test_ec_private_key().into(),
        );
        let first = apns_auth_token(&jwt).await.unwrap();
        let second = apns_auth_token(&jwt).await.unwrap();
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn fcm_service_account_can_be_read_from_inline_json() {
        let fcm = FcmConfig::new(
            true,
            Some("quartermaster-test".into()),
            None,
            Some(
                json!({
                    "client_email": "firebase@example.iam.gserviceaccount.com",
                    "private_key": "private-key",
                    "token_uri": "https://oauth2.example/token"
                })
                .to_string(),
            ),
            None,
            None,
        );
        let credentials = read_service_account_json(&fcm).await.unwrap();
        assert_eq!(
            credentials.client_email,
            "firebase@example.iam.gserviceaccount.com"
        );
        assert_eq!(credentials.private_key, "private-key");
        assert_eq!(
            credentials.token_uri.as_deref(),
            Some("https://oauth2.example/token")
        );
    }

    #[tokio::test]
    async fn run_push_cycle_records_transport_failure() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-transport",
            "ios",
            "ios-main",
        )
        .await;
        let _ = metrics::init_recorder().unwrap();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let apns = apns_config("http://127.0.0.1:9".into());

        run_push_cycle(
            &db,
            &http,
            &apns,
            &FcmConfig::new(false, None, None, None, None, None),
            &worker_config(),
        )
        .await
        .unwrap();

        let snapshot = metrics_snapshot().await;
        assert!(snapshot.contains("qm_push_transport_failures_total"));
    }

    #[tokio::test]
    async fn run_push_cycle_sends_apns_payload_and_records_provider_message_id() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        let batch_id = seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-apns",
            "ios",
            "ios-main",
        )
        .await;
        let server = FakeProviderServer::start(
            vec![FakeProviderResponse {
                status: StatusCode::OK,
                body: json!({}),
                provider_message_id: Some("test-apns-id".into()),
            }],
            vec![],
        )
        .await;

        run_push_cycle(
            &db,
            &reqwest::Client::new(),
            &apns_config(server.base_url.clone()),
            &FcmConfig::new(false, None, None, None, None, None),
            &worker_config(),
        )
        .await
        .unwrap();

        let captures = server.captures().await;
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].channel, reminders::CHANNEL_APNS);
        assert_eq!(captures[0].path, "/3/device/token-apns");
        assert_eq!(captures[0].authorization.as_deref(), Some("Bearer token"));
        assert_eq!(
            captures[0].body["aps"]["alert"]["title-loc-key"],
            "EXPIRY_REMINDER_TITLE"
        );
        assert_eq!(
            captures[0].body["aps"]["alert"]["title-loc-args"],
            json!(["Milk", "Pantry"])
        );
        assert_eq!(
            captures[0].body["aps"]["alert"]["loc-key"],
            "EXPIRY_REMINDER_BODY"
        );
        assert_eq!(
            captures[0].body["aps"]["alert"]["loc-args"],
            json!(["1", "ml", "2999-01-03"])
        );
        assert_eq!(captures[0].body["batch_id"], batch_id.to_string());

        let row = sqlx::query(
            "SELECT status, provider_message_id FROM reminder_delivery ORDER BY attempted_at DESC LIMIT 1",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            row.try_get::<String, _>("status").unwrap(),
            reminders::DELIVERY_STATUS_SUCCEEDED
        );
        assert_eq!(
            row.try_get::<String, _>("provider_message_id").unwrap(),
            "test-apns-id"
        );
    }

    #[tokio::test]
    async fn run_push_cycle_sends_fcm_notification_and_data_payload() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        let batch_id = seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-fcm",
            "android",
            "android-main",
        )
        .await;
        let server = FakeProviderServer::start(
            vec![],
            vec![FakeProviderResponse {
                status: StatusCode::OK,
                body: json!({"name": "projects/quartermaster-test/messages/fcm-123"}),
                provider_message_id: None,
            }],
        )
        .await;

        run_push_cycle(
            &db,
            &reqwest::Client::new(),
            &disabled_apns_config(),
            &fcm_config(server.base_url.clone()).await,
            &worker_config(),
        )
        .await
        .unwrap();

        let captures = server.captures().await;
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].channel, reminders::CHANNEL_FCM);
        assert_eq!(
            captures[0].path,
            "/v1/projects/quartermaster-test/messages:send"
        );
        assert_eq!(
            captures[0].authorization.as_deref(),
            Some("Bearer ya29.cached")
        );
        assert!(captures[0].body["message"].get("notification").is_none());
        assert_eq!(
            captures[0].body["message"]["android"]["notification"]["channel_id"],
            "expiry_reminders"
        );
        assert_eq!(
            captures[0].body["message"]["data"]["batch_id"],
            batch_id.to_string()
        );
        assert_eq!(captures[0].body["message"]["data"]["product_name"], "Milk");
        assert_eq!(
            captures[0].body["message"]["data"]["location_name"],
            "Pantry"
        );
        assert_eq!(captures[0].body["message"]["data"]["quantity"], "1");
        assert_eq!(captures[0].body["message"]["data"]["unit"], "ml");
        assert_eq!(
            captures[0].body["message"]["data"]["expires_on"],
            "2999-01-03"
        );

        let row = sqlx::query(
            "SELECT status, provider_message_id FROM reminder_delivery ORDER BY attempted_at DESC LIMIT 1",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            row.try_get::<String, _>("status").unwrap(),
            reminders::DELIVERY_STATUS_SUCCEEDED
        );
        assert_eq!(
            row.try_get::<String, _>("provider_message_id").unwrap(),
            "projects/quartermaster-test/messages/fcm-123"
        );
    }

    #[tokio::test]
    async fn run_push_cycle_records_mixed_fake_provider_outcomes() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-ios",
            "ios",
            "ios-main",
        )
        .await;
        seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-android",
            "android",
            "android-main",
        )
        .await;
        let server = FakeProviderServer::start(
            vec![FakeProviderResponse {
                status: StatusCode::OK,
                body: json!({}),
                provider_message_id: Some("mixed-apns-id".into()),
            }],
            vec![FakeProviderResponse {
                status: StatusCode::BAD_REQUEST,
                body: json!({
                    "error": {
                        "code": 400,
                        "status": "INVALID_ARGUMENT",
                        "message": "bad token",
                        "details": [
                            {
                                "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError",
                                "errorCode": "INVALID_ARGUMENT"
                            }
                        ]
                    }
                }),
                provider_message_id: None,
            }],
        )
        .await;

        run_push_cycle(
            &db,
            &reqwest::Client::new(),
            &apns_config(server.base_url.clone()),
            &fcm_config(server.base_url.clone()).await,
            &worker_config(),
        )
        .await
        .unwrap();

        let summary = reminders::push_delivery_metrics_summary(
            &db,
            &time::format_timestamp(Timestamp::now()),
        )
        .await
        .unwrap();
        assert_eq!(summary.failed_permanent_count, 1);
        assert_eq!(summary.invalid_token_count, 1);
        let captures = server.captures().await;
        assert_eq!(captures.len(), 4);
        assert!(captures
            .iter()
            .any(|capture| capture.channel == reminders::CHANNEL_APNS));
        assert!(captures
            .iter()
            .any(|capture| capture.channel == reminders::CHANNEL_FCM));
    }

    #[tokio::test]
    async fn run_push_cycle_marks_retryable_provider_failures_with_next_retry() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-retry",
            "android",
            "android-main",
        )
        .await;
        let server = FakeProviderServer::start(
            vec![],
            vec![FakeProviderResponse {
                status: StatusCode::SERVICE_UNAVAILABLE,
                body: json!({
                    "error": {
                        "code": 503,
                        "status": "UNAVAILABLE",
                        "message": "retry later"
                    }
                }),
                provider_message_id: None,
            }],
        )
        .await;

        run_push_cycle(
            &db,
            &reqwest::Client::new(),
            &disabled_apns_config(),
            &fcm_config(server.base_url.clone()).await,
            &worker_config(),
        )
        .await
        .unwrap();

        let row = sqlx::query(
            "SELECT last_push_status, next_retry_at \
             FROM reminder_device_state ORDER BY updated_at DESC LIMIT 1",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        assert_eq!(
            row.try_get::<String, _>("last_push_status").unwrap(),
            reminders::DELIVERY_STATUS_FAILED_RETRYABLE
        );
        assert!(row
            .try_get::<String, _>("next_retry_at")
            .unwrap()
            .starts_with("20"));
    }

    #[tokio::test]
    async fn mixed_platform_claims_choose_expected_channels() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-ios",
            "ios",
            "ios-main",
        )
        .await;
        seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-android",
            "android",
            "android-main",
        )
        .await;

        let claimed = reminders::claim_due_push_work(
            &db,
            "2000-01-01T00:00:00.000Z",
            10,
            "2000-01-01T00:01:00.000Z",
        )
        .await
        .unwrap();
        assert_eq!(claimed.items.len(), 4);
        assert!(claimed
            .items
            .iter()
            .any(|item| item.channel == reminders::CHANNEL_APNS));
        assert!(claimed
            .items
            .iter()
            .any(|item| item.channel == reminders::CHANNEL_FCM));
    }

    #[tokio::test]
    async fn api_registered_android_device_is_claimed_with_fcm_channel() {
        let db = qm_db::test_support::sqlite().await.into_db();
        let config = Arc::new(ApiConfig {
            expiry_reminder_policy: qm_db::reminders::ExpiryReminderPolicy {
                enabled: true,
                ..Default::default()
            },
            ..ApiConfig::default()
        });
        let state = AppState {
            db: db.clone(),
            config: config.clone(),
            http: reqwest::Client::new(),
            off_breaker: Arc::new(qm_api::openfoodfacts::OffCircuitBreaker::default()),
            rate_limiters: Arc::new(qm_api::rate_limit::RateLimiters::new(&config)),
        };
        let app = qm_api::router(state);

        let household = households::create(&db, "Home", "UTC").await.unwrap();
        locations::seed_defaults(&db, household.id).await.unwrap();
        let pantry = locations::list_for_household(&db, household.id)
            .await
            .unwrap()
            .into_iter()
            .find(|row| row.kind == "pantry")
            .unwrap()
            .id;
        let hash = qm_api::auth::hash_password("password123").unwrap();
        let user = users::create(&db, "alice", Some("alice@example.com"), &hash)
            .await
            .unwrap();
        memberships::insert(&db, household.id, user.id, "admin")
            .await
            .unwrap();
        let product = products::create_manual(
            &db,
            household.id,
            "Milk",
            None,
            "volume",
            Some("ml"),
            None,
            None,
        )
        .await
        .unwrap();
        let batch = stock::create(
            &db,
            household.id,
            product.id,
            pantry,
            "1",
            "ml",
            Some("2999-01-03"),
            None,
            None,
            user.id,
            Some(&config.expiry_reminder_policy),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE stock_reminder SET fire_at = ? WHERE batch_id = ?")
            .bind("2000-01-01T00:00:00.000Z")
            .bind(batch.id.to_string())
            .execute(&db.pool)
            .await
            .unwrap();

        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "username": "alice",
                            "password": "password123",
                            "device_label": "Android",
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), AxumStatusCode::OK);
        let body = axum::body::to_bytes(login.into_body(), usize::MAX)
            .await
            .unwrap();
        let token_body: Value = serde_json::from_slice(&body).unwrap();
        let access_token = token_body["access_token"].as_str().unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/devices/register")
                    .header("authorization", format!("Bearer {access_token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "device_id": "android-main",
                            "platform": "android",
                            "push_authorization": "authorized",
                            "push_token": "token-android",
                            "app_version": "0.1",
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), AxumStatusCode::NO_CONTENT);

        let claimed = reminders::claim_due_push_work(
            &db,
            "2000-01-01T00:00:00.000Z",
            10,
            "2000-01-01T00:01:00.000Z",
        )
        .await
        .unwrap();
        assert_eq!(claimed.items.len(), 1);
        assert_eq!(claimed.items[0].channel, reminders::CHANNEL_FCM);
    }

    #[tokio::test]
    async fn mixed_delivery_outcomes_update_metrics_without_socket_io() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        let _batch_id = seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-success",
            "ios",
            "ios-success",
        )
        .await;
        let android_user = users::create(&db, "bob", Some("bob@example.com"), "hash")
            .await
            .unwrap();
        memberships::insert(&db, household_id, android_user.id, "member")
            .await
            .unwrap();
        let android_session = Uuid::now_v7();
        auth_sessions::upsert(&db, android_session, android_user.id, Some(household_id))
            .await
            .unwrap();
        devices::upsert(
            &db,
            &DeviceUpsert {
                user_id: android_user.id,
                session_id: android_session,
                device_id: "android-permanent".into(),
                platform: "android".into(),
                push_token: Some("token-permanent".into()),
                push_authorization: "authorized".into(),
                app_version: Some("0.1".into()),
            },
        )
        .await
        .unwrap();

        let now = "2000-01-01T00:00:00.000Z";
        let _ = metrics::refresh_delivery_gauges(&db, now).await.unwrap();
        let claimed = reminders::claim_due_push_work(&db, now, 10, "2000-01-01T00:01:00.000Z")
            .await
            .unwrap();
        assert_eq!(claimed.items.len(), 2);

        for item in &claimed.items {
            let outcome = match item.channel.as_str() {
                reminders::CHANNEL_APNS => classify_apns_response(
                    StatusCode::OK,
                    Some("apns-success".into()),
                    String::new(),
                    "2000-01-01T00:05:00.000Z",
                    item,
                ),
                reminders::CHANNEL_FCM => classify_fcm_response(
                    StatusCode::BAD_REQUEST,
                    json!({
                        "error": {
                            "code": 400,
                            "status": "INVALID_ARGUMENT",
                            "details": [
                                {
                                    "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError",
                                    "errorCode": "INVALID_ARGUMENT"
                                }
                            ]
                        }
                    })
                    .to_string(),
                    "2000-01-01T00:05:00.000Z",
                ),
                other => panic!("unexpected channel {other}"),
            };
            reminders::complete_push_attempt(
                &db,
                item,
                &reminders::PushDeliveryResult {
                    channel: outcome.channel,
                    status: outcome.status,
                    finished_at: "2000-01-01T00:00:10.000Z".into(),
                    next_retry_at: outcome.next_retry_at,
                    provider_message_id: outcome.provider_message_id,
                    error_code: outcome.error_code,
                    error_message: outcome.error_message,
                },
            )
            .await
            .unwrap();
        }
        let _ = metrics::refresh_delivery_gauges(&db, now).await.unwrap();

        let summary = reminders::push_delivery_metrics_summary(
            &db,
            &time::format_timestamp(Timestamp::now()),
        )
        .await
        .unwrap();
        assert_eq!(summary.failed_permanent_count, 1);
        assert_eq!(summary.invalid_token_count, 1);

        let rows = sqlx::query(
            "SELECT d.push_token AS push_token, s.last_push_channel, s.last_error_code \
             FROM reminder_device_state s \
             INNER JOIN notification_device d ON d.id = s.device_id \
             ORDER BY d.push_token ASC",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        let permanent = rows.iter().find(|row| {
            row.try_get::<String, _>("push_token").unwrap().as_str() == "token-permanent"
        });
        assert_eq!(
            permanent
                .unwrap()
                .try_get::<String, _>("last_push_channel")
                .unwrap(),
            reminders::CHANNEL_FCM
        );
        assert_eq!(
            permanent
                .unwrap()
                .try_get::<String, _>("last_error_code")
                .unwrap(),
            "invalid_token"
        );
    }
}
