use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use jiff::{tz, Timestamp, Unit};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    types::{ReminderKind, ReminderUrgency},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reminders", get(list))
        .route("/reminders/{id}/present", post(present))
        .route("/reminders/{id}/open", post(open))
        .route("/reminders/{id}/ack", post(ack))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReminderDto {
    pub id: Uuid,
    pub kind: ReminderKind,
    pub fire_at: String,
    pub household_timezone: String,
    pub household_fire_local_at: String,
    pub expires_on: Option<String>,
    pub days_until_expiry: Option<i64>,
    #[schema(nullable = false)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urgency: Option<ReminderUrgency>,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub product_name: String,
    pub location_name: String,
    pub quantity: String,
    pub unit: String,
    pub presented_on_device_at: Option<String>,
    pub opened_on_device_at: Option<String>,
}

impl ReminderDto {
    fn try_from_row(
        value: qm_db::reminders::ReminderRow,
        now: Timestamp,
    ) -> Result<Self, ApiError> {
        let days_until_expiry =
            days_until_expiry(value.expires_on.as_deref(), &value.household_timezone, now);
        Ok(Self {
            id: value.id,
            kind: value.kind.parse()?,
            fire_at: value.fire_at,
            household_timezone: value.household_timezone,
            household_fire_local_at: value.household_fire_local_at,
            expires_on: value.expires_on,
            days_until_expiry,
            urgency: days_until_expiry.map(reminder_urgency),
            batch_id: value.batch_id,
            product_id: value.product_id,
            location_id: value.location_id,
            product_name: value.product_name,
            location_name: value.location_name,
            quantity: value.quantity,
            unit: value.unit,
            presented_on_device_at: value.presented_on_device_at,
            opened_on_device_at: value.opened_on_device_at,
        })
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ReminderListQuery {
    pub after_fire_at: Option<String>,
    pub after_id: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReminderListResponse {
    pub items: Vec<ReminderDto>,
    pub next_after_fire_at: Option<String>,
    pub next_after_id: Option<Uuid>,
}

const DEFAULT_REMINDER_LIMIT: i64 = 50;
const MAX_REMINDER_LIMIT: i64 = 200;

#[utoipa::path(
    get,
    path = "/reminders",
    operation_id = "reminders_list",
    tag = "reminders",
    params(ReminderListQuery),
    responses(
        (status = 200, body = ReminderListResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<ReminderListQuery>,
) -> ApiResult<Json<ReminderListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    if q.after_id.is_some() && q.after_fire_at.is_none() {
        return Err(ApiError::BadRequest(
            "after_id requires after_fire_at".into(),
        ));
    }

    let limit = q
        .limit
        .unwrap_or(DEFAULT_REMINDER_LIMIT)
        .clamp(1, MAX_REMINDER_LIMIT);
    let now = qm_db::time::now_timestamp();
    let now_rfc3339 = qm_db::time::format_timestamp(now);
    let page = qm_db::reminders::list_due(
        &state.db,
        household_id,
        current.session_id,
        &now_rfc3339,
        q.after_fire_at.as_deref(),
        q.after_id,
        limit,
    )
    .await?;
    let items = page
        .items
        .into_iter()
        .map(|row| ReminderDto::try_from_row(row, now))
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(ReminderListResponse {
        items,
        next_after_fire_at: page.next_after_fire_at,
        next_after_id: page.next_after_id,
    }))
}

fn days_until_expiry(
    expires_on: Option<&str>,
    household_timezone: &str,
    now: Timestamp,
) -> Option<i64> {
    let expires_on = expires_on?;
    let expiry = qm_db::time::parse_date(expires_on).ok()?;
    let time_zone = tz::db().get(household_timezone).ok()?;
    let today = now.to_zoned(time_zone).date();
    let span = today.until((Unit::Day, expiry)).ok()?;
    Some(i64::from(span.get_days()))
}

fn reminder_urgency(days: i64) -> ReminderUrgency {
    match days {
        i64::MIN..=-1 => ReminderUrgency::Expired,
        0 => ReminderUrgency::ExpiresToday,
        1 => ReminderUrgency::ExpiresTomorrow,
        _ => ReminderUrgency::ExpiresFuture,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_until_expiry_uses_household_timezone() {
        let now: Timestamp = "2026-04-24T00:30:00.000Z".parse().unwrap();

        assert_eq!(
            days_until_expiry(Some("2026-04-23"), "America/Los_Angeles", now),
            Some(0)
        );
        assert_eq!(reminder_urgency(0), ReminderUrgency::ExpiresToday);
        assert_eq!(
            days_until_expiry(Some("2026-04-24"), "America/Los_Angeles", now),
            Some(1)
        );
        assert_eq!(reminder_urgency(1), ReminderUrgency::ExpiresTomorrow);
        assert_eq!(
            days_until_expiry(Some("2026-04-20"), "Europe/Madrid", now),
            Some(-4)
        );
        assert_eq!(reminder_urgency(-4), ReminderUrgency::Expired);
        assert_eq!(
            days_until_expiry(Some("2026-05-01"), "Europe/Madrid", now),
            Some(7)
        );
        assert_eq!(reminder_urgency(7), ReminderUrgency::ExpiresFuture);
        assert_eq!(days_until_expiry(None, "Europe/Madrid", now), None);
        assert_eq!(
            days_until_expiry(Some("not-a-date"), "Europe/Madrid", now),
            None
        );
    }
}

#[utoipa::path(
    post,
    path = "/reminders/{id}/present",
    operation_id = "reminders_present",
    tag = "reminders",
    params(("id" = Uuid, Path)),
    responses(
        (status = 204),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn present(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let found = qm_db::reminders::mark_presented(
        &state.db,
        household_id,
        current.session_id,
        id,
        &qm_db::now_utc_rfc3339(),
    )
    .await?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    post,
    path = "/reminders/{id}/open",
    operation_id = "reminders_open",
    tag = "reminders",
    params(("id" = Uuid, Path)),
    responses(
        (status = 204),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn open(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let found = qm_db::reminders::mark_opened(
        &state.db,
        household_id,
        current.session_id,
        id,
        &qm_db::now_utc_rfc3339(),
    )
    .await?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    post,
    path = "/reminders/{id}/ack",
    operation_id = "reminders_ack",
    tag = "reminders",
    params(("id" = Uuid, Path)),
    responses(
        (status = 204),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn ack(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let found =
        qm_db::reminders::ack(&state.db, household_id, id, &qm_db::now_utc_rfc3339()).await?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
