use std::str::FromStr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use jiff::{SignedDuration, Timestamp};
use qm_core::units::MeasurementSystem;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    quotas,
    rate_limit::RateLimitTarget,
    routes::accounts::{self, MeResponse, UserDto},
    types::MembershipRole,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/households", post(create_household))
        .route("/households/import", post(import_household))
        .route(
            "/households/current",
            get(get_current_household).patch(update_current_household),
        )
        .route("/households/current/export", get(export_current_household))
        .route(
            "/households/current/deletion",
            post(request_current_household_deletion),
        )
        .route("/households/current/members", get(list_members))
        .route(
            "/households/current/members/{user_id}",
            delete(remove_member),
        )
        .route(
            "/households/current/invites",
            get(list_invites).post(create_invite),
        )
        .route("/invites/{id}", delete(revoke_invite))
        .route("/invites/redeem", post(redeem_invite))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HouseholdDetailDto {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub measurement_system: MeasurementSystem,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateHouseholdRequest {
    pub name: String,
    pub timezone: String,
    pub measurement_system: Option<MeasurementSystem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateHouseholdRequest {
    pub name: String,
    pub timezone: String,
    pub measurement_system: MeasurementSystem,
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
    pub max_uses: i64,
    pub role_granted: MembershipRole,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RedeemInviteRequest {
    pub invite_code: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteHouseholdRequest {
    pub confirmation_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteHouseholdResponse {
    pub household_id: Uuid,
    pub purge_job_id: Uuid,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/households",
    operation_id = "household_create",
    tag = "households",
    request_body = CreateHouseholdRequest,
    responses((status = 201, body = MeResponse), (status = 400, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn create_household(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateHouseholdRequest>,
) -> ApiResult<(StatusCode, Json<MeResponse>)> {
    let name = validate_household_name(&req.name)?;
    let timezone = validate_household_timezone(&req.timezone)?;
    let measurement_system = req.measurement_system.unwrap_or(MeasurementSystem::DEFAULT);
    let billing_account_id =
        quotas::ensure_can_add_household_for_current_account(&state, current.household_id).await?;
    let mut household = qm_db::households::create(&state.db, name, timezone).await?;
    if let Some(billing_account_id) = billing_account_id {
        qm_db::billing::attach_household(&state.db, household.id, billing_account_id).await?;
    } else {
        qm_db::billing::ensure_for_household(
            &state.db,
            household.id,
            qm_db::billing::DEFAULT_PLAN_KEY,
        )
        .await?;
    }
    if measurement_system != MeasurementSystem::DEFAULT {
        household = qm_db::households::update(
            &state.db,
            household.id,
            name,
            timezone,
            measurement_system.as_str(),
        )
        .await?
        .ok_or(ApiError::NotFound)?;
    }
    qm_db::locations::seed_defaults(&state.db, household.id).await?;
    qm_db::memberships::insert(&state.db, household.id, current.user_id, auth::ROLE_ADMIN).await?;
    qm_db::auth_sessions::upsert(
        &state.db,
        current.session_id,
        current.user_id,
        Some(household.id),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(accounts::build_me_response(&state, current.user_id, Some(household.id)).await?),
    ))
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
        timezone: household.timezone,
        measurement_system: measurement_system_from_db(&household.measurement_system)?,
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
    auth::require_admin(&current)?;
    let name = validate_household_name(&req.name)?;
    let timezone = validate_household_timezone(&req.timezone)?;
    let household = qm_db::households::update(
        &state.db,
        household_id,
        name,
        timezone,
        req.measurement_system.as_str(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(HouseholdDetailDto {
        id: household.id,
        name: household.name,
        timezone: household.timezone,
        measurement_system: req.measurement_system,
    }))
}

#[utoipa::path(
    get,
    path = "/households/current/export",
    operation_id = "household_current_export",
    tag = "households",
    responses((status = 200, body = qm_db::household_exports::HouseholdExportDocument), (status = 403, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn export_current_household(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<qm_db::household_exports::HouseholdExportDocument>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_admin(&current)?;
    let document = qm_db::household_exports::export_household(&state.db, household_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(document))
}

#[utoipa::path(
    post,
    path = "/households/import",
    operation_id = "household_import",
    tag = "households",
    request_body = qm_db::household_exports::HouseholdExportDocument,
    responses((status = 201, body = MeResponse), (status = 400, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn import_household(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(document): Json<qm_db::household_exports::HouseholdExportDocument>,
) -> ApiResult<(StatusCode, Json<MeResponse>)> {
    let outcome = qm_db::household_exports::import_household(&state.db, &document, current.user_id)
        .await
        .map_err(import_error_to_api)?;
    qm_db::auth_sessions::upsert(
        &state.db,
        current.session_id,
        current.user_id,
        Some(outcome.household_id),
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(
            accounts::build_me_response(&state, current.user_id, Some(outcome.household_id))
                .await?,
        ),
    ))
}

#[utoipa::path(
    post,
    path = "/households/current/deletion",
    operation_id = "household_current_deletion_request",
    tag = "households",
    request_body = DeleteHouseholdRequest,
    responses((status = 202, body = DeleteHouseholdResponse), (status = 403, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn request_current_household_deletion(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<DeleteHouseholdRequest>,
) -> ApiResult<(StatusCode, Json<DeleteHouseholdResponse>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_admin(&current)?;
    let outcome = qm_db::household_exports::request_household_deletion(
        &state.db,
        household_id,
        current.user_id,
        &req.confirmation_name,
    )
    .await
    .map_err(delete_error_to_api)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(DeleteHouseholdResponse {
            household_id,
            purge_job_id: outcome.purge_job_id,
            status: "queued".into(),
        }),
    ))
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
                    email: row.email,
                    display_name: row.display_name,
                    email_verified_at: row.email_verified_at,
                    pending_email: None,
                    pending_email_verification_expires_at: None,
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
    auth::require_admin(&current)?;
    let membership = qm_db::memberships::find(&state.db, household_id, user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if membership.role == auth::ROLE_ADMIN
        && qm_db::memberships::count_admins(&state.db, household_id).await? <= 1
    {
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
    auth::require_admin(&current)?;
    enforce_invite_rate_limits(&state, household_id, current.user_id).await?;
    quotas::ensure_can_add_invite(&state, household_id).await?;
    if req.max_uses < 1 {
        return Err(ApiError::BadRequest("max_uses must be >= 1".into()));
    }
    let expires_at = Timestamp::now()
        .checked_add(SignedDuration::from_secs(state.config.invite_ttl_seconds))
        .map_err(|_| ApiError::BadRequest("invite expiry is out of range".into()))
        .map(qm_db::time::format_timestamp)?;
    let code = Uuid::now_v7().simple().to_string()[..12].to_ascii_uppercase();
    let row = qm_db::invites::create(
        &state.db,
        household_id,
        &code,
        current.user_id,
        &expires_at,
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
    auth::require_admin(&current)?;
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
    auth::require_admin(&current)?;
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
    responses((status = 204), (status = 400, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn redeem_invite(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<RedeemInviteRequest>,
) -> ApiResult<StatusCode> {
    let code = req.invite_code.trim().to_ascii_uppercase();
    if let Some(invite) = qm_db::invites::find_by_code(&state.db, &code).await? {
        if qm_db::invites::classify(&invite) == qm_db::invites::InviteStatus::Active
            && qm_db::memberships::find(&state.db, invite.household_id, current.user_id)
                .await?
                .is_none()
        {
            quotas::ensure_can_add_member(&state, invite.household_id).await?;
        }
    }
    match qm_db::invites::redeem_for_user(&state.db, &code, current.user_id).await {
        Ok(qm_db::invites::RedeemOutcome::Joined { household_id })
        | Ok(qm_db::invites::RedeemOutcome::AlreadyMember { household_id }) => {
            qm_db::auth_sessions::upsert(
                &state.db,
                current.session_id,
                current.user_id,
                Some(household_id),
            )
            .await?;
            Ok(StatusCode::NO_CONTENT)
        }
        Err(qm_db::invites::RedeemInviteError::InvalidInvite) => Err(ApiError::InvalidInvite),
        Err(qm_db::invites::RedeemInviteError::Database(err)) => Err(ApiError::Database(err)),
    }
}

async fn enforce_invite_rate_limits(
    state: &AppState,
    household_id: Uuid,
    user_id: Uuid,
) -> ApiResult<()> {
    if !state
        .rate_limiters
        .allow(
            RateLimitTarget::InviteHousehold,
            &format!("household:{household_id}"),
        )
        .await
        || !state
            .rate_limiters
            .allow(RateLimitTarget::InviteUser, &format!("user:{user_id}"))
            .await
    {
        return Err(ApiError::RateLimited);
    }
    Ok(())
}

pub(crate) fn validate_household_name(name: &str) -> ApiResult<&str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(ApiError::BadRequest(
            "household name must be 1..=128 chars".into(),
        ));
    }
    Ok(name)
}

pub(crate) fn validate_household_timezone(timezone: &str) -> ApiResult<&str> {
    let timezone = timezone.trim();
    if timezone.is_empty() {
        return Err(ApiError::BadRequest("timezone is required".into()));
    }
    jiff::tz::db()
        .get(timezone)
        .map_err(|_| ApiError::BadRequest("timezone must be a valid IANA zone".into()))?;
    Ok(timezone)
}

pub(crate) fn measurement_system_from_db(value: &str) -> ApiResult<MeasurementSystem> {
    MeasurementSystem::from_str_ci(value).ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!(
            "invalid household measurement_system stored in database: {value}"
        ))
    })
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

fn import_error_to_api(err: qm_db::household_exports::ImportError) -> ApiError {
    match err {
        qm_db::household_exports::ImportError::UnsupportedSchemaVersion(_)
        | qm_db::household_exports::ImportError::DuplicateId(_)
        | qm_db::household_exports::ImportError::DanglingReference(_)
        | qm_db::household_exports::ImportError::InvalidValue(_) => {
            ApiError::BadRequest(err.to_string())
        }
        qm_db::household_exports::ImportError::Database(err) => ApiError::Database(err),
    }
}

fn delete_error_to_api(err: qm_db::household_exports::DeleteRequestError) -> ApiError {
    match err {
        qm_db::household_exports::DeleteRequestError::NotFound => ApiError::NotFound,
        qm_db::household_exports::DeleteRequestError::ConfirmationMismatch => {
            ApiError::BadRequest(err.to_string())
        }
        qm_db::household_exports::DeleteRequestError::Database(err) => ApiError::Database(err),
    }
}
