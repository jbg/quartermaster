use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    AppState, RegistrationMode,
};

const ROLE_ADMIN: &str = "admin";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
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
    /// Optional label applied to the refresh token (shown on `/auth/me`).
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
    pub refresh_token: String,
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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub user: UserDto,
    pub household_id: Option<Uuid>,
    pub household_name: Option<String>,
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
    ),
)]
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<TokenPair>)> {
    validate_credentials(&req.username, &req.password)?;

    let existing_count = qm_db::users::count(&state.db).await?;
    let password_hash = auth::hash_password(&req.password)?;

    let user = match (state.config.registration_mode, existing_count) {
        (RegistrationMode::FirstRunOnly, 0) => {
            let h = qm_db::households::create(&state.db, "My Household").await?;
            qm_db::locations::seed_defaults(&state.db, h.id).await?;
            if qm_db::users::find_by_username(&state.db, &req.username).await?.is_some() {
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
            )
            .await?;
            qm_db::locations::seed_defaults(&state.db, h.id).await?;
            if qm_db::users::find_by_username(&state.db, &req.username).await?.is_some() {
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

    let pair = issue_token_pair(&state, user.id, Uuid::now_v7(), req.device_label.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(pair)))
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
    ),
)]
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<TokenPair>> {
    let user = qm_db::users::find_by_username(&state.db, &req.username)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !auth::verify_password(&req.password, &user.password_hash) {
        return Err(ApiError::Unauthorized);
    }
    let pair = issue_token_pair(&state, user.id, Uuid::now_v7(), req.device_label.as_deref()).await?;
    Ok(Json(pair))
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
    ),
)]
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> ApiResult<Json<TokenPair>> {
    let hash = auth::sha256_hex(&req.refresh_token);
    let token = qm_db::tokens::find_active_by_hash(&state.db, &hash)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if token.kind != qm_db::tokens::KIND_REFRESH {
        return Err(ApiError::Unauthorized);
    }
    let expires = chrono::DateTime::parse_from_rfc3339(&token.expires_at)
        .map_err(|_| ApiError::Unauthorized)?
        .with_timezone(&Utc);
    if expires < Utc::now() {
        return Err(ApiError::Unauthorized);
    }

    // Rotate: revoke the presented refresh token, mint a fresh pair.
    qm_db::tokens::revoke(&state.db, token.id).await?;
    let pair = issue_token_pair(
        &state,
        token.user_id,
        token.session_id,
        token.device_label.as_deref(),
    )
    .await?;
    Ok(Json(pair))
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
) -> ApiResult<StatusCode> {
    let _ = current; // presence enforces auth
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
    Ok(StatusCode::NO_CONTENT)
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
    let user = qm_db::users::find_by_id(&state.db, current.user_id)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let household = if let Some(hid) = current.household_id {
        qm_db::households::find_for_user(&state.db, current.user_id)
            .await?
            .filter(|h| h.id == hid)
    } else {
        None
    };
    let (household_id, household_name) = match household {
        Some(h) => (Some(h.id), Some(h.name)),
        None => (None, None),
    };
    Ok(Json(MeResponse {
        user: UserDto {
            id: user.id,
            username: user.username,
            email: user.email,
        },
        household_id,
        household_name,
    }))
}

fn validate_credentials(username: &str, password: &str) -> ApiResult<()> {
    if username.trim().is_empty() || username.len() > 64 {
        return Err(ApiError::BadRequest("username must be 1..=64 chars".into()));
    }
    if password.len() < 8 {
        return Err(ApiError::BadRequest("password must be at least 8 chars".into()));
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
) -> ApiResult<TokenPair> {
    let access = auth::generate_token();
    let refresh = auth::generate_token();
    let now = Utc::now();
    let access_expires = now + Duration::seconds(state.config.access_token_ttl_seconds);
    let refresh_expires = now + Duration::seconds(state.config.refresh_token_ttl_seconds);

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
