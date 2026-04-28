use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth,
    error::{ApiError, ApiResult},
    rate_limit::RateLimitLayerState,
    routes::{accounts, households},
    AppState, RegistrationMode,
};

const ROLE_ADMIN: &str = "admin";

pub fn router(rate_limit_state: RateLimitLayerState) -> Router<AppState> {
    Router::new()
        .route("/onboarding/status", get(status))
        .route(
            "/onboarding/create-household",
            post(create_household).route_layer(middleware::from_fn_with_state(
                rate_limit_state.clone(),
                crate::rate_limit::enforce,
            )),
        )
        .route(
            "/onboarding/join-invite",
            post(join_invite).route_layer(middleware::from_fn_with_state(
                rate_limit_state,
                crate::rate_limit::enforce,
            )),
        )
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingServerState {
    NeedsInitialSetup,
    Ready,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingAvailability {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingAuthMethod {
    Password,
    Passkey,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingAuthMethodAvailability {
    Enabled,
    Unavailable,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OnboardingAuthMethodDescriptor {
    pub method: OnboardingAuthMethod,
    pub availability: OnboardingAuthMethodAvailability,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OnboardingStatusResponse {
    pub server_state: OnboardingServerState,
    pub household_signup: OnboardingAvailability,
    pub invite_join: OnboardingAvailability,
    pub auth_methods: Vec<OnboardingAuthMethodDescriptor>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOnboardingHouseholdRequest {
    pub username: String,
    pub password: String,
    pub household_name: String,
    pub timezone: String,
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JoinInviteRequest {
    pub username: String,
    pub password: String,
    pub invite_code: String,
    pub device_label: Option<String>,
}

#[utoipa::path(
    get,
    path = "/onboarding/status",
    operation_id = "onboarding_status",
    tag = "onboarding",
    responses((status = 200, body = OnboardingStatusResponse)),
)]
pub async fn status(State(state): State<AppState>) -> ApiResult<Json<OnboardingStatusResponse>> {
    Ok(Json(build_status(&state).await?))
}

#[utoipa::path(
    post,
    path = "/onboarding/create-household",
    operation_id = "onboarding_create_household",
    tag = "onboarding",
    request_body = CreateOnboardingHouseholdRequest,
    responses(
        (status = 201, body = accounts::TokenPair),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn create_household(
    State(state): State<AppState>,
    Json(req): Json<CreateOnboardingHouseholdRequest>,
) -> ApiResult<(StatusCode, HeaderMap, Json<accounts::TokenPair>)> {
    accounts::validate_credentials(&req.username, &req.password)?;
    let household_name = households::validate_household_name(&req.household_name)?;
    let timezone = households::validate_household_timezone(&req.timezone)?;

    let existing_count = qm_db::users::count(&state.db).await?;
    match (state.config.registration_mode, existing_count) {
        (RegistrationMode::FirstRunOnly, 0) | (RegistrationMode::Open, _) => {}
        (RegistrationMode::FirstRunOnly, _) | (RegistrationMode::InviteOnly, _) => {
            return Err(ApiError::RegistrationDisabled);
        }
    }

    let password_hash = auth::hash_password(&req.password)?;
    let (user_id, household_id) = create_user_household(
        &state,
        &req.username,
        &password_hash,
        household_name,
        timezone,
    )
    .await?;
    let pair = accounts::issue_token_pair(
        &state,
        user_id,
        Uuid::now_v7(),
        req.device_label.as_deref(),
        Some(household_id),
    )
    .await?;
    let headers = accounts::session_cookie_headers(&state, &pair);
    Ok((StatusCode::CREATED, headers, Json(pair)))
}

#[utoipa::path(
    post,
    path = "/onboarding/join-invite",
    operation_id = "onboarding_join_invite",
    tag = "onboarding",
    request_body = JoinInviteRequest,
    responses(
        (status = 201, body = accounts::TokenPair),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn join_invite(
    State(state): State<AppState>,
    Json(req): Json<JoinInviteRequest>,
) -> ApiResult<(StatusCode, HeaderMap, Json<accounts::TokenPair>)> {
    accounts::validate_credentials(&req.username, &req.password)?;
    let code = req.invite_code.trim().to_ascii_uppercase();
    if code.is_empty() {
        return Err(ApiError::BadRequest("invite_code is required".into()));
    }
    let password_hash = auth::hash_password(&req.password)?;
    let registered = match qm_db::invites::register_user_with_invite(
        &state.db,
        &code,
        &req.username,
        None,
        &password_hash,
    )
    .await
    {
        Ok(registered) => registered,
        Err(qm_db::invites::RegisterWithInviteError::InvalidInvite) => {
            return Err(ApiError::InvalidInvite);
        }
        Err(qm_db::invites::RegisterWithInviteError::UsernameTaken) => {
            return Err(ApiError::Conflict("username already taken".into()));
        }
        Err(qm_db::invites::RegisterWithInviteError::Database(err)) => {
            return Err(ApiError::Database(err));
        }
    };
    let pair = accounts::issue_token_pair(
        &state,
        registered.user.id,
        Uuid::now_v7(),
        req.device_label.as_deref(),
        Some(registered.household_id),
    )
    .await?;
    let headers = accounts::session_cookie_headers(&state, &pair);
    Ok((StatusCode::CREATED, headers, Json(pair)))
}

async fn build_status(state: &AppState) -> ApiResult<OnboardingStatusResponse> {
    let user_count = qm_db::users::count(&state.db).await?;
    let needs_initial_setup =
        state.config.registration_mode == RegistrationMode::FirstRunOnly && user_count == 0;
    let household_signup = match (state.config.registration_mode, user_count) {
        (RegistrationMode::FirstRunOnly, 0) | (RegistrationMode::Open, _) => {
            OnboardingAvailability::Enabled
        }
        (RegistrationMode::FirstRunOnly, _) | (RegistrationMode::InviteOnly, _) => {
            OnboardingAvailability::Disabled
        }
    };
    Ok(OnboardingStatusResponse {
        server_state: if needs_initial_setup {
            OnboardingServerState::NeedsInitialSetup
        } else {
            OnboardingServerState::Ready
        },
        household_signup,
        invite_join: if needs_initial_setup {
            OnboardingAvailability::Disabled
        } else {
            OnboardingAvailability::Enabled
        },
        auth_methods: vec![
            OnboardingAuthMethodDescriptor {
                method: OnboardingAuthMethod::Password,
                availability: OnboardingAuthMethodAvailability::Enabled,
                unavailable_reason: None,
            },
            OnboardingAuthMethodDescriptor {
                method: OnboardingAuthMethod::Passkey,
                availability: OnboardingAuthMethodAvailability::Unavailable,
                unavailable_reason: Some("not_implemented".into()),
            },
        ],
    })
}

async fn create_user_household(
    state: &AppState,
    username: &str,
    password_hash: &str,
    household_name: &str,
    timezone: &str,
) -> ApiResult<(Uuid, Uuid)> {
    let mut tx = state.db.pool.begin().await?;
    let user = match qm_db::users::create_in_tx(&mut tx, username, None, password_hash).await {
        Ok(user) => user,
        Err(err) if qm_db::memberships::is_unique_violation(&err) => {
            return Err(ApiError::Conflict("username already taken".into()));
        }
        Err(err) => return Err(ApiError::Database(err)),
    };
    let household = qm_db::households::create_in_tx(&mut tx, household_name, timezone).await?;
    qm_db::locations::seed_defaults_in_tx(&mut tx, household.id).await?;
    qm_db::memberships::insert_in_tx(&mut tx, household.id, user.id, ROLE_ADMIN).await?;
    tx.commit().await?;
    Ok((user.id, household.id))
}
