use std::str::FromStr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    routes::accounts::UserDto,
    types::MembershipRole,
    AppState,
};

const ROLE_ADMIN: &str = "admin";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/households/current", get(get_current_household).patch(update_current_household))
        .route("/households/current/members", get(list_members))
        .route("/households/current/members/{user_id}", delete(remove_member))
        .route("/households/current/invites", get(list_invites).post(create_invite))
        .route("/invites/{id}", delete(revoke_invite))
        .route("/invites/redeem", post(redeem_invite))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HouseholdDetailDto {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateHouseholdRequest {
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberDto {
    pub user: UserDto,
    pub role: MembershipRole,
    pub joined_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InviteDto {
    pub id: Uuid,
    pub code: String,
    pub role_granted: MembershipRole,
    pub expires_at: String,
    pub max_uses: i64,
    pub use_count: i64,
    pub created_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInviteRequest {
    pub expires_at: String,
    pub max_uses: i64,
    pub role_granted: MembershipRole,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RedeemInviteRequest {
    pub invite_code: String,
}

#[utoipa::path(
    get,
    path = "/households/current",
    operation_id = "household_current_get",
    tag = "households",
    responses((status = 200, body = HouseholdDetailDto), (status = 401, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_current_household(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<HouseholdDetailDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let household = qm_db::households::find_by_id(&state.db, household_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(HouseholdDetailDto {
        id: household.id,
        name: household.name,
    }))
}

#[utoipa::path(
    patch,
    path = "/households/current",
    operation_id = "household_current_update",
    tag = "households",
    request_body = UpdateHouseholdRequest,
    responses((status = 200, body = HouseholdDetailDto), (status = 403, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn update_current_household(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<UpdateHouseholdRequest>,
) -> ApiResult<Json<HouseholdDetailDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    require_admin(&current)?;
    let name = req.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(ApiError::BadRequest("household name must be 1..=128 chars".into()));
    }
    let household = qm_db::households::rename(&state.db, household_id, name)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(HouseholdDetailDto {
        id: household.id,
        name: household.name,
    }))
}

#[utoipa::path(
    get,
    path = "/households/current/members",
    operation_id = "household_members_list",
    tag = "households",
    responses((status = 200, body = [MemberDto])),
    security(("bearer" = [])),
)]
pub async fn list_members(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<Vec<MemberDto>>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let rows = qm_db::memberships::list_members(&state.db, household_id).await?;
    let items = rows
        .into_iter()
        .map(|row| {
            Ok::<_, ApiError>(MemberDto {
                user: UserDto {
                    id: row.membership.user_id,
                    username: row.username,
                    email: row.email,
                },
                role: MembershipRole::from_str(&row.membership.role)?,
                joined_at: row.membership.joined_at,
            })
        })
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(items))
}

#[utoipa::path(
    delete,
    path = "/households/current/members/{user_id}",
    operation_id = "household_member_remove",
    tag = "households",
    params(("user_id" = Uuid, Path)),
    responses((status = 204), (status = 403, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn remove_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    require_admin(&current)?;
    let membership = qm_db::memberships::find(&state.db, household_id, user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if membership.role == ROLE_ADMIN && qm_db::memberships::count_admins(&state.db, household_id).await? <= 1 {
        return Err(ApiError::LastAdminRemoval);
    }
    let removed = qm_db::memberships::remove(&state.db, household_id, user_id).await?;
    if !removed {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/households/current/invites",
    operation_id = "household_invite_create",
    tag = "households",
    request_body = CreateInviteRequest,
    responses((status = 201, body = InviteDto), (status = 403, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn create_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateInviteRequest>,
) -> ApiResult<(StatusCode, Json<InviteDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    require_admin(&current)?;
    if req.max_uses < 1 {
        return Err(ApiError::BadRequest("max_uses must be >= 1".into()));
    }
    let expires = DateTime::parse_from_rfc3339(&req.expires_at)
        .map_err(|_| ApiError::BadRequest("expires_at must be RFC-3339".into()))?
        .with_timezone(&Utc);
    if expires <= Utc::now() {
        return Err(ApiError::BadRequest("expires_at must be in the future".into()));
    }
    let code = Uuid::now_v7().simple().to_string()[..12].to_ascii_uppercase();
    let row = qm_db::invites::create(
        &state.db,
        household_id,
        &code,
        current.user_id,
        &req.expires_at,
        req.max_uses,
        req.role_granted.as_str(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(invite_to_dto(row)?)))
}

#[utoipa::path(
    get,
    path = "/households/current/invites",
    operation_id = "household_invites_list",
    tag = "households",
    responses((status = 200, body = [InviteDto])),
    security(("bearer" = [])),
)]
pub async fn list_invites(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<Vec<InviteDto>>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    require_admin(&current)?;
    let rows = qm_db::invites::list_for_household(&state.db, household_id).await?;
    Ok(Json(
        rows.into_iter()
            .map(invite_to_dto)
            .collect::<ApiResult<Vec<_>>>()?,
    ))
}

#[utoipa::path(
    delete,
    path = "/invites/{id}",
    operation_id = "invite_revoke",
    tag = "households",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 403, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn revoke_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    require_admin(&current)?;
    let revoked = qm_db::invites::revoke(&state.db, id, household_id).await?;
    if !revoked {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/invites/redeem",
    operation_id = "invite_redeem",
    tag = "households",
    request_body = RedeemInviteRequest,
    responses((status = 204), (status = 400, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn redeem_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<RedeemInviteRequest>,
) -> ApiResult<StatusCode> {
    let code = req.invite_code.trim().to_ascii_uppercase();
    let invite = validate_invite(&state, &code).await?;
    if qm_db::memberships::find(&state.db, invite.household_id, current.user_id)
        .await?
        .is_some()
    {
        return Err(ApiError::AlreadyMember);
    }
    qm_db::memberships::insert(&state.db, invite.household_id, current.user_id, &invite.role_granted)
        .await?;
    if !qm_db::invites::consume(&state.db, invite.id).await? {
        return Err(ApiError::InvalidInvite);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn validate_invite(state: &AppState, code: &str) -> ApiResult<qm_db::invites::InviteRow> {
    let status = qm_db::invites::status_for_code(&state.db, code).await?;
    if status != qm_db::invites::InviteStatus::Active {
        return Err(ApiError::InvalidInvite);
    }
    qm_db::invites::find_by_code(&state.db, code)
        .await?
        .ok_or(ApiError::InvalidInvite)
}

fn require_admin(current: &CurrentUser) -> ApiResult<()> {
    if current.role.as_deref() == Some(ROLE_ADMIN) {
        Ok(())
    } else {
        Err(ApiError::AdminOnly)
    }
}

fn invite_to_dto(row: qm_db::invites::InviteRow) -> ApiResult<InviteDto> {
    Ok(InviteDto {
        id: row.id,
        code: row.code,
        role_granted: MembershipRole::from_str(&row.role_granted)?,
        expires_at: row.expires_at,
        max_uses: row.max_uses,
        use_count: row.use_count,
        created_at: row.created_at,
    })
}
