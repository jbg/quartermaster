use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    types::ReminderKind,
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
    pub title: String,
    pub body: String,
    pub fire_at: String,
    pub household_timezone: String,
    pub household_fire_local_at: String,
    pub expires_on: Option<String>,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub presented_on_device_at: Option<String>,
    pub opened_on_device_at: Option<String>,
    pub acked_at: Option<String>,
}

impl TryFrom<qm_db::reminders::ReminderRow> for ReminderDto {
    type Error = ApiError;

    fn try_from(value: qm_db::reminders::ReminderRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            kind: value.kind.parse()?,
            title: value.title,
            body: value.body,
            fire_at: value.fire_at,
            household_timezone: value.household_timezone,
            household_fire_local_at: value.household_fire_local_at,
            expires_on: value.expires_on,
            batch_id: value.batch_id,
            product_id: value.product_id,
            location_id: value.location_id,
            presented_on_device_at: value.presented_on_device_at,
            opened_on_device_at: value.opened_on_device_at,
            acked_at: value.acked_at,
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
    let page = qm_db::reminders::list_due(
        &state.db,
        household_id,
        current.session_id,
        &qm_db::now_utc_rfc3339(),
        q.after_fire_at.as_deref(),
        q.after_id,
        limit,
    )
    .await?;
    let items = page
        .items
        .into_iter()
        .map(ReminderDto::try_from)
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(ReminderListResponse {
        items,
        next_after_fire_at: page.next_after_fire_at,
        next_after_id: page.next_after_id,
    }))
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
