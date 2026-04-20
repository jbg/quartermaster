use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/locations", get(list_locations))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LocationDto {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub sort_order: i64,
}

#[utoipa::path(
    get,
    path = "/locations",
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
