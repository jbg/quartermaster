use std::str::FromStr;

use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware,
    routing::{get, post},
    Json, Router,
};
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use utoipa::{
    openapi::{
        schema::{AllOfBuilder, ArrayBuilder, ObjectBuilder, Schema, SchemaType, Type},
        Ref, RefOr,
    },
    PartialSchema, ToSchema,
};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    rate_limit::RateLimitLayerState,
    AppState, RegistrationMode,
};

const ROLE_ADMIN: &str = "admin";

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
                rate_limit_state,
                crate::rate_limit::enforce,
            )),
        )
        .route("/auth/logout", post(logout))
        .route("/auth/switch-household", post(switch_household))
        .route("/auth/me", get(me))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
    /// Required unless the server is in `first_run_only` mode and no users
    /// exist yet, or in `open` mode.
    pub invite_code: Option<String>,
    /// Optional label applied to the refresh token (shown on `/api/v1/auth/me`).
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
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
    pub username: String,
    pub email: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HouseholdDto {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct HouseholdSummaryDto {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
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
    validate_credentials(&req.username, &req.password)?;

    let existing_count = qm_db::users::count(&state.db).await?;
    let password_hash = auth::hash_password(&req.password)?;

    let user = match (state.config.registration_mode, existing_count) {
        (RegistrationMode::FirstRunOnly, 0) => {
            let h = qm_db::households::create(&state.db, "My Household", "UTC").await?;
            qm_db::locations::seed_defaults(&state.db, h.id).await?;
            if qm_db::users::find_by_username(&state.db, &req.username)
                .await?
                .is_some()
            {
                return Err(ApiError::Conflict("username already taken".into()));
            }
            let user = qm_db::users::create(
                &state.db,
                &req.username,
                req.email.as_deref(),
                &password_hash,
            )
            .await?;
            qm_db::memberships::insert(&state.db, h.id, user.id, ROLE_ADMIN).await?;
            user
        }
        (RegistrationMode::FirstRunOnly, _) => {
            return Err(ApiError::RegistrationDisabled);
        }
        (RegistrationMode::Open, _) => {
            let h = qm_db::households::create(
                &state.db,
                &format!("{}'s Household", req.username),
                "UTC",
            )
            .await?;
            qm_db::locations::seed_defaults(&state.db, h.id).await?;
            if qm_db::users::find_by_username(&state.db, &req.username)
                .await?
                .is_some()
            {
                return Err(ApiError::Conflict("username already taken".into()));
            }
            let user = qm_db::users::create(
                &state.db,
                &req.username,
                req.email.as_deref(),
                &password_hash,
            )
            .await?;
            qm_db::memberships::insert(&state.db, h.id, user.id, ROLE_ADMIN).await?;
            user
        }
        (RegistrationMode::InviteOnly, _) => {
            let code = req
                .invite_code
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ApiError::BadRequest("invite_code is required".into()))?
                .to_ascii_uppercase();
            match qm_db::invites::register_user_with_invite(
                &state.db,
                &code,
                &req.username,
                req.email.as_deref(),
                &password_hash,
            )
            .await
            {
                Ok(registered) => registered.user,
                Err(qm_db::invites::RegisterWithInviteError::InvalidInvite) => {
                    return Err(ApiError::InvalidInvite);
                }
                Err(qm_db::invites::RegisterWithInviteError::UsernameTaken) => {
                    return Err(ApiError::Conflict("username already taken".into()));
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
    let user = qm_db::users::find_by_username(&state.db, &req.username)
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

fn validate_credentials(username: &str, password: &str) -> ApiResult<()> {
    if username.trim().is_empty() || username.len() > 64 {
        return Err(ApiError::BadRequest("username must be 1..=64 chars".into()));
    }
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

async fn issue_token_pair(
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

fn session_cookie_headers(state: &AppState, pair: &TokenPair) -> HeaderMap {
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
    let path = if name == auth::CSRF_COOKIE {
        "/"
    } else {
        "/api/v1"
    };
    let mut cookie = format!(
        "{name}={value}; Path={path}; SameSite={}",
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
    !state.config.web_auth_allowed_origins.is_empty() || state.config.public_base_url.is_some()
}

pub(crate) async fn build_me_response(
    state: &AppState,
    user_id: Uuid,
    active_household_id: Option<Uuid>,
) -> ApiResult<MeResponse> {
    let user = qm_db::users::find_by_id(&state.db, user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let memberships = qm_db::memberships::list_for_user(&state.db, user_id).await?;
    let households = memberships
        .iter()
        .map(|row| {
            Ok::<_, ApiError>(HouseholdSummaryDto {
                id: row.membership.household_id,
                name: row.household_name.clone(),
                timezone: row.household_timezone.clone(),
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
            username: user.username,
            email: user.email,
        },
        current_household,
        households,
        public_base_url: state.config.public_base_url.clone(),
    })
}
