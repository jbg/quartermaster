use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/locations", get(list_locations).post(create_location))
        .route(
            "/locations/{id}",
            patch(update_location).delete(delete_location),
        )
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LocationDto {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub sort_order: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLocationRequest {
    pub name: String,
    pub kind: String,
    pub sort_order: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateLocationRequest {
    pub name: String,
    pub kind: String,
    pub sort_order: i64,
}

#[utoipa::path(
    get,
    path = "/locations",
    operation_id = "locations_list",
    tag = "locations",
    responses(
        (status = 200, body = [LocationDto]),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list_locations(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<Vec<LocationDto>>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let rows = qm_db::locations::list_for_household(&state.db, household_id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|l| LocationDto {
                id: l.id,
                name: l.name,
                kind: l.kind,
                sort_order: l.sort_order,
            })
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/locations",
    operation_id = "locations_create",
    tag = "locations",
    request_body = CreateLocationRequest,
    responses((status = 201, body = LocationDto)),
    security(("bearer" = [])),
)]
pub async fn create_location(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateLocationRequest>,
) -> ApiResult<(StatusCode, Json<LocationDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let name = req.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(ApiError::BadRequest(
            "location name must be 1..=64 chars".into(),
        ));
    }
    validate_kind(&req.kind)?;
    let sort_order = match req.sort_order {
        Some(v) => v,
        None => qm_db::locations::next_sort_order(&state.db, household_id).await?,
    };
    let row =
        qm_db::locations::create(&state.db, household_id, name, &req.kind, sort_order).await?;
    Ok((StatusCode::CREATED, Json(to_dto(row))))
}

#[utoipa::path(
    patch,
    path = "/locations/{id}",
    operation_id = "locations_update",
    tag = "locations",
    request_body = UpdateLocationRequest,
    params(("id" = Uuid, Path)),
    responses((status = 200, body = LocationDto)),
    security(("bearer" = [])),
)]
pub async fn update_location(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateLocationRequest>,
) -> ApiResult<Json<LocationDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let name = req.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(ApiError::BadRequest(
            "location name must be 1..=64 chars".into(),
        ));
    }
    validate_kind(&req.kind)?;
    let row =
        qm_db::locations::update(&state.db, household_id, id, name, &req.kind, req.sort_order)
            .await?
            .ok_or(ApiError::NotFound)?;
    Ok(Json(to_dto(row)))
}

#[utoipa::path(
    delete,
    path = "/locations/{id}",
    operation_id = "locations_delete",
    tag = "locations",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 409, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn delete_location(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    if qm_db::locations::has_active_stock(&state.db, household_id, id).await? {
        return Err(ApiError::LocationHasStock);
    }
    let deleted = qm_db::locations::delete(&state.db, household_id, id).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn to_dto(l: qm_db::locations::LocationRow) -> LocationDto {
    LocationDto {
        id: l.id,
        name: l.name,
        kind: l.kind,
        sort_order: l.sort_order,
    }
}

fn validate_kind(kind: &str) -> ApiResult<()> {
    match kind {
        "pantry" | "fridge" | "freezer" => Ok(()),
        _ => Err(ApiError::BadRequest(
            "location kind must be pantry, fridge, or freezer".into(),
        )),
    }
}
