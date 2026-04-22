//! HTTP surface for Quartermaster.

use std::{sync::Arc, time::Duration};

use axum::{routing::get, Json, Router};
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{field::Empty, Level};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

pub mod auth;
pub mod barcode;
pub mod error;
pub mod openfoodfacts;
pub mod rate_limit;
pub mod routes;
pub mod types;

pub use error::{ApiError, ApiResult};
use openfoodfacts::OffCircuitBreaker;
use rate_limit::{RateLimitLayerState, RateLimitTarget};

#[derive(Clone, Debug)]
pub struct AppState {
    pub db: qm_db::Database,
    pub config: Arc<ApiConfig>,
    pub http: reqwest::Client,
    pub off_breaker: Arc<OffCircuitBreaker>,
    pub rate_limiters: Arc<rate_limit::RateLimiters>,
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
    pub off_api_base_url: String,
    pub public_base_url: Option<String>,
    /// Whether `X-Forwarded-For` should override the socket address for
    /// client-identity keyed rate limiting.
    pub trust_proxy_headers: bool,
    pub rate_limit_auth: RateLimitConfig,
    pub rate_limit_barcode: RateLimitConfig,
    pub rate_limit_history: RateLimitConfig,
    pub off_timeout: Duration,
    pub off_max_retries: u32,
    pub off_retry_base_delay: Duration,
    pub off_circuit_breaker_failure_threshold: u32,
    pub off_circuit_breaker_open_for: Duration,
}

#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst: u32,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            registration_mode: RegistrationMode::FirstRunOnly,
            access_token_ttl_seconds: 30 * 60,
            refresh_token_ttl_seconds: 60 * 24 * 60 * 60,
            off_positive_ttl_days: 30,
            off_negative_ttl_days: 7,
            off_api_base_url: "https://world.openfoodfacts.org/api/v2/product".into(),
            public_base_url: None,
            trust_proxy_headers: false,
            rate_limit_auth: RateLimitConfig {
                requests_per_minute: 10,
                burst: 5,
            },
            rate_limit_barcode: RateLimitConfig {
                requests_per_minute: 60,
                burst: 20,
            },
            rate_limit_history: RateLimitConfig {
                requests_per_minute: 120,
                burst: 40,
            },
            off_timeout: Duration::from_secs(5),
            off_max_retries: 2,
            off_retry_base_delay: Duration::from_millis(200),
            off_circuit_breaker_failure_threshold: 5,
            off_circuit_breaker_open_for: Duration::from_secs(60),
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

/// Registers the `bearer` security scheme referenced by every
/// authenticated path's `security(("bearer" = []))` attribute. Without
/// this pass, tooling that validates the spec (e.g.
/// swift-openapi-generator) rejects the document as inconsistent.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("opaque")
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(
        title = "Quartermaster API",
        description = "Self-hosted kitchen inventory management.",
        version = "0.1.0",
    ),
    paths(
        routes::health::healthz,
        routes::join::join_landing,
        routes::accounts::register,
        routes::accounts::login,
        routes::accounts::refresh,
        routes::accounts::logout,
        routes::accounts::me,
        routes::accounts::switch_household,
        routes::locations::list_locations,
        routes::locations::create_location,
        routes::locations::update_location,
        routes::locations::delete_location,
        routes::units::list_units,
        routes::households::get_current_household,
        routes::households::update_current_household,
        routes::households::list_members,
        routes::households::remove_member,
        routes::households::create_invite,
        routes::households::list_invites,
        routes::households::revoke_invite,
        routes::households::redeem_invite,
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
        qm_core::units::UnitFamily,
        types::ProductSource,
        types::StockEventType,
        types::MembershipRole,
        routes::health::HealthResponse,
        routes::accounts::RegisterRequest,
        routes::accounts::LoginRequest,
        routes::accounts::RefreshRequest,
        routes::accounts::TokenPair,
        routes::accounts::MeResponse,
        routes::accounts::MeHouseholdDto,
        routes::accounts::SwitchHouseholdRequest,
        routes::accounts::UserDto,
        routes::accounts::HouseholdDto,
        routes::households::HouseholdDetailDto,
        routes::households::UpdateHouseholdRequest,
        routes::households::MemberDto,
        routes::households::InviteDto,
        routes::households::CreateInviteRequest,
        routes::households::RedeemInviteRequest,
        routes::locations::LocationDto,
        routes::locations::CreateLocationRequest,
        routes::locations::UpdateLocationRequest,
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
        (name = "households", description = "Household administration, invites, and members"),
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
        .merge(routes::join::router())
        .merge(routes::accounts::router(RateLimitLayerState::new(
            state.clone(),
            RateLimitTarget::Auth,
        )))
        .merge(routes::households::router())
        .merge(routes::locations::router())
        .merge(routes::units::router())
        .merge(routes::products::router(RateLimitLayerState::new(
            state.clone(),
            RateLimitTarget::Barcode,
        )))
        .merge(routes::stock::router(RateLimitLayerState::new(
            state.clone(),
            RateLimitTarget::History,
        )))
        .route(
            "/openapi.json",
            get(move || {
                let spec = openapi_spec.clone();
                async move { Json(spec) }
            }),
        )
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(
                    TraceLayer::new_for_http()
                        .on_request(())
                        .on_body_chunk(())
                        .on_eos(())
                        .make_span_with(|request: &axum::http::Request<_>| {
                            let request_id = request
                                .headers()
                                .get("x-request-id")
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or("-");

                            tracing::info_span!(
                                "http_request",
                                method = %request.method(),
                                uri = %request.uri(),
                                request_id = %request_id,
                                user_id = Empty,
                                household_id = Empty,
                            )
                        })
                        .on_response(
                            DefaultOnResponse::new()
                                .level(Level::INFO)
                                .latency_unit(LatencyUnit::Millis),
                        )
                        .on_failure(
                            DefaultOnFailure::new()
                                .level(Level::ERROR)
                                .latency_unit(LatencyUnit::Millis),
                        ),
                ),
        )
        .with_state(state)
}
