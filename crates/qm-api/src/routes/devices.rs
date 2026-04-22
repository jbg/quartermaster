use axum::{
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    auth::CurrentUser,
    error::ApiResult,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/devices/register", post(register))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PushAuthorizationStatus {
    NotDetermined,
    Denied,
    Authorized,
    Provisional,
}

impl PushAuthorizationStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NotDetermined => "not_determined",
            Self::Denied => "denied",
            Self::Authorized => "authorized",
            Self::Provisional => "provisional",
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterDeviceRequest {
    pub device_id: String,
    pub platform: String,
    pub push_token: Option<String>,
    pub push_authorization: PushAuthorizationStatus,
    pub app_version: Option<String>,
}

#[utoipa::path(
    post,
    path = "/devices/register",
    operation_id = "device_register",
    tag = "devices",
    request_body = RegisterDeviceRequest,
    responses((status = 204), (status = 401, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn register(
    axum::extract::State(state): axum::extract::State<AppState>,
    current: CurrentUser,
    Json(req): Json<RegisterDeviceRequest>,
) -> ApiResult<StatusCode> {
    qm_db::devices::upsert(
        &state.db,
        &qm_db::devices::DeviceUpsert {
            user_id: current.user_id,
            session_id: current.session_id,
            device_id: req.device_id,
            platform: req.platform,
            push_token: req.push_token,
            push_authorization: req.push_authorization.as_str().to_owned(),
            app_version: req.app_version,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
