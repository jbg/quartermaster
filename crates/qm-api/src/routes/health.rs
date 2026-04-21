use axum::{routing::get, Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[utoipa::path(
    get,
    path = "/healthz",
    operation_id = "healthz",
    tag = "health",
    responses((status = 200, body = HealthResponse)),
)]
pub async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub fn router() -> Router<AppState> {
    Router::new().route("/healthz", get(healthz))
}
