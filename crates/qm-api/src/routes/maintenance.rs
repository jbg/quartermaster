use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use metrics::counter;
use serde::Serialize;

use crate::{ApiError, ApiResult, AppState};

pub const MAINTENANCE_TOKEN_HEADER: &str = "x-qm-maintenance-token";

#[derive(Debug, Serialize)]
pub struct SweepAuthSessionsResponse {
    pub deleted_sessions: u64,
}

#[derive(Debug, Serialize)]
pub struct SweepExpiryRemindersResponse {
    pub inserted: u64,
    pub deleted: u64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/maintenance/sweep-auth-sessions",
            post(sweep_auth_sessions),
        )
        .route(
            "/internal/maintenance/sweep-expiry-reminders",
            post(sweep_expiry_reminders),
        )
}

async fn sweep_auth_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<SweepAuthSessionsResponse>)> {
    let provided = headers
        .get(MAINTENANCE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    let expected = state
        .config
        .auth_session_sweep_trigger_secret
        .as_deref()
        .ok_or(ApiError::NotFound)?;

    if provided != Some(expected) {
        return Err(ApiError::Unauthorized);
    }

    let deleted_sessions = match qm_db::auth_sessions::delete_stale_sessions(
        &state.db,
        &qm_db::now_utc_rfc3339(),
        qm_db::auth_sessions::STALE_SESSION_SWEEP_BATCH_SIZE,
    )
    .await
    {
        Ok(deleted_sessions) => deleted_sessions,
        Err(err) => {
            counter!("qm_auth_session_sweeps_total", "surface" => "manual", "outcome" => "failure")
                .increment(1);
            return Err(err.into());
        }
    };
    counter!("qm_auth_session_sweeps_total", "surface" => "manual", "outcome" => "success")
        .increment(1);
    counter!("qm_auth_session_swept_sessions_total", "surface" => "manual")
        .increment(deleted_sessions);

    Ok((
        StatusCode::OK,
        Json(SweepAuthSessionsResponse { deleted_sessions }),
    ))
}

async fn sweep_expiry_reminders(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<SweepExpiryRemindersResponse>)> {
    let provided = headers
        .get(MAINTENANCE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    let expected = state
        .config
        .expiry_reminder_trigger_secret
        .as_deref()
        .ok_or(ApiError::NotFound)?;

    if provided != Some(expected) {
        return Err(ApiError::Unauthorized);
    }

    let stats = match qm_db::reminders::reconcile_all(&state.db, &state.config.expiry_reminder_policy).await {
        Ok(stats) => stats,
        Err(err) => {
            counter!("qm_expiry_reminder_sweeps_total", "surface" => "manual", "outcome" => "failure")
                .increment(1);
            return Err(err.into());
        }
    };
    counter!("qm_expiry_reminder_sweeps_total", "surface" => "manual", "outcome" => "success")
        .increment(1);
    counter!("qm_expiry_reminder_sweep_inserted_total", "surface" => "manual")
        .increment(stats.inserted);
    counter!("qm_expiry_reminder_sweep_deleted_total", "surface" => "manual")
        .increment(stats.deleted);
    Ok((
        StatusCode::OK,
        Json(SweepExpiryRemindersResponse {
            inserted: stats.inserted,
            deleted: stats.deleted,
        }),
    ))
}
