use std::time::Duration;

use anyhow::Context;
use jiff::{Timestamp, ToSpan};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

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
    provider_message_id: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    next_retry_at: Option<String>,
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
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(worker.poll_interval);
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = run_push_cycle(&db, &http, &apns, &worker).await {
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

    let now = Timestamp::now();
    let now_rfc3339 = time::format_timestamp(now);
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
    let work =
        reminders::claim_due_push_work(db, &now_rfc3339, worker.batch_size, &claim_until).await?;
    info!(
        expired_claims = expired,
        candidates = work.len(),
        "push worker claimed reminder deliveries"
    );

    for item in work {
        let outcome = match send_apns(http, apns, &item, &retry_at).await {
            Ok(outcome) => outcome,
            Err(err) => {
                warn!(?err, reminder_id = %item.reminder_id, device_id = %item.device_row_id, "push send failed before response");
                PushSendOutcome {
                    status: reminders::DELIVERY_STATUS_FAILED_RETRYABLE,
                    provider_message_id: None,
                    error_code: Some("transport_error".into()),
                    error_message: Some(err.to_string()),
                    next_retry_at: Some(retry_at.clone()),
                }
            }
        };

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
    if status.is_success() {
        info!(
            reminder_id = %item.reminder_id,
            device_id = %item.device_row_id,
            apns_id = apns_id.as_deref().unwrap_or(""),
            "push delivery succeeded"
        );
        return Ok(PushSendOutcome {
            status: reminders::DELIVERY_STATUS_SUCCEEDED,
            provider_message_id: apns_id,
            error_code: None,
            error_message: None,
            next_retry_at: None,
        });
    }

    let body = response.text().await.unwrap_or_default();
    let parsed = serde_json::from_str::<ApnsErrorBody>(&body).ok();
    let error_code = parsed
        .as_ref()
        .and_then(|value| value.reason.clone())
        .unwrap_or_else(|| format!("http_{}", status.as_u16()));
    let permanent = matches!(status.as_u16(), 400 | 403 | 404 | 410);
    let error_message = if body.is_empty() { None } else { Some(body) };
    Ok(PushSendOutcome {
        status: if permanent {
            reminders::DELIVERY_STATUS_FAILED_PERMANENT
        } else {
            reminders::DELIVERY_STATUS_FAILED_RETRYABLE
        },
        provider_message_id: apns_id,
        error_code: Some(error_code),
        error_message,
        next_retry_at: if permanent {
            None
        } else {
            Some(retry_at.to_owned())
        },
    })
}
