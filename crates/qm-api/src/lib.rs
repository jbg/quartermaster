//! HTTP surface for Quartermaster.

use std::sync::Arc;

use axum::{routing::get, Json, Router};
use utoipa::OpenApi;

pub mod auth;
pub mod barcode;
pub mod error;
pub mod openfoodfacts;
pub mod routes;

pub use error::{ApiError, ApiResult};

#[derive(Clone, Debug)]
pub struct AppState {
    pub db: qm_db::Database,
    pub config: Arc<ApiConfig>,
    pub http: reqwest::Client,
}

#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub registration_mode: RegistrationMode,
    pub access_token_ttl_seconds: i64,
    pub refresh_token_ttl_seconds: i64,
    /// How many days a positive barcode-cache entry (`barcode → product`) is
    /// considered fresh before we re-fetch from OpenFoodFacts.
    pub off_positive_ttl_days: i64,
    /// How many days a negative barcode-cache entry (`barcode → miss`) is
    /// considered fresh.
    pub off_negative_ttl_days: i64,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            registration_mode: RegistrationMode::FirstRunOnly,
            access_token_ttl_seconds: 30 * 60,
            refresh_token_ttl_seconds: 60 * 24 * 60 * 60,
            off_positive_ttl_days: 30,
            off_negative_ttl_days: 7,
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
        routes::units::list_units,
        routes::products::search,
        routes::products::by_barcode,
        routes::products::create,
        routes::products::get_one,
        routes::products::update,
        routes::products::delete_one,
        routes::products::refresh,
        routes::products::restore,
        routes::stock::list,
        routes::stock::get_one,
        routes::stock::create,
        routes::stock::update,
        routes::stock::delete_one,
        routes::stock::consume,
        routes::stock::list_events,
        routes::stock::list_events_for_batch,
        routes::stock::restore_one,
        routes::stock::restore_many,
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
        routes::units::UnitDto,
        routes::products::ProductDto,
        routes::products::CreateProductRequest,
        routes::products::UpdateProductRequest,
        routes::products::ProductSearchResponse,
        routes::products::BarcodeLookupResponse,
        routes::stock::StockBatchDto,
        routes::stock::StockListResponse,
        routes::stock::CreateStockRequest,
        routes::stock::UpdateStockRequest,
        routes::stock::ConsumeRequest,
        routes::stock::ConsumedBatchDto,
        routes::stock::ConsumeResponse,
        routes::stock::StockEventDto,
        routes::stock::StockEventListResponse,
        routes::stock::RestoreManyRequest,
        routes::stock::RestoreManyResponse,
        error::ApiErrorBody,
    )),
    tags(
        (name = "health", description = "Liveness / readiness"),
        (name = "accounts", description = "Authentication and session"),
        (name = "locations", description = "Pantry / fridge / freezer"),
        (name = "units", description = "Units of measure"),
        (name = "products", description = "Product catalogue and barcode lookup"),
        (name = "stock", description = "Batches of stock and FIFO consumption"),
    ),
)]
pub struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let openapi_spec = ApiDoc::openapi();
    Router::new()
        .merge(routes::health::router())
        .merge(routes::accounts::router())
        .merge(routes::locations::router())
        .merge(routes::units::router())
        .merge(routes::products::router())
        .merge(routes::stock::router())
        .route(
            "/openapi.json",
            get(move || {
                let spec = openapi_spec.clone();
                async move { Json(spec) }
            }),
        )
        .with_state(state)
}
