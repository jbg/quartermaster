use std::str::FromStr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/storage-vessels",
            get(list_storage_vessels).post(create_storage_vessel),
        )
        .route(
            "/storage-vessels/{id}",
            patch(update_storage_vessel).delete(delete_storage_vessel),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StorageVesselDto {
    pub id: Uuid,
    pub name: String,
    pub tare_weight: String,
    pub tare_unit: String,
    pub sort_order: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateStorageVesselRequest {
    pub name: String,
    pub tare_weight: String,
    pub tare_unit: String,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateStorageVesselRequest {
    pub name: String,
    pub tare_weight: String,
    pub tare_unit: String,
    pub sort_order: i64,
}

#[utoipa::path(
    get,
    path = "/storage-vessels",
    operation_id = "storage_vessels_list",
    tag = "storage-vessels",
    responses(
        (status = 200, body = [StorageVesselDto]),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list_storage_vessels(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<Vec<StorageVesselDto>>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let rows = qm_db::storage_vessels::list_for_household(&state.db, household_id).await?;
    Ok(Json(rows.into_iter().map(to_dto).collect()))
}

#[utoipa::path(
    post,
    path = "/storage-vessels",
    operation_id = "storage_vessels_create",
    tag = "storage-vessels",
    request_body = CreateStorageVesselRequest,
    responses((status = 201, body = StorageVesselDto)),
    security(("bearer" = [])),
)]
pub async fn create_storage_vessel(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateStorageVesselRequest>,
) -> ApiResult<(StatusCode, Json<StorageVesselDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let name = validate_name(&req.name)?;
    validate_tare(&req.tare_weight, &req.tare_unit)?;
    let sort_order = match req.sort_order {
        Some(v) => v,
        None => qm_db::storage_vessels::next_sort_order(&state.db, household_id).await?,
    };
    let row = qm_db::storage_vessels::create(
        &state.db,
        household_id,
        name,
        &req.tare_weight,
        &req.tare_unit,
        sort_order,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(to_dto(row))))
}

#[utoipa::path(
    patch,
    path = "/storage-vessels/{id}",
    operation_id = "storage_vessels_update",
    tag = "storage-vessels",
    request_body = UpdateStorageVesselRequest,
    params(("id" = Uuid, Path)),
    responses((status = 200, body = StorageVesselDto)),
    security(("bearer" = [])),
)]
pub async fn update_storage_vessel(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStorageVesselRequest>,
) -> ApiResult<Json<StorageVesselDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let name = validate_name(&req.name)?;
    validate_tare(&req.tare_weight, &req.tare_unit)?;
    let row = qm_db::storage_vessels::update(
        &state.db,
        household_id,
        id,
        name,
        &req.tare_weight,
        &req.tare_unit,
        req.sort_order,
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(to_dto(row)))
}

#[utoipa::path(
    delete,
    path = "/storage-vessels/{id}",
    operation_id = "storage_vessels_delete",
    tag = "storage-vessels",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn delete_storage_vessel(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let deleted = qm_db::storage_vessels::delete(&state.db, household_id, id).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn to_dto(row: qm_db::storage_vessels::StorageVesselRow) -> StorageVesselDto {
    StorageVesselDto {
        id: row.id,
        name: row.name,
        tare_weight: row.tare_weight,
        tare_unit: row.tare_unit,
        sort_order: row.sort_order,
    }
}

fn validate_name(name: &str) -> ApiResult<&str> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return Err(ApiError::BadRequest(
            "storage vessel name must be 1..=80 chars".into(),
        ));
    }
    Ok(trimmed)
}

fn validate_tare(weight: &str, unit: &str) -> ApiResult<()> {
    let parsed = Decimal::from_str(weight)
        .map_err(|_| ApiError::BadRequest("tare_weight not a valid decimal".into()))?;
    if parsed < Decimal::ZERO {
        return Err(ApiError::BadRequest(
            "tare_weight must be zero or greater".into(),
        ));
    }
    let unit = qm_core::units::lookup(unit)
        .map_err(|_| ApiError::BadRequest("tare_unit is not a known unit".into()))?;
    if unit.family.as_str() != "mass" {
        return Err(ApiError::BadRequest("tare_unit must be a mass unit".into()));
    }
    Ok(())
}
