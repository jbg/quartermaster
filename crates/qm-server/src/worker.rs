use std::time::Duration;

use anyhow::Context;
use jiff::{Timestamp, ToSpan};
use metrics::counter;
use serde::Deserialize;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use qm_db::{auth_sessions, jobs, reminders, time, Database};

#[derive(Clone, Debug)]
pub struct JobWorkerConfig {
    pub poll_interval: Duration,
    pub batch_size: i64,
    pub lease_ttl: Duration,
    pub retry_backoff: Duration,
    pub worker_id: String,
}

#[derive(Debug, Deserialize)]
struct HouseholdJobPayload {
    household_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobOutcome {
    Done,
    ReenqueueAuthCleanup,
}

pub async fn run_job_worker(
    db: Database,
    policy: reminders::ExpiryReminderPolicy,
    config: JobWorkerConfig,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(config.poll_interval);
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Err(err) = run_job_cycle(&db, &policy, &config).await {
                    error!(?err, "background job worker cycle failed");
                }
            }
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    info!("background job worker shutting down");
                    break;
                }
            }
        }
    }
}

pub async fn run_job_cycle(
    db: &Database,
    policy: &reminders::ExpiryReminderPolicy,
    config: &JobWorkerConfig,
) -> anyhow::Result<()> {
    let now = Timestamp::now();
    let now_rfc3339 = time::format_timestamp(now);
    let retry_at = add_duration(now, config.retry_backoff).context("computing job retry time")?;
    let lease_until = add_duration(now, config.lease_ttl).context("computing job lease expiry")?;

    let expired = jobs::expire_leases(db, &now_rfc3339, &retry_at).await?;
    if expired > 0 {
        counter!("qm_background_job_leases_expired_total").increment(expired);
    }

    let claimed = jobs::claim_due(
        db,
        &now_rfc3339,
        config.batch_size,
        &config.worker_id,
        &lease_until,
    )
    .await?;
    counter!("qm_background_jobs_claimed_total").increment(claimed.len() as u64);

    for job in claimed {
        let result = run_job(db, policy, &job).await;
        match result {
            Ok(outcome) => {
                let finished_at = qm_db::now_utc_rfc3339();
                if jobs::complete(db, job.id, &config.worker_id, &finished_at).await? {
                    counter!("qm_background_jobs_completed_total", "kind" => job.kind.clone())
                        .increment(1);
                    if outcome == JobOutcome::ReenqueueAuthCleanup {
                        let _ = enqueue_auth_session_cleanup(db).await?;
                    }
                }
            }
            Err(err) => {
                let updated_at = qm_db::now_utc_rfc3339();
                let error = err.to_string();
                if jobs::retry(
                    db,
                    job.id,
                    &config.worker_id,
                    &retry_at,
                    &error,
                    &updated_at,
                )
                .await?
                {
                    counter!("qm_background_jobs_retried_total", "kind" => job.kind.clone())
                        .increment(1);
                }
                warn!(job_id = %job.id, kind = %job.kind, ?err, "background job failed");
            }
        }
    }

    Ok(())
}

async fn run_job(
    db: &Database,
    policy: &reminders::ExpiryReminderPolicy,
    job: &jobs::JobRow,
) -> anyhow::Result<JobOutcome> {
    match job.kind.as_str() {
        jobs::KIND_AUTH_SESSION_CLEANUP => run_auth_session_cleanup(db).await,
        jobs::KIND_EXPIRY_REMINDER_RECONCILE => {
            run_expiry_reminder_reconcile(db, policy, job).await
        }
        jobs::KIND_BILLING_SYNC => {
            debug!(job_id = %job.id, "reserved billing sync job kind has no handler yet");
            Ok(JobOutcome::Done)
        }
        other => anyhow::bail!("unknown background job kind: {other}"),
    }
}

async fn run_auth_session_cleanup(db: &Database) -> anyhow::Result<JobOutcome> {
    let deleted = auth_sessions::delete_stale_session_batch(
        db,
        &qm_db::now_utc_rfc3339(),
        auth_sessions::STALE_SESSION_SWEEP_BATCH_SIZE,
    )
    .await?;
    counter!("qm_auth_session_swept_sessions_total", "surface" => "worker").increment(deleted);
    if deleted >= u64::from(auth_sessions::STALE_SESSION_SWEEP_BATCH_SIZE) {
        Ok(JobOutcome::ReenqueueAuthCleanup)
    } else {
        Ok(JobOutcome::Done)
    }
}

async fn run_expiry_reminder_reconcile(
    db: &Database,
    policy: &reminders::ExpiryReminderPolicy,
    job: &jobs::JobRow,
) -> anyhow::Result<JobOutcome> {
    let payload: HouseholdJobPayload = serde_json::from_str(&job.payload_json)
        .context("parsing reminder reconcile job payload")?;
    let stats = reminders::reconcile_household(db, payload.household_id, policy).await?;
    counter!("qm_expiry_reminder_sweep_inserted_total", "surface" => "worker")
        .increment(stats.inserted);
    counter!("qm_expiry_reminder_sweep_deleted_total", "surface" => "worker")
        .increment(stats.deleted);
    Ok(JobOutcome::Done)
}

pub async fn enqueue_auth_session_cleanup(db: &Database) -> anyhow::Result<bool> {
    Ok(jobs::enqueue_auth_session_cleanup(db).await?)
}

pub async fn enqueue_expiry_reconcile_all(db: &Database) -> anyhow::Result<u64> {
    Ok(jobs::enqueue_expiry_reconcile_all(db).await?)
}

fn add_duration(now: Timestamp, duration: Duration) -> anyhow::Result<String> {
    let seconds = i64::try_from(duration.as_secs()).unwrap_or(i64::MAX);
    let timestamp = now.checked_add(seconds.seconds())?;
    Ok(time::format_timestamp(timestamp))
}
