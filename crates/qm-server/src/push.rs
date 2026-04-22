use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use jiff::{Timestamp, ToSpan};
use metrics_exporter_prometheus::PrometheusHandle;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::metrics;
use qm_db::{reminders, time, Database};

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
    pub base_url: Option<String>,
}

impl ApnsConfig {
    pub fn is_ready(&self) -> bool {
        self.enabled && self.topic.is_some()
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
pub struct PushWorkerConfig {
    pub poll_interval: Duration,
    pub batch_size: i64,
    pub claim_ttl: Duration,
    pub retry_backoff: Duration,
}

#[derive(Debug)]
struct PushSendOutcome {
    status: &'static str,
    metric_outcome: &'static str,
    provider_message_id: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    next_retry_at: Option<String>,
    transport_error: bool,
}

#[derive(Debug, Deserialize)]
struct ApnsErrorBody {
    reason: Option<String>,
}

pub async fn run_push_worker(
    db: Database,
    http: reqwest::Client,
    apns: ApnsConfig,
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
                if let Err(err) = run_push_cycle(&db, &http, &apns, &worker).await {
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
    worker: &PushWorkerConfig,
) -> anyhow::Result<()> {
    if !apns.is_ready() {
        debug!("push worker skipped: APNs is not configured");
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

    let mut successful_attempts = 0_u64;
    let mut retryable_attempts = 0_u64;
    let mut permanent_attempts = 0_u64;
    let mut transport_failures = 0_u64;

    for item in claimed.items {
        let send_started = Instant::now();
        let outcome = match send_apns(http, apns, &item, &retry_at).await {
            Ok(outcome) => outcome,
            Err(err) => {
                warn!(?err, reminder_id = %item.reminder_id, device_id = %item.device_row_id, "push send failed before response");
                PushSendOutcome {
                    status: reminders::DELIVERY_STATUS_FAILED_RETRYABLE,
                    metric_outcome: "failed_retryable",
                    provider_message_id: None,
                    error_code: Some("transport_error".into()),
                    error_message: Some(err.to_string()),
                    next_retry_at: Some(retry_at.clone()),
                    transport_error: true,
                }
            }
        };
        metrics::record_send_duration(send_started.elapsed().as_secs_f64());
        metrics::record_attempt(outcome.metric_outcome);
        if outcome.transport_error {
            metrics::record_transport_failure();
            transport_failures += 1;
        }
        match outcome.status {
            reminders::DELIVERY_STATUS_SUCCEEDED => successful_attempts += 1,
            reminders::DELIVERY_STATUS_FAILED_RETRYABLE => retryable_attempts += 1,
            reminders::DELIVERY_STATUS_FAILED_PERMANENT => permanent_attempts += 1,
            _ => {}
        }

        reminders::complete_push_attempt(
            db,
            &item,
            &reminders::PushDeliveryResult {
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
    info!(
        due_before = before_summary.due_count,
        due_after = after_summary.due_count,
        retry_due_after = after_summary.retry_due_count,
        active_claims_after = after_summary.active_claim_count,
        failed_retryable_after = after_summary.failed_retryable_count,
        failed_permanent_after = after_summary.failed_permanent_count,
        invalid_tokens_after = after_summary.invalid_token_count,
        expired_claims = expired,
        claimed = successful_attempts + retryable_attempts + permanent_attempts,
        claim_conflicts = claimed.claim_conflicts,
        successful_attempts,
        retryable_attempts,
        permanent_attempts,
        transport_failures,
        "push worker cycle completed"
    );

    Ok(())
}

async fn send_apns(
    http: &reqwest::Client,
    apns: &ApnsConfig,
    item: &reminders::PushWorkItem,
    retry_at: &str,
) -> anyhow::Result<PushSendOutcome> {
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
    }

    let payload = json!({
        "aps": {
            "alert": {
                "title": item.title,
                "body": item.body,
            },
            "sound": "default",
        },
        "reminder_id": item.reminder_id,
        "batch_id": item.batch_id,
        "product_id": item.product_id,
        "location_id": item.location_id,
        "kind": item.kind,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::StatusCode;
    use serde_json::Value;
    use sqlx::Row;
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
                device_id: "ios-main".into(),
                platform: "ios".into(),
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

    fn apns_config(base_url: String) -> ApnsConfig {
        ApnsConfig {
            enabled: true,
            environment: ApnsEnvironment::Sandbox,
            topic: Some("com.example.quartermaster".into()),
            auth_token: Some("token".into()),
            base_url: Some(base_url),
        }
    }

    async fn metrics_snapshot() -> String {
        metrics::init_recorder().unwrap().render()
    }

    #[tokio::test]
    async fn classify_apns_response_marks_success() {
        let (_, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        let dummy = reminders::PushWorkItem {
            attempt_id: Uuid::now_v7(),
            reminder_id: Uuid::now_v7(),
            household_id,
            batch_id: pantry,
            product_id,
            location_id: pantry,
            kind: reminders::KIND_EXPIRY.into(),
            title: "Milk expires tomorrow".into(),
            body: "Pantry".into(),
            device_row_id: user_id,
            device_token: "token-success".into(),
        };
        let outcome = classify_apns_response(
            StatusCode::OK,
            Some("test-apns-id".into()),
            String::new(),
            "2000-01-01T00:05:00.000Z",
            &dummy,
        );
        assert_eq!(outcome.status, reminders::DELIVERY_STATUS_SUCCEEDED);
        assert_eq!(outcome.metric_outcome, "succeeded");
        assert_eq!(outcome.provider_message_id.as_deref(), Some("test-apns-id"));
    }

    #[tokio::test]
    async fn classify_apns_response_marks_retryable_failure() {
        let (_, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        let dummy = reminders::PushWorkItem {
            attempt_id: Uuid::now_v7(),
            reminder_id: Uuid::now_v7(),
            household_id,
            batch_id: pantry,
            product_id,
            location_id: pantry,
            kind: reminders::KIND_EXPIRY.into(),
            title: "Milk expires tomorrow".into(),
            body: "Pantry".into(),
            device_row_id: user_id,
            device_token: "token-retryable".into(),
        };
        let outcome = classify_apns_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            json!({ "reason": "InternalServerError" }).to_string(),
            "2000-01-01T00:05:00.000Z",
            &dummy,
        );
        assert_eq!(outcome.status, reminders::DELIVERY_STATUS_FAILED_RETRYABLE);
        assert_eq!(outcome.metric_outcome, "failed_retryable");
        assert_eq!(outcome.error_code.as_deref(), Some("InternalServerError"));
        assert_eq!(
            outcome.next_retry_at.as_deref(),
            Some("2000-01-01T00:05:00.000Z")
        );
    }

    #[tokio::test]
    async fn classify_apns_response_marks_permanent_failure() {
        let (_, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        let dummy = reminders::PushWorkItem {
            attempt_id: Uuid::now_v7(),
            reminder_id: Uuid::now_v7(),
            household_id,
            batch_id: pantry,
            product_id,
            location_id: pantry,
            kind: reminders::KIND_EXPIRY.into(),
            title: "Milk expires tomorrow".into(),
            body: "Pantry".into(),
            device_row_id: user_id,
            device_token: "token-permanent".into(),
        };
        let outcome = classify_apns_response(
            StatusCode::GONE,
            None,
            json!({ "reason": "Unregistered" }).to_string(),
            "2000-01-01T00:05:00.000Z",
            &dummy,
        );
        assert_eq!(outcome.status, reminders::DELIVERY_STATUS_FAILED_PERMANENT);
        assert_eq!(outcome.metric_outcome, "failed_permanent");
        assert_eq!(outcome.error_code.as_deref(), Some("Unregistered"));
        assert!(outcome.next_retry_at.is_none());
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
        )
        .await;
        let _ = metrics::init_recorder().unwrap();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let apns = apns_config("http://127.0.0.1:9".into());

        run_push_cycle(&db, &http, &apns, &worker_config())
            .await
            .unwrap();

        let snapshot = metrics_snapshot().await;
        assert!(snapshot.contains("qm_push_transport_failures_total"));
    }

    #[tokio::test]
    async fn mixed_delivery_outcomes_update_metrics_without_socket_io() {
        let (db, household_id, user_id, pantry, product_id) = setup_push_fixture().await;
        seed_due_reminder(
            &db,
            household_id,
            user_id,
            pantry,
            product_id,
            "token-success",
        )
        .await;
        let retry_session = Uuid::now_v7();
        auth_sessions::upsert(&db, retry_session, user_id, Some(household_id))
            .await
            .unwrap();
        devices::upsert(
            &db,
            &DeviceUpsert {
                user_id,
                session_id: retry_session,
                device_id: "ios-retry".into(),
                platform: "ios".into(),
                push_token: Some("token-retryable".into()),
                push_authorization: "authorized".into(),
                app_version: Some("0.1".into()),
            },
        )
        .await
        .unwrap();
        let permanent_session = Uuid::now_v7();
        auth_sessions::upsert(&db, permanent_session, user_id, Some(household_id))
            .await
            .unwrap();
        devices::upsert(
            &db,
            &DeviceUpsert {
                user_id,
                session_id: permanent_session,
                device_id: "ios-permanent".into(),
                platform: "ios".into(),
                push_token: Some("token-permanent".into()),
                push_authorization: "authorized".into(),
                app_version: Some("0.1".into()),
            },
        )
        .await
        .unwrap();

        let now = "2000-01-01T00:00:00.000Z";
        let _ = metrics::refresh_delivery_gauges(&db, now).await.unwrap();
        let claimed =
            reminders::claim_due_push_work(&db, now, 10, "2000-01-01T00:01:00.000Z")
                .await
                .unwrap();
        assert_eq!(claimed.items.len(), 3);

        for item in &claimed.items {
            let outcome = match item.device_token.as_str() {
                "token-success" => classify_apns_response(
                    StatusCode::OK,
                    Some("apns-success".into()),
                    String::new(),
                    "2000-01-01T00:05:00.000Z",
                    item,
                ),
                "token-retryable" => classify_apns_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    None,
                    json!({ "reason": "InternalServerError" }).to_string(),
                    "2000-01-01T00:05:00.000Z",
                    item,
                ),
                "token-permanent" => classify_apns_response(
                    StatusCode::GONE,
                    None,
                    json!({ "reason": "Unregistered" }).to_string(),
                    "2000-01-01T00:05:00.000Z",
                    item,
                ),
                other => panic!("unexpected token {other}"),
            };
            reminders::complete_push_attempt(
                &db,
                item,
                &reminders::PushDeliveryResult {
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
        assert_eq!(summary.retry_due_count, 1);
        assert_eq!(summary.active_claim_count, 0);
        assert_eq!(summary.failed_permanent_count, 1);
        assert_eq!(summary.invalid_token_count, 1);

        let rows = sqlx::query(
            "SELECT d.push_token AS push_token, s.last_push_status, s.next_retry_at, s.last_error_code \
             FROM reminder_device_state s \
             INNER JOIN notification_device d ON d.id = s.device_id \
             ORDER BY d.push_token ASC",
        )
        .fetch_all(&db.pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 3);

        let permanent = rows.iter().find(|row| {
            row.try_get::<String, _>("push_token")
                .unwrap()
                .as_str()
                == "token-permanent"
        });
        assert_eq!(
            permanent
                .unwrap()
                .try_get::<String, _>("last_push_status")
                .unwrap(),
            reminders::DELIVERY_STATUS_FAILED_PERMANENT
        );
        assert_eq!(
            permanent
                .unwrap()
                .try_get::<String, _>("last_error_code")
                .unwrap(),
            "Unregistered"
        );
    }

    #[tokio::test]
    async fn api_registered_device_is_claimed_and_expired_claim_is_retried() {
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

        let login_req = axum::http::Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                json!({"username":"alice","password":"password123"}).to_string(),
            ))
            .unwrap();
        let login_res = app.clone().oneshot(login_req).await.unwrap();
        let login_body = axum::body::to_bytes(login_res.into_body(), usize::MAX)
            .await
            .unwrap();
        let access_token = serde_json::from_slice::<Value>(&login_body).unwrap()["access_token"]
            .as_str()
            .unwrap()
            .to_owned();

        let register_req = axum::http::Request::builder()
            .method("POST")
            .uri("/devices/register")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {access_token}"))
            .body(axum::body::Body::from(
                json!({
                    "device_id":"ios-main",
                    "platform":"ios",
                    "push_authorization":"authorized",
                    "push_token":"token-api",
                    "app_version":"0.1"
                })
                .to_string(),
            ))
            .unwrap();
        let register_res = app.clone().oneshot(register_req).await.unwrap();
        assert_eq!(register_res.status(), StatusCode::NO_CONTENT);

        let first_claim = reminders::claim_due_push_work(
            &db,
            "2000-01-01T00:00:00.000Z",
            10,
            "2000-01-01T00:01:00.000Z",
        )
        .await
        .unwrap();
        assert_eq!(first_claim.items.len(), 1);

        let expired = reminders::expire_stale_push_claims(
            &db,
            "2000-01-01T00:02:00.000Z",
            "2000-01-01T00:05:00.000Z",
        )
        .await
        .unwrap();
        assert_eq!(expired, 1);

        let second_claim = reminders::claim_due_push_work(
            &db,
            "2000-01-01T00:05:00.000Z",
            10,
            "2000-01-01T00:06:00.000Z",
        )
        .await
        .unwrap();
        assert_eq!(second_claim.items.len(), 1);

        let state_row = sqlx::query(
            "SELECT last_push_status, next_retry_at FROM reminder_device_state ORDER BY updated_at DESC LIMIT 1",
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();
        let last_push_status: Option<String> = state_row.try_get("last_push_status").unwrap();
        assert_eq!(
            last_push_status.as_deref(),
            Some(reminders::DELIVERY_STATUS_SENDING)
        );
    }
}
