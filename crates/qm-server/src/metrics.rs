use std::{sync::Arc, sync::OnceLock};

use anyhow::Context;
use axum::{
    extract::Extension,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use qm_api::routes::{health::HealthResponse, maintenance::MAINTENANCE_TOKEN_HEADER};
use qm_db::{reminders::PushDeliveryMetricsSummary, time, Database};

static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

#[derive(Clone)]
struct MetricsRouteState {
    handle: PrometheusHandle,
    trigger_secret: Arc<String>,
}

pub fn init_recorder() -> anyhow::Result<PrometheusHandle> {
    if let Some(handle) = METRICS_HANDLE.get() {
        return Ok(handle.clone());
    }

    let handle = PrometheusBuilder::new()
        .install_recorder()
        .context("installing Prometheus recorder")?;
    let _ = METRICS_HANDLE.set(handle.clone());
    Ok(METRICS_HANDLE.get().cloned().unwrap_or(handle))
}

pub fn internal_router(
    handle: PrometheusHandle,
    trigger_secret: Arc<String>,
    include_health: bool,
) -> Router {
    let mut router = Router::new().route("/internal/metrics", get(render_metrics));
    if include_health {
        router = router.route("/healthz", get(healthz));
    }
    router.layer(Extension(MetricsRouteState {
        handle,
        trigger_secret,
    }))
}

pub async fn refresh_delivery_gauges(
    db: &Database,
    now_rfc3339: &str,
) -> Result<PushDeliveryMetricsSummary, sqlx::Error> {
    let summary = qm_db::reminders::push_delivery_metrics_summary(db, now_rfc3339).await?;
    let oldest_due_age_seconds = summary
        .oldest_due_at
        .as_deref()
        .map(|value| {
            let oldest = time::parse_timestamp(value)?;
            let now = time::parse_timestamp(now_rfc3339)?;
            Ok::<f64, sqlx::Error>(now.duration_since(oldest).as_secs_f64())
        })
        .transpose()?
        .unwrap_or(0.0);

    gauge!("qm_reminders_due_count").set(summary.due_count as f64);
    gauge!("qm_reminders_oldest_due_age_seconds").set(oldest_due_age_seconds);
    gauge!("qm_push_deliveries_retry_due_count").set(summary.retry_due_count as f64);
    gauge!("qm_push_deliveries_active_claim_count").set(summary.active_claim_count as f64);
    gauge!("qm_push_deliveries_failed_retryable_count").set(summary.failed_retryable_count as f64);
    gauge!("qm_push_deliveries_failed_permanent_count").set(summary.failed_permanent_count as f64);
    gauge!("qm_push_devices_with_invalid_token_count").set(summary.invalid_token_count as f64);
    Ok(summary)
}

pub fn record_cycle_started() {
    counter!("qm_push_worker_cycles_total").increment(1);
}

pub fn record_cycle_failed() {
    counter!("qm_push_worker_cycle_errors_total").increment(1);
}

pub fn record_cycle_duration(seconds: f64) {
    histogram!("qm_push_cycle_duration_seconds").record(seconds);
}

pub fn record_last_cycle_completed(timestamp_seconds: f64) {
    gauge!("qm_push_worker_last_cycle_completed_timestamp_seconds").set(timestamp_seconds);
}

pub fn record_claimed(count: u64) {
    counter!("qm_push_worker_claimed_total").increment(count);
}

pub fn record_claim_conflicts(count: u64) {
    counter!("qm_push_worker_claim_conflicts_total").increment(count);
}

pub fn record_expired_claims(count: u64) {
    counter!("qm_push_worker_expired_claims_total").increment(count);
}

pub fn record_attempt(outcome: &'static str) {
    counter!("qm_push_attempts_total", "channel" => "apns", "outcome" => outcome).increment(1);
}

pub fn record_transport_failure() {
    counter!("qm_push_transport_failures_total").increment(1);
}

pub fn record_send_duration(seconds: f64) {
    histogram!("qm_push_send_duration_seconds").record(seconds);
}

async fn healthz() -> axum::Json<HealthResponse> {
    axum::Json(HealthResponse { status: "ok" })
}

async fn render_metrics(
    headers: HeaderMap,
    Extension(state): Extension<MetricsRouteState>,
) -> impl IntoResponse {
    let provided = headers
        .get(MAINTENANCE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    if provided != Some(state.trigger_secret.as_str()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    (StatusCode::OK, response_headers, state.handle.render()).into_response()
}
