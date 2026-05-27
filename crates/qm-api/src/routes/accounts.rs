use std::str::FromStr;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware,
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use jiff::{SignedDuration, Timestamp};
use qm_core::units::MeasurementSystem;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use utoipa::{
    openapi::{
        schema::{AllOfBuilder, ArrayBuilder, ObjectBuilder, Schema, SchemaType, Type},
        Ref, RefOr,
    },
    PartialSchema, ToSchema,
};
use uuid::Uuid;
use webauthn_rs::prelude::{
    Passkey, PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential, Url, Webauthn, WebauthnBuilder,
};

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rand::RngExt;

use crate::{
    auth::{self, CurrentUser},
    email::{EmailAddress, EmailMessage},
    error::{ApiError, ApiResult},
    rate_limit::RateLimitLayerState,
    AppState, RegistrationMode,
};

pub fn router(rate_limit_state: RateLimitLayerState) -> Router<AppState> {
    Router::new()
        .route(
            "/auth/register",
            post(register).route_layer(middleware::from_fn_with_state(
                rate_limit_state.clone(),
                crate::rate_limit::enforce,
            )),
        )
        .route(
            "/auth/login",
            post(login).route_layer(middleware::from_fn_with_state(
                rate_limit_state.clone(),
                crate::rate_limit::enforce,
            )),
        )
        .route(
            "/auth/refresh",
            post(refresh).route_layer(middleware::from_fn_with_state(
                rate_limit_state.clone(),
                crate::rate_limit::enforce,
            )),
        )
        .route("/auth/logout", post(logout))
        .route("/auth/passkeys", get(list_passkeys))
        .route(
            "/auth/passkeys/register/start",
            post(start_passkey_registration),
        )
        .route(
            "/auth/passkeys/register/finish",
            post(finish_passkey_registration),
        )
        .route("/auth/passkeys/login/start", post(start_passkey_login))
        .route("/auth/passkeys/login/finish", post(finish_passkey_login))
        .route("/auth/passkeys/{credential_id}", delete(delete_passkey))
        .route("/auth/handoffs", post(create_auth_handoff))
        .route("/auth/handoffs/{handoff_id}", delete(cancel_auth_handoff))
        .route("/auth/handoffs/preview", post(preview_auth_handoff))
        .route("/auth/handoffs/accept", post(accept_auth_handoff))
        .route("/auth/switch-household", post(switch_household))
        .route("/auth/email-verification", post(request_email_verification))
        .route(
            "/auth/email-verification/confirm",
            post(confirm_email_verification),
        )
        .route(
            "/auth/password-reset/request",
            post(request_password_reset).route_layer(middleware::from_fn_with_state(
                rate_limit_state.clone(),
                crate::rate_limit::enforce,
            )),
        )
        .route(
            "/auth/password-reset/confirm",
            post(confirm_password_reset).route_layer(middleware::from_fn_with_state(
                rate_limit_state,
                crate::rate_limit::enforce,
            )),
        )
        .route("/auth/email", delete(clear_recovery_email))
        .route("/auth/me", get(me))
        .route(
            "/account/openfoodfacts",
            put(put_openfoodfacts_credentials).delete(delete_openfoodfacts_credentials),
        )
        .route(
            "/account/openfoodfacts/status",
            get(get_openfoodfacts_status),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    pub display_name: String,
    pub password: String,
    /// Required unless the server is in `first_run_only` mode and no users
    /// exist yet, or in `open` mode.
    pub invite_code: Option<String>,
    /// Optional label applied to the refresh token (shown on `/api/v1/auth/me`).
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub email_verified_at: Option<String>,
    pub pending_email: Option<String>,
    pub pending_email_verification_expires_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HouseholdDto {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub measurement_system: MeasurementSystem,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct HouseholdSummaryDto {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub measurement_system: MeasurementSystem,
    pub role: crate::types::MembershipRole,
    pub joined_at: String,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user: UserDto,
    pub current_household: Option<HouseholdSummaryDto>,
    pub households: Vec<HouseholdSummaryDto>,
    pub public_base_url: Option<String>,
}

impl PartialSchema for MeResponse {
    fn schema() -> RefOr<Schema> {
        let nullable_current_household = Schema::AllOf(
            AllOfBuilder::new()
                .item(Ref::from_schema_name(HouseholdSummaryDto::name()))
                .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
                .build(),
        );

        ObjectBuilder::new()
            .property("user", Ref::from_schema_name(UserDto::name()))
            .required("user")
            .property("current_household", nullable_current_household)
            .property(
                "households",
                ArrayBuilder::new()
                    .items(Ref::from_schema_name(HouseholdSummaryDto::name()))
                    .build(),
            )
            .required("households")
            .property("public_base_url", String::schema())
            .into()
    }
}

impl ToSchema for MeResponse {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((UserDto::name().into_owned(), UserDto::schema()));
        <UserDto as ToSchema>::schemas(schemas);
        schemas.push((
            HouseholdSummaryDto::name().into_owned(),
            HouseholdSummaryDto::schema(),
        ));
        <HouseholdSummaryDto as ToSchema>::schemas(schemas);
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SwitchHouseholdRequest {
    pub household_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestEmailVerificationRequest {
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RequestEmailVerificationResponse {
    pub pending_email: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfirmEmailVerificationRequest {
    pub code: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordResetConfirmRequest {
    pub email: String,
    pub new_password: String,
    pub code: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasswordResetRequestResponse {
    pub status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OpenFoodFactsCredentialStatusResponse {
    pub configured: bool,
    pub username: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasskeyCredentialDto {
    pub id: Uuid,
    pub label: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasskeyListResponse {
    pub credentials: Vec<PasskeyCredentialDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasskeyRegistrationStartRequest {
    pub label: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasskeyRegistrationStartResponse {
    pub ceremony_id: Uuid,
    #[schema(nullable = false)]
    pub public_key: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasskeyRegistrationFinishRequest {
    pub ceremony_id: Uuid,
    #[schema(nullable = false)]
    pub credential: Value,
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasskeyLoginStartRequest {
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasskeyLoginStartResponse {
    pub ceremony_id: Uuid,
    #[schema(nullable = false)]
    pub public_key: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasskeyLoginFinishRequest {
    pub ceremony_id: Uuid,
    #[schema(nullable = false)]
    pub credential: Value,
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAuthHandoffRequest {
    pub target_device_label: Option<String>,
    pub server_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthHandoffCreateResponse {
    pub id: Uuid,
    pub handoff_url: String,
    pub expires_at: String,
    pub target_device_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthHandoffTokenRequest {
    pub id: Uuid,
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthHandoffPreviewResponse {
    pub id: Uuid,
    pub source_email: String,
    pub source_display_name: String,
    pub household_id: Option<Uuid>,
    pub target_device_label: Option<String>,
    pub expires_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthHandoffAcceptRequest {
    pub id: Uuid,
    pub token: String,
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveOpenFoodFactsCredentialsRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenFoodFactsPlainCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SignupVerification {
    pub code: String,
    pub code_hash: String,
    pub expires_at: String,
}

#[utoipa::path(
    post,
    path = "/auth/register",
    operation_id = "auth_register",
    tag = "accounts",
    request_body = RegisterRequest,
    responses(
        (status = 201, body = TokenPair),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, HeaderMap, Json<TokenPair>)> {
    let email = validate_email(&req.email)?;
    let display_name = validate_display_name(&req.display_name)?;
    validate_password(&req.password)?;
    let verification = signup_verification(&state, &email).await?;

    let existing_count = qm_db::users::count(&state.db).await?;
    let password_hash = auth::hash_password(&req.password)?;

    let user = match (state.config.registration_mode, existing_count) {
        (RegistrationMode::FirstRunOnly, 0) => {
            if qm_db::users::find_by_email(&state.db, &email)
                .await?
                .is_some()
            {
                return Err(ApiError::Conflict("email already registered".into()));
            }
            create_user_with_pending_verification(
                &state,
                &email,
                display_name,
                &password_hash,
                &verification,
            )
            .await?
        }
        (RegistrationMode::FirstRunOnly, _) => {
            return Err(ApiError::RegistrationDisabled);
        }
        (RegistrationMode::Open, _) => {
            if qm_db::users::find_by_email(&state.db, &email)
                .await?
                .is_some()
            {
                return Err(ApiError::Conflict("email already registered".into()));
            }
            create_user_with_pending_verification(
                &state,
                &email,
                display_name,
                &password_hash,
                &verification,
            )
            .await?
        }
        (RegistrationMode::InviteOnly, _) => {
            let code = req
                .invite_code
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ApiError::BadRequest("invite_code is required".into()))?
                .to_ascii_uppercase();
            let mut tx = qm_db::invites::begin_invite_tx(&state.db).await?;
            match qm_db::invites::register_user_with_invite_in_tx(
                &state.db,
                &mut tx,
                &code,
                &email,
                display_name,
                &password_hash,
                Some((&verification.code_hash, &verification.expires_at)),
            )
            .await
            {
                Ok(registered) => {
                    send_signup_verification(
                        &state,
                        &email,
                        &verification.code,
                        &verification.expires_at,
                    )
                    .await?;
                    tx.commit().await?;
                    registered.user
                }
                Err(qm_db::invites::RegisterWithInviteError::InvalidInvite) => {
                    return Err(ApiError::InvalidInvite);
                }
                Err(qm_db::invites::RegisterWithInviteError::EmailTaken) => {
                    return Err(ApiError::Conflict("email already registered".into()));
                }
                Err(qm_db::invites::RegisterWithInviteError::Database(err)) => {
                    return Err(ApiError::Database(err));
                }
            }
        }
    };
    let initial_household_id = qm_db::households::find_for_user(&state.db, user.id)
        .await?
        .map(|household| household.id);
    let pair = issue_token_pair(
        &state,
        user.id,
        Uuid::now_v7(),
        req.device_label.as_deref(),
        initial_household_id,
    )
    .await?;
    let headers = session_cookie_headers(&state, &pair);
    Ok((StatusCode::CREATED, headers, Json(pair)))
}

#[utoipa::path(
    get,
    path = "/account/openfoodfacts/status",
    operation_id = "account_openfoodfacts_status",
    tag = "accounts",
    responses(
        (status = 200, body = OpenFoodFactsCredentialStatusResponse),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn get_openfoodfacts_status(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<OpenFoodFactsCredentialStatusResponse>> {
    let row = qm_db::off_credentials::get(&state.db, current.user_id).await?;
    Ok(Json(OpenFoodFactsCredentialStatusResponse {
        configured: row.is_some(),
        username: row.map(|row| row.off_username),
    }))
}

#[utoipa::path(
    put,
    path = "/account/openfoodfacts",
    operation_id = "account_openfoodfacts_put",
    tag = "accounts",
    request_body = SaveOpenFoodFactsCredentialsRequest,
    responses(
        (status = 200, body = OpenFoodFactsCredentialStatusResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn put_openfoodfacts_credentials(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<SaveOpenFoodFactsCredentialsRequest>,
) -> ApiResult<Json<OpenFoodFactsCredentialStatusResponse>> {
    let username = req.username.trim();
    if username.is_empty() || username.len() > 128 {
        return Err(ApiError::BadRequest(
            "OpenFoodFacts username must be 1..=128 chars".into(),
        ));
    }
    if req.password.is_empty() {
        return Err(ApiError::BadRequest(
            "OpenFoodFacts password must not be empty".into(),
        ));
    }
    let encrypted_password = encrypt_off_password(&state, current.user_id, &req.password)?;
    let row =
        qm_db::off_credentials::upsert(&state.db, current.user_id, username, &encrypted_password)
            .await?;
    Ok(Json(OpenFoodFactsCredentialStatusResponse {
        configured: true,
        username: Some(row.off_username),
    }))
}

#[utoipa::path(
    delete,
    path = "/account/openfoodfacts",
    operation_id = "account_openfoodfacts_delete",
    tag = "accounts",
    responses(
        (status = 204),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_openfoodfacts_credentials(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<StatusCode> {
    qm_db::off_credentials::delete(&state.db, current.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/auth/passkeys",
    operation_id = "auth_passkeys_list",
    tag = "accounts",
    responses(
        (status = 200, body = PasskeyListResponse),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list_passkeys(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<PasskeyListResponse>> {
    let credentials = qm_db::passkeys::list_credentials(&state.db, current.user_id)
        .await?
        .into_iter()
        .map(passkey_credential_dto)
        .collect();
    Ok(Json(PasskeyListResponse { credentials }))
}

#[utoipa::path(
    post,
    path = "/auth/passkeys/register/start",
    operation_id = "auth_passkey_registration_start",
    tag = "accounts",
    request_body = PasskeyRegistrationStartRequest,
    responses(
        (status = 200, body = PasskeyRegistrationStartResponse),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn start_passkey_registration(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(_req): Json<PasskeyRegistrationStartRequest>,
) -> ApiResult<Json<PasskeyRegistrationStartResponse>> {
    let webauthn = passkey_webauthn(&state)?;
    let user = qm_db::users::find_by_id(&state.db, current.user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let existing = qm_db::passkeys::list_credentials(&state.db, current.user_id).await?;
    let exclude = existing
        .into_iter()
        .map(|row| passkey_from_json(&row.passkey_json).map(|passkey| passkey.cred_id().clone()))
        .collect::<ApiResult<Vec<_>>>()?;
    let (challenge, ceremony_state) = webauthn
        .start_passkey_registration(user.id, &user.email, &user.display_name, Some(exclude))
        .map_err(passkey_error)?;
    let state_json = serde_json::to_string(&ceremony_state).map_err(json_error)?;
    let expires_at = Timestamp::now()
        .checked_add(SignedDuration::from_mins(10))
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("passkey expiry overflow: {e}")))?;
    let ceremony = qm_db::passkeys::create_ceremony(
        &state.db,
        Some(current.user_id),
        qm_db::passkeys::CEREMONY_REGISTRATION,
        &state_json,
        expires_at,
    )
    .await?;
    Ok(Json(PasskeyRegistrationStartResponse {
        ceremony_id: ceremony.id,
        public_key: serde_json::to_value(challenge).map_err(json_error)?,
    }))
}

#[utoipa::path(
    post,
    path = "/auth/passkeys/register/finish",
    operation_id = "auth_passkey_registration_finish",
    tag = "accounts",
    request_body = PasskeyRegistrationFinishRequest,
    responses(
        (status = 201, body = PasskeyCredentialDto),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn finish_passkey_registration(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<PasskeyRegistrationFinishRequest>,
) -> ApiResult<(StatusCode, Json<PasskeyCredentialDto>)> {
    let webauthn = passkey_webauthn(&state)?;
    let ceremony = qm_db::passkeys::consume_ceremony(
        &state.db,
        req.ceremony_id,
        qm_db::passkeys::CEREMONY_REGISTRATION,
        Some(current.user_id),
        Timestamp::now(),
    )
    .await?
    .ok_or_else(|| ApiError::BadRequest("passkey ceremony is invalid or expired".into()))?;
    let registration_state: PasskeyRegistration =
        serde_json::from_str(&ceremony.state_json).map_err(json_error)?;
    let credential: RegisterPublicKeyCredential = serde_json::from_value(req.credential)
        .map_err(|_| ApiError::BadRequest("passkey registration credential is invalid".into()))?;
    let passkey = webauthn
        .finish_passkey_registration(&credential, &registration_state)
        .map_err(passkey_error)?;
    let credential_id = passkey_credential_id(&passkey)?;
    if qm_db::passkeys::find_credential_by_credential_id(&state.db, &credential_id)
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict("passkey is already registered".into()));
    }
    let passkey_json = serde_json::to_string(&passkey).map_err(json_error)?;
    let label = req
        .label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let row = qm_db::passkeys::insert_credential(
        &state.db,
        current.user_id,
        &credential_id,
        label,
        &passkey_json,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(passkey_credential_dto(row))))
}

#[utoipa::path(
    post,
    path = "/auth/passkeys/login/start",
    operation_id = "auth_passkey_login_start",
    tag = "accounts",
    request_body = PasskeyLoginStartRequest,
    responses(
        (status = 200, body = PasskeyLoginStartResponse),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn start_passkey_login(
    State(state): State<AppState>,
    Json(req): Json<PasskeyLoginStartRequest>,
) -> ApiResult<Json<PasskeyLoginStartResponse>> {
    let webauthn = passkey_webauthn(&state)?;
    let email = validate_email(&req.email)?;
    let user = qm_db::users::find_by_email(&state.db, &email)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let credentials = qm_db::passkeys::list_credentials(&state.db, user.id).await?;
    if credentials.is_empty() {
        return Err(ApiError::Unauthorized);
    }
    let passkeys = credentials
        .iter()
        .map(|row| passkey_from_json(&row.passkey_json))
        .collect::<ApiResult<Vec<_>>>()?;
    let (challenge, ceremony_state) = webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(passkey_error)?;
    let state_json = serde_json::to_string(&ceremony_state).map_err(json_error)?;
    let expires_at = Timestamp::now()
        .checked_add(SignedDuration::from_mins(10))
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("passkey expiry overflow: {e}")))?;
    let ceremony = qm_db::passkeys::create_ceremony(
        &state.db,
        Some(user.id),
        qm_db::passkeys::CEREMONY_AUTHENTICATION,
        &state_json,
        expires_at,
    )
    .await?;
    Ok(Json(PasskeyLoginStartResponse {
        ceremony_id: ceremony.id,
        public_key: serde_json::to_value(challenge).map_err(json_error)?,
    }))
}

#[utoipa::path(
    post,
    path = "/auth/passkeys/login/finish",
    operation_id = "auth_passkey_login_finish",
    tag = "accounts",
    request_body = PasskeyLoginFinishRequest,
    responses(
        (status = 200, body = TokenPair),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn finish_passkey_login(
    State(state): State<AppState>,
    Json(req): Json<PasskeyLoginFinishRequest>,
) -> ApiResult<(HeaderMap, Json<TokenPair>)> {
    let webauthn = passkey_webauthn(&state)?;
    let ceremony = qm_db::passkeys::consume_ceremony(
        &state.db,
        req.ceremony_id,
        qm_db::passkeys::CEREMONY_AUTHENTICATION,
        None,
        Timestamp::now(),
    )
    .await?
    .ok_or_else(|| ApiError::BadRequest("passkey ceremony is invalid or expired".into()))?;
    let user_id = ceremony.user_id.ok_or(ApiError::Unauthorized)?;
    let auth_state: PasskeyAuthentication =
        serde_json::from_str(&ceremony.state_json).map_err(json_error)?;
    let credential: PublicKeyCredential = serde_json::from_value(req.credential)
        .map_err(|_| ApiError::BadRequest("passkey login credential is invalid".into()))?;
    let auth_result = webauthn
        .finish_passkey_authentication(&credential, &auth_state)
        .map_err(passkey_error)?;
    let credentials = qm_db::passkeys::list_credentials(&state.db, user_id).await?;
    let mut matched = None;
    for row in credentials {
        let mut passkey = passkey_from_json(&row.passkey_json)?;
        if passkey.update_credential(&auth_result).is_some() {
            let passkey_json = serde_json::to_string(&passkey).map_err(json_error)?;
            qm_db::passkeys::update_credential_after_auth(&state.db, row.id, &passkey_json).await?;
            matched = Some(row);
            break;
        }
    }
    let row = matched.ok_or(ApiError::Unauthorized)?;
    let initial_household_id = qm_db::households::find_for_user(&state.db, row.user_id)
        .await?
        .map(|household| household.id);
    let pair = issue_token_pair(
        &state,
        row.user_id,
        Uuid::now_v7(),
        req.device_label.as_deref(),
        initial_household_id,
    )
    .await?;
    let headers = session_cookie_headers(&state, &pair);
    Ok((headers, Json(pair)))
}

#[utoipa::path(
    delete,
    path = "/auth/passkeys/{credential_id}",
    operation_id = "auth_passkey_delete",
    tag = "accounts",
    responses(
        (status = 204),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_passkey(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(credential_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let deleted =
        qm_db::passkeys::delete_credential(&state.db, credential_id, current.user_id).await?;
    if !deleted {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/auth/handoffs",
    operation_id = "auth_handoff_create",
    tag = "accounts",
    request_body = CreateAuthHandoffRequest,
    responses(
        (status = 201, body = AuthHandoffCreateResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn create_auth_handoff(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateAuthHandoffRequest>,
) -> ApiResult<(StatusCode, Json<AuthHandoffCreateResponse>)> {
    let token = auth::generate_token();
    let token_hash = auth::sha256_hex(&token);
    let expires_at = Timestamp::now()
        .checked_add(SignedDuration::from_mins(5))
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("handoff expiry overflow: {e}")))?;
    let target_device_label = req
        .target_device_label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let handoff = qm_db::auth_handoff::create(
        &state.db,
        current.user_id,
        current.session_id,
        current.household_id,
        target_device_label,
        &token_hash,
        expires_at,
    )
    .await?;
    let server_url = handoff_server_url(&state, req.server_url.as_deref())?;
    let handoff_url = format!(
        "quartermaster://handoff?server={}&id={}&token={}",
        utf8_percent_encode(&server_url, NON_ALPHANUMERIC),
        handoff.id,
        utf8_percent_encode(&token, NON_ALPHANUMERIC)
    );
    Ok((
        StatusCode::CREATED,
        Json(AuthHandoffCreateResponse {
            id: handoff.id,
            handoff_url,
            expires_at: handoff.expires_at,
            target_device_label: handoff.target_device_label,
        }),
    ))
}

#[utoipa::path(
    delete,
    path = "/auth/handoffs/{handoff_id}",
    operation_id = "auth_handoff_cancel",
    tag = "accounts",
    responses(
        (status = 204),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn cancel_auth_handoff(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(handoff_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    if !qm_db::auth_handoff::cancel(&state.db, handoff_id, current.user_id, current.session_id)
        .await?
    {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/auth/handoffs/preview",
    operation_id = "auth_handoff_preview",
    tag = "accounts",
    request_body = AuthHandoffTokenRequest,
    responses(
        (status = 200, body = AuthHandoffPreviewResponse),
        (status = 400, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn preview_auth_handoff(
    State(state): State<AppState>,
    Json(req): Json<AuthHandoffTokenRequest>,
) -> ApiResult<Json<AuthHandoffPreviewResponse>> {
    let handoff = valid_handoff_for_token(&state, req.id, &req.token).await?;
    let user = qm_db::users::find_by_id(&state.db, handoff.user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    Ok(Json(AuthHandoffPreviewResponse {
        id: handoff.id,
        source_email: user.email,
        source_display_name: user.display_name,
        household_id: handoff.active_household_id,
        target_device_label: handoff.target_device_label,
        expires_at: handoff.expires_at,
    }))
}

#[utoipa::path(
    post,
    path = "/auth/handoffs/accept",
    operation_id = "auth_handoff_accept",
    tag = "accounts",
    request_body = AuthHandoffAcceptRequest,
    responses(
        (status = 200, body = TokenPair),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn accept_auth_handoff(
    State(state): State<AppState>,
    Json(req): Json<AuthHandoffAcceptRequest>,
) -> ApiResult<(HeaderMap, Json<TokenPair>)> {
    let session_id = Uuid::now_v7();
    let token_hash = auth::sha256_hex(&req.token);
    let handoff =
        qm_db::auth_handoff::consume(&state.db, req.id, &token_hash, session_id, Timestamp::now())
            .await?
            .ok_or_else(|| ApiError::BadRequest("handoff token is invalid or expired".into()))?;
    if qm_db::auth_sessions::find(&state.db, handoff.source_session_id)
        .await?
        .is_none()
    {
        return Err(ApiError::Unauthorized);
    }
    let pair = issue_token_pair(
        &state,
        handoff.user_id,
        session_id,
        req.device_label
            .as_deref()
            .or(handoff.target_device_label.as_deref()),
        handoff.active_household_id,
    )
    .await?;
    let headers = session_cookie_headers(&state, &pair);
    Ok((headers, Json(pair)))
}

#[utoipa::path(
    post,
    path = "/auth/login",
    operation_id = "auth_login",
    tag = "accounts",
    request_body = LoginRequest,
    responses(
        (status = 200, body = TokenPair),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<(HeaderMap, Json<TokenPair>)> {
    let email = validate_email(&req.email)?;
    let user = qm_db::users::find_by_email(&state.db, &email)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !auth::verify_password(&req.password, &user.password_hash) {
        return Err(ApiError::Unauthorized);
    }
    let initial_household_id = qm_db::households::find_for_user(&state.db, user.id)
        .await?
        .map(|household| household.id);
    let pair = issue_token_pair(
        &state,
        user.id,
        Uuid::now_v7(),
        req.device_label.as_deref(),
        initial_household_id,
    )
    .await?;
    let headers = session_cookie_headers(&state, &pair);
    Ok((headers, Json(pair)))
}

#[utoipa::path(
    post,
    path = "/auth/refresh",
    operation_id = "auth_refresh",
    tag = "accounts",
    request_body = RefreshRequest,
    responses(
        (status = 200, body = TokenPair),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: Option<Json<RefreshRequest>>,
) -> ApiResult<(HeaderMap, Json<TokenPair>)> {
    let body_refresh_token = req
        .as_ref()
        .and_then(|Json(req)| req.refresh_token.as_deref())
        .map(str::to_owned);
    let cookie_refresh_token = body_refresh_token
        .is_none()
        .then(|| auth::cookie_value(&headers, auth::REFRESH_COOKIE))
        .flatten();
    if cookie_refresh_token.is_some() {
        let cookie_csrf = auth::cookie_value(&headers, auth::CSRF_COOKIE);
        let header_csrf = headers.get(auth::CSRF_HEADER).and_then(|v| v.to_str().ok());
        if cookie_csrf.as_deref().is_none() || cookie_csrf.as_deref() != header_csrf {
            return Err(ApiError::Forbidden);
        }
    }
    let refresh_token = body_refresh_token
        .or(cookie_refresh_token)
        .ok_or(ApiError::Unauthorized)?;
    let hash = auth::sha256_hex(&refresh_token);
    let token = qm_db::tokens::find_active_by_hash(&state.db, &hash)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if token.kind != qm_db::tokens::KIND_REFRESH {
        auth::cleanup_session_if_unused(&state.db, token.session_id).await?;
        return Err(ApiError::Unauthorized);
    }
    let expires: Timestamp = token
        .expires_at
        .parse()
        .map_err(|_| ApiError::Unauthorized)?;
    if expires < Timestamp::now() {
        auth::cleanup_session_if_unused(&state.db, token.session_id).await?;
        return Err(ApiError::Unauthorized);
    }

    // Rotate: revoke the presented refresh token, mint a fresh pair.
    qm_db::tokens::revoke(&state.db, token.id).await?;
    let pair = issue_token_pair(
        &state,
        token.user_id,
        token.session_id,
        token.device_label.as_deref(),
        auth::resolve_active_household(&state.db, token.session_id, token.user_id)
            .await?
            .household_id,
    )
    .await?;
    auth::cleanup_session_if_unused(&state.db, token.session_id).await?;
    let headers = session_cookie_headers(&state, &pair);
    Ok((headers, Json(pair)))
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    operation_id = "auth_logout",
    tag = "accounts",
    responses((status = 204), (status = 401, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn logout(
    State(state): State<AppState>,
    current: CurrentUser,
    header: axum::http::HeaderMap,
) -> ApiResult<(HeaderMap, StatusCode)> {
    let session_id = current.session_id;
    if let Some(bearer) = header
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        let hash = auth::sha256_hex(bearer);
        if let Some(token) = qm_db::tokens::find_active_by_hash(&state.db, &hash).await? {
            qm_db::tokens::revoke_session(&state.db, token.session_id).await?;
        }
    }
    qm_db::tokens::revoke_session(&state.db, session_id).await?;
    qm_db::auth_sessions::delete(&state.db, session_id).await?;
    Ok((clear_session_cookie_headers(&state), StatusCode::NO_CONTENT))
}

#[utoipa::path(
    get,
    path = "/auth/me",
    operation_id = "auth_me",
    tag = "accounts",
    responses((status = 200, body = MeResponse), (status = 401, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn me(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<MeResponse>> {
    Ok(Json(
        build_me_response(&state, current.user_id, current.household_id).await?,
    ))
}

#[utoipa::path(
    post,
    path = "/auth/email-verification",
    operation_id = "auth_email_verification_request",
    tag = "accounts",
    request_body = RequestEmailVerificationRequest,
    responses(
        (status = 200, body = RequestEmailVerificationResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn request_email_verification(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<RequestEmailVerificationRequest>,
) -> ApiResult<Json<RequestEmailVerificationResponse>> {
    let email = validate_email(&req.email)?;
    let Some(email_transport) = state.email_transport.as_ref() else {
        return Err(ApiError::ServiceUnavailable(
            "email delivery is not configured".into(),
        ));
    };
    let code = auth::generate_human_code(10);
    let code_hash = auth::sha256_hex(&code);
    let expires_at = Timestamp::now()
        .checked_add(SignedDuration::from_mins(30))
        .map_err(|e| {
            ApiError::Internal(anyhow::anyhow!("email verification expiry overflow: {e}"))
        })?;
    let expires_at = qm_db::time::format_timestamp(expires_at);
    let pending = qm_db::users::create_email_verification(
        &state.db,
        current.user_id,
        &email,
        &code_hash,
        &expires_at,
    )
    .await?;

    email_transport
        .send(recovery_verification_email(&email, &code, &expires_at))
        .await
        .map_err(|err| {
            tracing::warn!(
                user_id = %current.user_id,
                target_email = %email,
                error = ?err.source(),
                "recovery email verification delivery failed"
            );
            ApiError::ServiceUnavailable("email delivery failed".into())
        })?;

    Ok(Json(RequestEmailVerificationResponse {
        pending_email: pending.email,
        expires_at: pending.expires_at,
    }))
}

#[utoipa::path(
    post,
    path = "/auth/password-reset/request",
    operation_id = "auth_password_reset_request",
    tag = "accounts",
    request_body = PasswordResetRequest,
    responses(
        (status = 202, body = PasswordResetRequestResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn request_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetRequest>,
) -> ApiResult<(StatusCode, Json<PasswordResetRequestResponse>)> {
    let email = validate_email(&req.email)?;
    if let Some(user) = qm_db::users::find_by_email(&state.db, &email).await? {
        if let (Some(email), Some(_verified_at), Some(email_transport)) = (
            Some(user.email.as_str()),
            user.email_verified_at.as_deref(),
            state.email_transport.as_ref(),
        ) {
            let code = auth::generate_human_code(10);
            let token = auth::generate_token();
            let expires_at = Timestamp::now()
                .checked_add(SignedDuration::from_mins(30))
                .map_err(|e| {
                    ApiError::Internal(anyhow::anyhow!("password reset expiry overflow: {e}"))
                })?;
            let expires_at = qm_db::time::format_timestamp(expires_at);
            qm_db::users::create_password_reset(
                &state.db,
                user.id,
                &auth::sha256_hex(&code),
                &auth::sha256_hex(&token),
                &expires_at,
            )
            .await?;
            if let Err(err) = email_transport
                .send(password_reset_email(
                    email,
                    &code,
                    &token,
                    state.config.public_base_url.as_deref(),
                    &expires_at,
                ))
                .await
            {
                tracing::warn!(
                    user_id = %user.id,
                    target_email = %email,
                    error = ?err.source(),
                    "password reset email delivery failed"
                );
            }
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(PasswordResetRequestResponse { status: "accepted" }),
    ))
}

#[utoipa::path(
    post,
    path = "/auth/password-reset/confirm",
    operation_id = "auth_password_reset_confirm",
    tag = "accounts",
    request_body = PasswordResetConfirmRequest,
    responses(
        (status = 204),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
)]
pub async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetConfirmRequest>,
) -> ApiResult<StatusCode> {
    let email = validate_email(&req.email)?;
    validate_password(&req.new_password)?;
    let code = req.code.as_deref().map(normalize_reset_code).transpose()?;
    let token = req.token.as_deref().map(validate_reset_token).transpose()?;
    if code.is_none() && token.is_none() {
        return Err(ApiError::BadRequest(
            "code or token is required to reset password".into(),
        ));
    }
    let password_hash = auth::hash_password(&req.new_password)?;
    let now = qm_db::now_utc_rfc3339();
    let code_hash = code.as_deref().map(auth::sha256_hex);
    let token_hash = token.as_deref().map(auth::sha256_hex);
    let updated = qm_db::users::reset_password_by_code_or_token(
        &state.db,
        &email,
        code_hash.as_deref(),
        token_hash.as_deref(),
        &password_hash,
        &now,
    )
    .await?;
    if updated.is_none() {
        return Err(ApiError::BadRequest(
            "password reset code or token is invalid or expired".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/auth/email-verification/confirm",
    operation_id = "auth_email_verification_confirm",
    tag = "accounts",
    request_body = ConfirmEmailVerificationRequest,
    responses(
        (status = 200, body = MeResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn confirm_email_verification(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<ConfirmEmailVerificationRequest>,
) -> ApiResult<Json<MeResponse>> {
    let code = req
        .code
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .collect::<String>()
        .to_ascii_uppercase();
    if code.is_empty() || code.len() > 32 {
        return Err(ApiError::BadRequest("verification code is invalid".into()));
    }
    let now = qm_db::now_utc_rfc3339();
    let Some(_) = qm_db::users::confirm_email_verification(
        &state.db,
        current.user_id,
        &auth::sha256_hex(&code),
        &now,
    )
    .await?
    else {
        return Err(ApiError::BadRequest(
            "verification code is invalid or expired".into(),
        ));
    };

    Ok(Json(
        build_me_response(&state, current.user_id, current.household_id).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/auth/email",
    operation_id = "auth_email_clear",
    tag = "accounts",
    responses(
        (status = 200, body = MeResponse),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn clear_recovery_email(
    State(_state): State<AppState>,
    _current: CurrentUser,
) -> ApiResult<Json<MeResponse>> {
    Err(ApiError::BadRequest(
        "account email is required and cannot be cleared".into(),
    ))
}

#[utoipa::path(
    post,
    path = "/auth/switch-household",
    operation_id = "auth_switch_household",
    tag = "accounts",
    request_body = SwitchHouseholdRequest,
    responses(
        (status = 200, body = MeResponse),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody)
    ),
    security(("bearer" = [])),
)]
pub async fn switch_household(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<SwitchHouseholdRequest>,
) -> ApiResult<Json<MeResponse>> {
    if qm_db::memberships::find(&state.db, req.household_id, current.user_id)
        .await?
        .is_none()
    {
        return Err(ApiError::Forbidden);
    }

    qm_db::auth_sessions::upsert(
        &state.db,
        current.session_id,
        current.user_id,
        Some(req.household_id),
    )
    .await?;

    Ok(Json(
        build_me_response(&state, current.user_id, Some(req.household_id)).await?,
    ))
}

pub(crate) fn validate_credentials(email: &str, password: &str) -> ApiResult<()> {
    validate_email(email)?;
    validate_password(password)
}

fn validate_password(password: &str) -> ApiResult<()> {
    if password.len() < 8 {
        return Err(ApiError::BadRequest(
            "password must be at least 8 chars".into(),
        ));
    }
    if password.len() > 256 {
        return Err(ApiError::BadRequest("password too long".into()));
    }
    Ok(())
}

fn normalize_reset_code(code: &str) -> ApiResult<String> {
    let code = code
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-')
        .collect::<String>()
        .to_ascii_uppercase();
    if code.is_empty() || code.len() > 32 {
        return Err(ApiError::BadRequest("reset code is invalid".into()));
    }
    Ok(code)
}

fn validate_reset_token(token: &str) -> ApiResult<String> {
    let token = token.trim();
    if token.is_empty() || token.len() > 512 {
        return Err(ApiError::BadRequest("reset token is invalid".into()));
    }
    Ok(token.to_owned())
}

pub(crate) fn validate_email(value: &str) -> ApiResult<String> {
    let email = value.trim().to_ascii_lowercase();
    if email.is_empty() || email.len() > 254 {
        return Err(ApiError::BadRequest("email must be 1..=254 chars".into()));
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err(ApiError::BadRequest("email is invalid".into()));
    }
    Ok(email)
}

pub(crate) fn validate_display_name(value: &str) -> ApiResult<&str> {
    let display_name = value.trim();
    if display_name.is_empty() || display_name.len() > 128 {
        return Err(ApiError::BadRequest(
            "display_name must be 1..=128 chars".into(),
        ));
    }
    Ok(display_name)
}

fn recovery_verification_email(email: &str, code: &str, expires_at: &str) -> EmailMessage {
    EmailMessage {
        to: EmailAddress::new(email, None),
        subject: "Your Quartermaster recovery email code".into(),
        text_body: format!(
            "Use this code to verify your Quartermaster recovery email:\n\n{code}\n\nThis code expires at {expires_at}."
        ),
    }
}

pub(crate) async fn signup_verification(
    state: &AppState,
    _email: &str,
) -> ApiResult<SignupVerification> {
    if state.email_transport.is_none() {
        return Err(ApiError::ServiceUnavailable(
            "email delivery is not configured".into(),
        ));
    }
    let code = auth::generate_human_code(10);
    let expires_at = Timestamp::now()
        .checked_add(SignedDuration::from_mins(30))
        .map_err(|e| {
            ApiError::Internal(anyhow::anyhow!("email verification expiry overflow: {e}"))
        })?;
    let expires_at = qm_db::time::format_timestamp(expires_at);
    Ok(SignupVerification {
        code_hash: auth::sha256_hex(&code),
        code,
        expires_at,
    })
}

pub(crate) async fn create_user_with_pending_verification(
    state: &AppState,
    email: &str,
    display_name: &str,
    password_hash: &str,
    verification: &SignupVerification,
) -> ApiResult<qm_db::users::UserRow> {
    let mut tx = state.db.pool.begin().await?;
    let user = match qm_db::users::create_in_tx(
        &mut tx,
        state.db.backend(),
        email,
        display_name,
        password_hash,
    )
    .await
    {
        Ok(user) => user,
        Err(err) if qm_db::memberships::is_unique_violation(&err) => {
            return Err(ApiError::Conflict("email already registered".into()));
        }
        Err(err) => return Err(ApiError::Database(err)),
    };
    qm_db::users::create_email_verification_in_tx(
        &mut tx,
        state.db.backend(),
        user.id,
        email,
        &verification.code_hash,
        &verification.expires_at,
    )
    .await?;
    send_signup_verification(state, email, &verification.code, &verification.expires_at).await?;
    tx.commit().await?;
    Ok(user)
}

pub(crate) async fn send_signup_verification(
    state: &AppState,
    email: &str,
    code: &str,
    expires_at: &str,
) -> ApiResult<()> {
    let Some(email_transport) = state.email_transport.as_ref() else {
        return Err(ApiError::ServiceUnavailable(
            "email delivery is not configured".into(),
        ));
    };
    email_transport
        .send(recovery_verification_email(email, code, expires_at))
        .await
        .map_err(|err| {
            tracing::warn!(
                target_email = %email,
                error = ?err.source(),
                "signup email verification delivery failed"
            );
            ApiError::ServiceUnavailable("email delivery failed".into())
        })
}

pub(crate) fn passkeys_available(state: &AppState) -> bool {
    state.config.passkeys.enabled
        && state.config.passkeys.rp_id.is_some()
        && state.config.passkeys.origin.is_some()
}

fn passkey_webauthn(state: &AppState) -> ApiResult<Webauthn> {
    if !passkeys_available(state) {
        return Err(ApiError::ServiceUnavailable(
            "passkeys are not configured".into(),
        ));
    }
    let rp_id = state
        .config
        .passkeys
        .rp_id
        .as_deref()
        .ok_or_else(|| ApiError::ServiceUnavailable("passkeys are not configured".into()))?;
    let origin = state
        .config
        .passkeys
        .origin
        .as_deref()
        .ok_or_else(|| ApiError::ServiceUnavailable("passkeys are not configured".into()))?;
    let origin = Url::parse(origin).map_err(|err| {
        ApiError::Internal(anyhow::anyhow!(
            "invalid passkey origin configuration: {err}"
        ))
    })?;
    WebauthnBuilder::new(rp_id, &origin)
        .map_err(passkey_error)?
        .rp_name(&state.config.passkeys.rp_name)
        .build()
        .map_err(passkey_error)
}

fn passkey_error(err: webauthn_rs::prelude::WebauthnError) -> ApiError {
    ApiError::BadRequest(format!("passkey ceremony failed: {err}"))
}

fn json_error(err: serde_json::Error) -> ApiError {
    ApiError::Internal(anyhow::anyhow!(err))
}

fn passkey_from_json(value: &str) -> ApiResult<Passkey> {
    serde_json::from_str(value).map_err(|err| {
        ApiError::Internal(anyhow::anyhow!(
            "stored passkey credential is invalid: {err}"
        ))
    })
}

fn passkey_credential_id(passkey: &Passkey) -> ApiResult<String> {
    serde_json::to_string(passkey.cred_id())
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("serializing credential id: {err}")))
}

fn passkey_credential_dto(row: qm_db::passkeys::PasskeyCredentialRow) -> PasskeyCredentialDto {
    PasskeyCredentialDto {
        id: row.id,
        label: row.label,
        created_at: row.created_at,
        last_used_at: row.last_used_at,
    }
}

fn handoff_server_url(state: &AppState, requested: Option<&str>) -> ApiResult<String> {
    let raw = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| state.config.public_base_url.clone())
        .ok_or_else(|| {
            ApiError::BadRequest("server_url is required when QM_PUBLIC_BASE_URL is unset".into())
        })?;
    let url = reqwest::Url::parse(&raw)
        .map_err(|_| ApiError::BadRequest("server_url is invalid".into()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::BadRequest(
            "server_url must use http or https".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ApiError::BadRequest(
            "server_url must not include user info".into(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
        return Err(ApiError::BadRequest(
            "server_url must be an origin without path, query, or fragment".into(),
        ));
    }
    Ok(url.origin().ascii_serialization())
}

async fn valid_handoff_for_token(
    state: &AppState,
    id: Uuid,
    token: &str,
) -> ApiResult<qm_db::auth_handoff::AuthHandoffRow> {
    let token = token.trim();
    if token.is_empty() || token.len() > 512 {
        return Err(ApiError::BadRequest("handoff token is invalid".into()));
    }
    let token_hash = auth::sha256_hex(token);
    let handoff = qm_db::auth_handoff::find_by_token_hash(&state.db, id, &token_hash)
        .await?
        .ok_or_else(|| ApiError::BadRequest("handoff token is invalid or expired".into()))?;
    if handoff.consumed_at.is_some() || handoff.cancelled_at.is_some() {
        return Err(ApiError::BadRequest(
            "handoff token is invalid or expired".into(),
        ));
    }
    let expires_at = qm_db::time::parse_timestamp(&handoff.expires_at)
        .map_err(|_| ApiError::BadRequest("handoff token is invalid or expired".into()))?;
    if expires_at <= Timestamp::now() {
        return Err(ApiError::BadRequest(
            "handoff token is invalid or expired".into(),
        ));
    }
    if qm_db::auth_sessions::find(&state.db, handoff.source_session_id)
        .await?
        .is_none()
    {
        return Err(ApiError::Unauthorized);
    }
    Ok(handoff)
}

fn password_reset_email(
    email: &str,
    code: &str,
    token: &str,
    public_base_url: Option<&str>,
    expires_at: &str,
) -> EmailMessage {
    let mut body = format!(
        "A Quartermaster password reset was requested for {email}.\n\nUse this code to reset your password:\n\n{code}\n\nThis reset expires at {expires_at}."
    );
    if let Some(base_url) = public_base_url {
        let base = base_url.trim_end_matches('/');
        let email = utf8_percent_encode(email, NON_ALPHANUMERIC);
        let token = utf8_percent_encode(token, NON_ALPHANUMERIC);
        body.push_str(&format!(
            "\n\nYou can also open this reset link:\n{base}/reset-password?email={email}&token={token}"
        ));
    }
    EmailMessage {
        to: EmailAddress::new(email, None),
        subject: "Reset your Quartermaster password".into(),
        text_body: body,
    }
}

pub(crate) async fn issue_token_pair(
    state: &AppState,
    user_id: Uuid,
    session_id: Uuid,
    device_label: Option<&str>,
    active_household_id: Option<Uuid>,
) -> ApiResult<TokenPair> {
    let access = auth::generate_token();
    let refresh = auth::generate_token();
    let now = Timestamp::now();
    let access_expires = now
        .checked_add(SignedDuration::from_secs(
            state.config.access_token_ttl_seconds,
        ))
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("access expiry overflow: {e}")))?;
    let refresh_expires = now
        .checked_add(SignedDuration::from_secs(
            state.config.refresh_token_ttl_seconds,
        ))
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("refresh expiry overflow: {e}")))?;

    qm_db::auth_sessions::upsert(&state.db, session_id, user_id, active_household_id).await?;

    qm_db::tokens::create(
        &state.db,
        user_id,
        session_id,
        &auth::sha256_hex(&access),
        qm_db::tokens::KIND_ACCESS,
        device_label,
        access_expires,
    )
    .await?;
    qm_db::tokens::create(
        &state.db,
        user_id,
        session_id,
        &auth::sha256_hex(&refresh),
        qm_db::tokens::KIND_REFRESH,
        device_label,
        refresh_expires,
    )
    .await?;

    Ok(TokenPair {
        access_token: access,
        refresh_token: refresh,
        token_type: "Bearer",
        expires_in: state.config.access_token_ttl_seconds,
    })
}

pub(crate) fn session_cookie_headers(state: &AppState, pair: &TokenPair) -> HeaderMap {
    let csrf = auth::generate_token();
    let mut headers = HeaderMap::new();
    append_cookie(
        &mut headers,
        build_cookie(
            state,
            auth::ACCESS_COOKIE,
            &pair.access_token,
            true,
            Some(pair.expires_in),
        ),
    );
    append_cookie(
        &mut headers,
        build_cookie(
            state,
            auth::REFRESH_COOKIE,
            &pair.refresh_token,
            true,
            Some(state.config.refresh_token_ttl_seconds),
        ),
    );
    append_cookie(
        &mut headers,
        build_cookie(
            state,
            auth::CSRF_COOKIE,
            &csrf,
            false,
            Some(state.config.refresh_token_ttl_seconds),
        ),
    );
    headers
}

fn clear_session_cookie_headers(state: &AppState) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in [auth::ACCESS_COOKIE, auth::REFRESH_COOKIE, auth::CSRF_COOKIE] {
        append_cookie(
            &mut headers,
            build_cookie(state, name, "", name != auth::CSRF_COOKIE, Some(0)),
        );
    }
    headers
}

fn append_cookie(headers: &mut HeaderMap, value: String) {
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&value).expect("cookie value is ASCII"),
    );
}

fn build_cookie(
    state: &AppState,
    name: &str,
    value: &str,
    http_only: bool,
    max_age_seconds: Option<i64>,
) -> String {
    let mut cookie = format!(
        "{name}={value}; Path=/; SameSite={}",
        cookie_same_site(state)
    );
    if let Some(max_age_seconds) = max_age_seconds {
        cookie.push_str(&format!("; Max-Age={max_age_seconds}"));
    }
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if cookie_secure(state) {
        cookie.push_str("; Secure");
    }
    cookie
}

fn cookie_same_site(state: &AppState) -> &'static str {
    if state.config.web_auth_allowed_origins.is_empty() {
        "Lax"
    } else {
        "None"
    }
}

fn cookie_secure(state: &AppState) -> bool {
    !state.config.web_auth_allowed_origins.is_empty()
}

pub(crate) async fn build_me_response(
    state: &AppState,
    user_id: Uuid,
    active_household_id: Option<Uuid>,
) -> ApiResult<MeResponse> {
    let user = qm_db::users::find_by_id(&state.db, user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let pending_email = qm_db::users::latest_pending_email_verification(
        &state.db,
        user_id,
        &qm_db::now_utc_rfc3339(),
    )
    .await?;
    let memberships = qm_db::memberships::list_for_user(&state.db, user_id).await?;
    let households = memberships
        .iter()
        .map(|row| {
            Ok::<_, ApiError>(HouseholdSummaryDto {
                id: row.membership.household_id,
                name: row.household_name.clone(),
                timezone: row.household_timezone.clone(),
                measurement_system: crate::routes::households::measurement_system_from_db(
                    &row.household_measurement_system,
                )?,
                role: crate::types::MembershipRole::from_str(&row.membership.role)?,
                joined_at: row.membership.joined_at.clone(),
            })
        })
        .collect::<ApiResult<Vec<_>>>()?;
    let current_household = households
        .iter()
        .find(|row| Some(row.id) == active_household_id)
        .cloned();

    Ok(MeResponse {
        user: UserDto {
            id: user.id,
            email: user.email,
            display_name: user.display_name,
            email_verified_at: user.email_verified_at,
            pending_email: pending_email.as_ref().map(|pending| pending.email.clone()),
            pending_email_verification_expires_at: pending_email.map(|pending| pending.expires_at),
        },
        current_household,
        households,
        public_base_url: state.config.public_base_url.clone(),
    })
}

pub(crate) async fn load_openfoodfacts_credentials(
    state: &AppState,
    user_id: Uuid,
) -> ApiResult<OpenFoodFactsPlainCredentials> {
    let row = qm_db::off_credentials::get(&state.db, user_id)
        .await?
        .ok_or(ApiError::OffCredentialsMissing)?;
    let password = decrypt_off_password(state, user_id, &row.encrypted_password)?;
    Ok(OpenFoodFactsPlainCredentials {
        username: row.off_username,
        password,
    })
}

fn encryption_key(state: &AppState) -> ApiResult<[u8; 32]> {
    let secret = state
        .config
        .off_credential_encryption_key
        .as_deref()
        .ok_or(ApiError::OffCredentialsNotConfigured)?;
    let digest = Sha256::digest(secret.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    Ok(key)
}

fn encrypt_off_password(state: &AppState, user_id: Uuid, password: &str) -> ApiResult<String> {
    let key = encryption_key(state)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("OFF cipher init: {err}")))?;
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            aes_gcm::aead::Payload {
                msg: password.as_bytes(),
                aad: user_id.as_bytes(),
            },
        )
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("OFF credential encrypt: {err}")))?;
    let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(STANDARD_NO_PAD.encode(payload))
}

fn decrypt_off_password(state: &AppState, user_id: Uuid, encrypted: &str) -> ApiResult<String> {
    let key = encryption_key(state)?;
    let payload = STANDARD_NO_PAD
        .decode(encrypted)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("OFF credential decode: {err}")))?;
    if payload.len() <= 12 {
        return Err(ApiError::Internal(anyhow::anyhow!(
            "OFF credential payload is too short"
        )));
    }
    let (nonce, ciphertext) = payload.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("OFF cipher init: {err}")))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad: user_id.as_bytes(),
            },
        )
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("OFF credential decrypt: {err}")))?;
    String::from_utf8(plaintext)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("OFF credential utf8: {err}")))
}
