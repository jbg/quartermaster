use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch},
    Json, Router,
};
use qm_db::ai_tasks::AiTaskRow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    types::{AiProvider, AiTaskType, AiTaskUserState, AiTaskValidationStatus},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ai/status", get(status))
        .route("/ai/tasks", get(list_tasks))
        .route("/ai/tasks/{id}", get(get_task))
        .route("/ai/tasks/{id}/state", patch(update_task_state))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AiStatusResponse {
    pub provider: AiProvider,
    pub enabled: bool,
    pub configured: bool,
    pub model: Option<String>,
    pub structured_outputs: bool,
    pub raw_response_retention: bool,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AiTaskListQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AiTaskListResponse {
    pub items: Vec<AiTaskDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AiTaskDto {
    pub id: Uuid,
    pub task_type: AiTaskType,
    pub provider: AiProvider,
    pub model: Option<String>,
    pub prompt_version: String,
    pub input_digest: String,
    pub input_summary: Value,
    pub output_json: Option<Value>,
    pub validation_status: AiTaskValidationStatus,
    pub validation_errors: Vec<String>,
    pub user_state: AiTaskUserState,
    pub credentials_assertion: bool,
    pub raw_response_json: Option<Value>,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAiTaskStateRequest {
    pub user_state: AiTaskUserState,
}

#[utoipa::path(
    get,
    path = "/ai/status",
    operation_id = "ai_status",
    tag = "ai",
    responses((status = 200, body = AiStatusResponse)),
    security(("bearer" = [])),
)]
pub async fn status(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<AiStatusResponse>> {
    current.household_id.ok_or(ApiError::Forbidden)?;
    let status = state.ai_provider.status();
    Ok(Json(AiStatusResponse {
        provider: status.provider.into(),
        enabled: status.enabled,
        configured: status.configured,
        model: status.model,
        structured_outputs: status.structured_outputs,
        raw_response_retention: status.raw_response_retention,
    }))
}

#[utoipa::path(
    get,
    path = "/ai/tasks",
    operation_id = "ai_task_list",
    tag = "ai",
    params(AiTaskListQuery),
    responses((status = 200, body = AiTaskListResponse)),
    security(("bearer" = [])),
)]
pub async fn list_tasks(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<AiTaskListQuery>,
) -> ApiResult<Json<AiTaskListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let rows = qm_db::ai_tasks::list(&state.db, household_id, limit).await?;
    let items = rows
        .into_iter()
        .map(task_into_dto)
        .collect::<ApiResult<_>>()?;
    Ok(Json(AiTaskListResponse { items }))
}

#[utoipa::path(
    get,
    path = "/ai/tasks/{id}",
    operation_id = "ai_task_get",
    tag = "ai",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = AiTaskDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_task(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AiTaskDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::ai_tasks::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(task_into_dto(row)?))
}

#[utoipa::path(
    patch,
    path = "/ai/tasks/{id}/state",
    operation_id = "ai_task_state_update",
    tag = "ai",
    params(("id" = Uuid, Path)),
    request_body = UpdateAiTaskStateRequest,
    responses((status = 200, body = AiTaskDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn update_task_state(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAiTaskStateRequest>,
) -> ApiResult<Json<AiTaskDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let row =
        qm_db::ai_tasks::update_user_state(&state.db, household_id, id, req.user_state.as_str())
            .await?
            .ok_or(ApiError::NotFound)?;
    Ok(Json(task_into_dto(row)?))
}

fn task_into_dto(row: AiTaskRow) -> ApiResult<AiTaskDto> {
    Ok(AiTaskDto {
        id: row.id,
        task_type: AiTaskType::from_str(&row.task_type)?,
        provider: AiProvider::from_str(&row.provider)?,
        model: row.model,
        prompt_version: row.prompt_version,
        input_digest: row.input_digest,
        input_summary: parse_json(&row.input_summary_json, "input summary", row.id)?,
        output_json: row
            .output_json
            .as_deref()
            .map(|value| parse_json(value, "output", row.id))
            .transpose()?,
        validation_status: AiTaskValidationStatus::from_str(&row.validation_status)?,
        validation_errors: serde_json::from_str(&row.validation_errors_json).map_err(|err| {
            ApiError::Internal(anyhow::anyhow!(
                "invalid AI task validation errors JSON for {}: {err}",
                row.id
            ))
        })?,
        user_state: AiTaskUserState::from_str(&row.user_state)?,
        credentials_assertion: row.credentials_assertion,
        raw_response_json: row
            .raw_response_json
            .as_deref()
            .map(|value| parse_json(value, "raw response", row.id))
            .transpose()?,
        created_by: row.created_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn parse_json(value: &str, label: &str, id: Uuid) -> ApiResult<Value> {
    serde_json::from_str(value).map_err(|err| {
        ApiError::Internal(anyhow::anyhow!(
            "invalid AI task {label} JSON for {id}: {err}",
        ))
    })
}
