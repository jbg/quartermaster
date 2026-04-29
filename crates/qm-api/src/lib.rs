//! HTTP surface for Quartermaster.

use std::{path::PathBuf, sync::Arc, time::Duration};

use axum::{
    extract::{MatchedPath, State},
    http::{header, HeaderName, HeaderValue, Method, StatusCode, Uri},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    services::ServeDir,
    trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{field::Empty, Level};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    openapi::{OpenApi as UtoipaOpenApi, Paths},
    Modify, OpenApi,
};

pub mod auth;
pub mod barcode;
pub mod error;
pub mod labels;
pub mod openfoodfacts;
pub mod rate_limit;
pub mod routes;
pub mod types;

pub use error::{ApiError, ApiResult};
use openfoodfacts::OffCircuitBreaker;
use rate_limit::{ClientIpMode, RateLimitLayerState, RateLimitTarget, TrustedProxyNet};

pub const API_PREFIX: &str = "/api/v1";

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
    /// Which client-IP source to use for rate-limit keys.
    pub rate_limit_client_ip_mode: ClientIpMode,
    pub rate_limit_trusted_proxy_cidrs: Vec<TrustedProxyNet>,
    pub rate_limit_auth: RateLimitConfig,
    pub rate_limit_barcode: RateLimitConfig,
    pub rate_limit_history: RateLimitConfig,
    pub off_timeout: Duration,
    pub off_max_retries: u32,
    pub off_retry_base_delay: Duration,
    pub off_circuit_breaker_failure_threshold: u32,
    pub off_circuit_breaker_open_for: Duration,
    pub ios_release_identity: Option<IosReleaseIdentity>,
    pub auth_session_sweep_trigger_secret: Option<String>,
    pub expiry_reminder_policy: qm_db::reminders::ExpiryReminderPolicy,
    pub expiry_reminder_trigger_secret: Option<String>,
    pub smoke_seed_trigger_secret: Option<String>,
    pub web_dist_dir: Option<PathBuf>,
    pub web_auth_allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IosReleaseIdentity {
    team_id: String,
    bundle_id: String,
}

impl IosReleaseIdentity {
    pub fn new(team_id: String, bundle_id: String) -> Result<Self, String> {
        validate_team_id(&team_id)?;
        validate_bundle_id(&bundle_id)?;
        Ok(Self { team_id, bundle_id })
    }

    pub fn team_id(&self) -> &str {
        &self.team_id
    }

    pub fn bundle_id(&self) -> &str {
        &self.bundle_id
    }

    pub fn app_id(&self) -> String {
        format!("{}.{}", self.team_id, self.bundle_id)
    }
}

fn validate_team_id(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("iOS team ID must not be blank".into());
    }
    if !value.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err("iOS team ID must be ASCII alphanumeric".into());
    }
    Ok(())
}

fn validate_bundle_id(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("iOS bundle ID must not be blank".into());
    }
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err("iOS bundle ID must be dot-separated".into());
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
    {
        return Err(
            "iOS bundle ID must contain only ASCII alphanumeric characters, dots, or hyphens"
                .into(),
        );
    }
    Ok(())
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
            rate_limit_client_ip_mode: ClientIpMode::Socket,
            rate_limit_trusted_proxy_cidrs: Vec::new(),
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
            ios_release_identity: None,
            auth_session_sweep_trigger_secret: None,
            expiry_reminder_policy: qm_db::reminders::ExpiryReminderPolicy::default(),
            expiry_reminder_trigger_secret: None,
            smoke_seed_trigger_secret: None,
            web_dist_dir: None,
            web_auth_allowed_origins: Vec::new(),
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
        routes::accounts::register,
        routes::accounts::login,
        routes::accounts::refresh,
        routes::accounts::logout,
        routes::accounts::me,
        routes::accounts::switch_household,
        routes::accounts::request_email_verification,
        routes::accounts::confirm_email_verification,
        routes::accounts::clear_recovery_email,
        routes::devices::register,
        routes::households::create_household,
        routes::locations::list_locations,
        routes::locations::create_location,
        routes::locations::update_location,
        routes::locations::delete_location,
        routes::label_printers::list_label_printers,
        routes::label_printers::create_label_printer,
        routes::label_printers::update_label_printer,
        routes::label_printers::delete_label_printer,
        routes::label_printers::test_label_printer,
        routes::units::list_units,
        routes::households::get_current_household,
        routes::households::update_current_household,
        routes::households::list_members,
        routes::households::remove_member,
        routes::households::create_invite,
        routes::households::list_invites,
        routes::households::revoke_invite,
        routes::households::redeem_invite,
        routes::onboarding::status,
        routes::onboarding::create_household,
        routes::onboarding::join_invite,
        routes::products::list,
        routes::products::search,
        routes::products::by_barcode,
        routes::products::create,
        routes::products::get_one,
        routes::products::update,
        routes::products::delete_one,
        routes::products::refresh,
        routes::products::restore,
        routes::reminders::list,
        routes::reminders::present,
        routes::reminders::open,
        routes::reminders::ack,
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
        routes::label_printers::print_stock_label,
    ),
    components(schemas(
        qm_core::units::UnitFamily,
        types::ProductSource,
        types::StockEventType,
        types::MembershipRole,
        types::ReminderKind,
        types::ReminderUrgency,
        types::LabelPrinterDriver,
        types::LabelPrinterMedia,
        routes::health::HealthResponse,
        routes::accounts::RegisterRequest,
        routes::accounts::LoginRequest,
        routes::accounts::RefreshRequest,
        routes::accounts::TokenPair,
        routes::accounts::MeResponse,
        routes::accounts::HouseholdSummaryDto,
        routes::accounts::SwitchHouseholdRequest,
        routes::accounts::RequestEmailVerificationRequest,
        routes::accounts::RequestEmailVerificationResponse,
        routes::accounts::ConfirmEmailVerificationRequest,
        routes::accounts::UserDto,
        routes::accounts::HouseholdDto,
        routes::devices::RegisterDeviceRequest,
        routes::devices::PushAuthorizationStatus,
        routes::households::HouseholdDetailDto,
        routes::households::CreateHouseholdRequest,
        routes::households::UpdateHouseholdRequest,
        routes::households::MemberDto,
        routes::households::InviteDto,
        routes::households::CreateInviteRequest,
        routes::households::RedeemInviteRequest,
        routes::onboarding::OnboardingStatusResponse,
        routes::onboarding::OnboardingServerState,
        routes::onboarding::OnboardingAvailability,
        routes::onboarding::OnboardingAuthMethod,
        routes::onboarding::OnboardingAuthMethodAvailability,
        routes::onboarding::OnboardingAuthMethodDescriptor,
        routes::onboarding::CreateOnboardingHouseholdRequest,
        routes::onboarding::JoinInviteRequest,
        routes::locations::LocationDto,
        routes::locations::CreateLocationRequest,
        routes::locations::UpdateLocationRequest,
        routes::label_printers::LabelPrinterDto,
        routes::label_printers::LabelPrinterListResponse,
        routes::label_printers::CreateLabelPrinterRequest,
        routes::label_printers::UpdateLabelPrinterRequest,
        routes::label_printers::PrintStockLabelRequest,
        routes::label_printers::PrintStockLabelResponse,
        routes::label_printers::LabelPrintStatus,
        routes::units::UnitDto,
        routes::products::ProductDto,
        routes::products::CreateProductRequest,
        routes::products::ProductSearchResponse,
        routes::products::BarcodeLookupResponse,
        routes::patch::JsonPatchOperation,
        routes::reminders::ReminderDto,
        routes::reminders::ReminderListResponse,
        routes::stock::StockBatchDto,
        routes::stock::StockListResponse,
        routes::stock::CreateStockRequest,
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
        (name = "devices", description = "Notification-capable client registrations"),
        (name = "households", description = "Household administration, invites, and members"),
        (name = "onboarding", description = "First-launch server setup and joining"),
        (name = "locations", description = "Pantry / fridge / freezer"),
        (name = "label-printers", description = "Household label printer configuration and print jobs"),
        (name = "units", description = "Units of measure"),
        (name = "products", description = "Product catalogue and barcode lookup"),
        (name = "reminders", description = "Backend-owned household reminders"),
        (name = "stock", description = "Batches of stock and FIFO consumption"),
    ),
)]
pub struct ApiDoc;

pub fn openapi_spec() -> UtoipaOpenApi {
    let spec = ApiDoc::openapi();
    UtoipaOpenApi::new(spec.info.clone(), Paths::new()).nest(API_PREFIX, spec)
}

pub fn router(state: AppState) -> Router {
    let openapi_spec = openapi_spec();
    let web_dist_dir = state.config.web_dist_dir.clone();
    let api_routes = Router::new()
        .merge(routes::health::router())
        .merge(routes::accounts::router(RateLimitLayerState::new(
            state.clone(),
            RateLimitTarget::Auth,
        )))
        .merge(routes::onboarding::router(RateLimitLayerState::new(
            state.clone(),
            RateLimitTarget::Auth,
        )))
        .merge(routes::devices::router())
        .merge(routes::households::router())
        .merge(routes::locations::router())
        .merge(routes::label_printers::router())
        .merge(routes::units::router())
        .merge(routes::products::router(RateLimitLayerState::new(
            state.clone(),
            RateLimitTarget::Barcode,
        )))
        .merge(routes::reminders::router())
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
        );
    let api_routes = with_web_auth_cors(api_routes, &state.config.web_auth_allowed_origins);
    let maintenance_routes = if state.config.auth_session_sweep_trigger_secret.is_some()
        || state.config.expiry_reminder_trigger_secret.is_some()
        || state.config.smoke_seed_trigger_secret.is_some()
    {
        routes::maintenance::router()
    } else {
        Router::new()
    };

    let app = Router::new()
        .merge(routes::health::router())
        .merge(routes::join::router())
        .nest(API_PREFIX, api_routes)
        .merge(
            // Operator hooks stay out of the public API contract and keep
            // their stable deployment paths.
            maintenance_routes,
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
                            let route = request
                                .extensions()
                                .get::<MatchedPath>()
                                .map(MatchedPath::as_str)
                                .unwrap_or_else(|| request.uri().path());

                            tracing::info_span!(
                                "http_request",
                                method = %request.method(),
                                route = %route,
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
        .with_state(state);

    if let Some(web_dist_dir) = web_dist_dir {
        app.merge(web_router(web_dist_dir))
    } else {
        app
    }
}

fn with_web_auth_cors(router: Router<AppState>, allowed_origins: &[String]) -> Router<AppState> {
    if allowed_origins.is_empty() {
        return router;
    }
    let allowed: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    router.layer(
        CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(move |origin, _| {
                allowed.iter().any(|allowed| allowed == origin)
            }))
            .allow_credentials(true)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
                HeaderName::from_static(auth::CSRF_HEADER),
            ]),
    )
}

fn web_router(dist_dir: PathBuf) -> Router {
    let files = WebFiles {
        index: dist_dir.join("index.html"),
        join: dist_dir.join("join.html"),
        fallback: dist_dir.join("200.html"),
    };
    Router::new()
        .nest_service("/_app", ServeDir::new(dist_dir.join("_app")))
        .nest_service("/brand", ServeDir::new(dist_dir.join("brand")))
        .route("/", get(serve_web_index))
        .route("/join", get(serve_web_join))
        .fallback(web_fallback)
        .with_state(files)
}

#[derive(Clone)]
struct WebFiles {
    index: PathBuf,
    join: PathBuf,
    fallback: PathBuf,
}

async fn serve_web_index(State(files): State<WebFiles>) -> impl IntoResponse {
    serve_web_file(files.index).await
}

async fn serve_web_join(State(files): State<WebFiles>) -> impl IntoResponse {
    serve_web_file(files.join).await
}

async fn web_fallback(
    method: Method,
    uri: Uri,
    State(files): State<WebFiles>,
) -> impl IntoResponse {
    if method != Method::GET || is_api_path(uri.path()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_web_file(files.fallback).await
}

async fn serve_web_file(fallback_file: PathBuf) -> axum::response::Response {
    match tokio::fs::read_to_string(fallback_file).await {
        Ok(body) => Html(body).into_response(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(err) => {
            tracing::error!(?err, "failed to read web fallback");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn is_api_path(path: &str) -> bool {
    path == "/healthz"
        || path
            .strip_prefix(API_PREFIX)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        || path == "/docs"
        || path.starts_with("/docs/")
        || path == "/.well-known/apple-app-site-association"
        || path.starts_with("/internal/")
}

#[cfg(test)]
mod tests {
    use super::IosReleaseIdentity;

    #[test]
    fn creates_app_id_from_valid_ios_identity() {
        let identity =
            IosReleaseIdentity::new("42J2SSX5SM".into(), "com.example.quartermaster".into())
                .unwrap();
        assert_eq!(identity.app_id(), "42J2SSX5SM.com.example.quartermaster");
    }

    #[test]
    fn rejects_invalid_ios_team_id() {
        let err = IosReleaseIdentity::new("TEAM ID".into(), "com.example.quartermaster".into())
            .unwrap_err();
        assert!(err.contains("ASCII alphanumeric"));
    }

    #[test]
    fn rejects_invalid_ios_bundle_id() {
        let err = IosReleaseIdentity::new("42J2SSX5SM".into(), "com example".into()).unwrap_err();
        assert!(err.contains("dots, or hyphens"));
    }
}
