//! HTTP surface for Quartermaster.

use std::sync::Arc;

use axum::{routing::get, Json, Router};
use utoipa::OpenApi;

pub mod auth;
pub mod error;
pub mod routes;

pub use error::{ApiError, ApiResult};

#[derive(Clone, Debug)]
pub struct AppState {
    pub db: qm_db::Database,
    pub config: Arc<ApiConfig>,
}

#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub registration_mode: RegistrationMode,
    pub access_token_ttl_seconds: i64,
    pub refresh_token_ttl_seconds: i64,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            registration_mode: RegistrationMode::FirstRunOnly,
            access_token_ttl_seconds: 30 * 60,
            refresh_token_ttl_seconds: 60 * 24 * 60 * 60,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationMode {
    FirstRunOnly,
    InviteOnly,
    Open,
}

impl std::str::FromStr for RegistrationMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "first_run_only" => Ok(Self::FirstRunOnly),
            "invite_only" => Ok(Self::InviteOnly),
            "open" => Ok(Self::Open),
            other => Err(format!("unknown registration_mode: {other}")),
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Quartermaster API",
        description = "Self-hosted kitchen inventory management.",
        version = "0.1.0",
    ),
    paths(
        routes::health::healthz,
        routes::accounts::register,
        routes::accounts::login,
        routes::accounts::refresh,
        routes::accounts::logout,
        routes::accounts::me,
        routes::locations::list_locations,
    ),
    components(schemas(
        routes::health::HealthResponse,
        routes::accounts::RegisterRequest,
        routes::accounts::LoginRequest,
        routes::accounts::RefreshRequest,
        routes::accounts::TokenPair,
        routes::accounts::MeResponse,
        routes::accounts::UserDto,
        routes::accounts::HouseholdDto,
        routes::locations::LocationDto,
        error::ApiErrorBody,
    )),
    tags(
        (name = "health", description = "Liveness / readiness"),
        (name = "accounts", description = "Authentication and session"),
        (name = "locations", description = "Pantry / fridge / freezer"),
    ),
)]
pub struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let openapi_spec = ApiDoc::openapi();
    Router::new()
        .merge(routes::health::router())
        .merge(routes::accounts::router())
        .merge(routes::locations::router())
        .route(
            "/openapi.json",
            get(move || {
                let spec = openapi_spec.clone();
                async move { Json(spec) }
            }),
        )
        .with_state(state)
}
