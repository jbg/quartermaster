use std::str::FromStr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use jiff::{civil::Date, tz, Timestamp, ToSpan};
use qm_core::units::{self, UnitFamily};
use qm_db::replenishment::{
    self, NewCartRun, NewDemandSignal, NewReplenishmentRule, ReplenishmentCartRunRow,
    ReplenishmentDemandSignalRow, ReplenishmentRuleRow, ReplenishmentSettingsRow,
    ReplenishmentSupplierPolicyRow, UpdateReplenishmentRule, UpsertReplenishmentSettings,
    UpsertSupplierPolicy,
};
use qm_db::suppliers::{self, NewCartDraft, NewCartLine, NewCatalogItem, NewSupplier};
use qm_suppliers::{
    Availability, CartDraft, CartLine, CartStatus, CatalogItem, InterventionState,
    MockSupplierIntegration, SupplierId, SupplierIntegration,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    AppState,
};

const HISTORY_DAYS: i64 = 30;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/replenishment/rules", get(list_rules).post(create_rule))
        .route(
            "/replenishment/rules/{id}",
            get(get_rule).patch(update_rule).delete(delete_rule),
        )
        .route("/replenishment/rules/{id}/pause", post(pause_rule))
        .route("/replenishment/rules/{id}/resume", post(resume_rule))
        .route(
            "/replenishment/settings",
            get(get_settings).put(put_settings),
        )
        .route(
            "/replenishment/suppliers/{supplier_id}/policy",
            get(get_supplier_policy).put(put_supplier_policy),
        )
        .route(
            "/replenishment/demand-signals",
            get(list_demand_signals).post(create_demand_signal),
        )
        .route(
            "/replenishment/demand-signals/{id}",
            patch(patch_demand_signal),
        )
        .route("/replenishment/cart-drafts", post(create_cart_draft))
        .route("/replenishment/cart-runs/{id}", get(get_cart_run))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplenishmentAutomationLevelDto {
    Off,
    Suggestions,
    ConfirmToSubmit,
    TrustedAutoSubmit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplenishmentDemandSignalTypeDto {
    ManualShopping,
    UpcomingRecipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplenishmentDemandSignalStatusDto {
    Active,
    Dismissed,
    Fulfilled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplenishmentGuardrailDecisionDto {
    Allowed,
    NeedsApproval,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplenishmentSuppressionReasonDto {
    AutomationOff,
    RulePaused,
    MissingSupplierMapping,
    SupplierMismatch,
    SufficientStock,
    ExpiringStockAvailable,
    PendingReplenishment,
    GlobalDisabled,
    SupplierDisabled,
    BudgetExceeded,
    UnknownPrice,
    InvalidRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplenishmentCartRunStatusDto {
    DraftCreated,
    Blocked,
    Submitted,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentRuleListResponse {
    pub items: Vec<ReplenishmentRuleDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentRuleDto {
    pub id: Uuid,
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub minimum_quantity: String,
    pub target_quantity: String,
    pub unit: String,
    pub preferred_supplier_id: Option<String>,
    pub preferred_supplier_item_id: Option<String>,
    pub preferred_package_quantity: Option<String>,
    pub preferred_package_unit: Option<String>,
    pub automation_level: ReplenishmentAutomationLevelDto,
    pub expiry_suppression_days: Option<i64>,
    pub paused_at: Option<String>,
    pub pause_reason: Option<String>,
    pub spend_cap_amount: Option<String>,
    pub spend_cap_currency: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplenishmentRuleRequest {
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub minimum_quantity: String,
    pub target_quantity: String,
    pub unit: String,
    pub preferred_supplier_id: Option<String>,
    pub preferred_supplier_item_id: Option<String>,
    pub preferred_package_quantity: Option<String>,
    pub preferred_package_unit: Option<String>,
    pub automation_level: ReplenishmentAutomationLevelDto,
    pub expiry_suppression_days: Option<i64>,
    pub spend_cap_amount: Option<String>,
    pub spend_cap_currency: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplenishmentPauseRuleRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentSettingsDto {
    pub global_disabled: bool,
    pub default_spend_cap_amount: Option<String>,
    pub default_spend_cap_currency: Option<String>,
    pub notification_lead_minutes: i64,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplenishmentSettingsRequest {
    pub global_disabled: bool,
    pub default_spend_cap_amount: Option<String>,
    pub default_spend_cap_currency: Option<String>,
    pub notification_lead_minutes: i64,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentSupplierPolicyDto {
    pub supplier_id: String,
    pub disabled: bool,
    pub spend_cap_amount: Option<String>,
    pub spend_cap_currency: Option<String>,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplenishmentSupplierPolicyRequest {
    pub disabled: bool,
    pub spend_cap_amount: Option<String>,
    pub spend_cap_currency: Option<String>,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentDemandSignalListResponse {
    pub items: Vec<ReplenishmentDemandSignalDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentDemandSignalDto {
    pub id: Uuid,
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub signal_type: ReplenishmentDemandSignalTypeDto,
    pub status: ReplenishmentDemandSignalStatusDto,
    pub quantity: String,
    pub unit: String,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub desired_on: Option<String>,
    pub supplier_id: Option<String>,
    pub supplier_item_id: Option<String>,
    pub note: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplenishmentDemandSignalRequest {
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub signal_type: ReplenishmentDemandSignalTypeDto,
    pub quantity: String,
    pub unit: String,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub desired_on: Option<String>,
    pub supplier_id: Option<String>,
    pub supplier_item_id: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplenishmentPatchDemandSignalRequest {
    pub status: ReplenishmentDemandSignalStatusDto,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReplenishmentCreateCartDraftRequest {
    pub supplier_id: Option<String>,
    #[serde(default)]
    pub include_ai_explanation: bool,
    #[serde(default)]
    pub submit_trusted: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentCreateCartDraftResponse {
    pub run: ReplenishmentCartRunDto,
    pub draft_id: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReplenishmentCartRunDto {
    pub id: Uuid,
    pub draft_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub supplier_id: Option<String>,
    pub status: ReplenishmentCartRunStatusDto,
    pub source: String,
    pub guardrail_decision: ReplenishmentGuardrailDecisionDto,
    pub guardrail_snapshot: Value,
    pub recommendations: Value,
    pub suppressions: Value,
    pub ai_explanation: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
struct RecommendationAudit {
    rule_id: Uuid,
    product_id: Uuid,
    supplier_id: String,
    supplier_item_id: String,
    quantity: String,
    unit: Option<String>,
    estimated_price_amount: Option<String>,
    estimated_price_currency: Option<String>,
    automation_level: String,
}

#[derive(Debug, Serialize)]
struct SuppressionAudit {
    rule_id: Uuid,
    product_id: Uuid,
    reason: ReplenishmentSuppressionReasonDto,
    detail: String,
}

struct GeneratedLine {
    rule: ReplenishmentRuleRow,
    supplier_id: String,
    supplier_item_id: String,
    quantity: Decimal,
    unit: Option<String>,
    estimated_price: Option<Decimal>,
    estimated_currency: Option<String>,
}

#[utoipa::path(
    get,
    path = "/replenishment/rules",
    operation_id = "replenishment_rule_list",
    tag = "replenishment",
    responses((status = 200, body = ReplenishmentRuleListResponse)),
    security(("bearer" = [])),
)]
pub async fn list_rules(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<ReplenishmentRuleListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let items = replenishment::list_rules(&state.db, household_id)
        .await?
        .into_iter()
        .map(rule_dto)
        .collect::<ApiResult<_>>()?;
    Ok(Json(ReplenishmentRuleListResponse { items }))
}

#[utoipa::path(
    post,
    path = "/replenishment/rules",
    operation_id = "replenishment_rule_create",
    tag = "replenishment",
    request_body = ReplenishmentRuleRequest,
    responses((status = 201, body = ReplenishmentRuleDto)),
    security(("bearer" = [])),
)]
pub async fn create_rule(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<ReplenishmentRuleRequest>,
) -> ApiResult<(StatusCode, Json<ReplenishmentRuleDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    validate_rule_request(&state, household_id, &req).await?;
    ensure_optional_supplier(&state, req.preferred_supplier_id.as_deref()).await?;
    let row = replenishment::create_rule(
        &state.db,
        household_id,
        current.user_id,
        &NewReplenishmentRule {
            product_id: req.product_id,
            location_id: req.location_id,
            minimum_quantity: req.minimum_quantity.trim(),
            target_quantity: req.target_quantity.trim(),
            unit: req.unit.trim(),
            preferred_supplier_id: req.preferred_supplier_id.as_deref(),
            preferred_supplier_item_id: req.preferred_supplier_item_id.as_deref(),
            preferred_package_quantity: req.preferred_package_quantity.as_deref(),
            preferred_package_unit: req.preferred_package_unit.as_deref(),
            automation_level: req.automation_level.as_str(),
            expiry_suppression_days: req.expiry_suppression_days,
            spend_cap_amount: req.spend_cap_amount.as_deref(),
            spend_cap_currency: req.spend_cap_currency.as_deref(),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(rule_dto(row)?)))
}

#[utoipa::path(
    get,
    path = "/replenishment/rules/{id}",
    operation_id = "replenishment_rule_get",
    tag = "replenishment",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ReplenishmentRuleDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_rule(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReplenishmentRuleDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = replenishment::find_rule(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(rule_dto(row)?))
}

#[utoipa::path(
    patch,
    path = "/replenishment/rules/{id}",
    operation_id = "replenishment_rule_update",
    tag = "replenishment",
    params(("id" = Uuid, Path)),
    request_body = ReplenishmentRuleRequest,
    responses((status = 200, body = ReplenishmentRuleDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn update_rule(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ReplenishmentRuleRequest>,
) -> ApiResult<Json<ReplenishmentRuleDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    validate_rule_request(&state, household_id, &req).await?;
    ensure_optional_supplier(&state, req.preferred_supplier_id.as_deref()).await?;
    let row = replenishment::update_rule(
        &state.db,
        household_id,
        id,
        current.user_id,
        &UpdateReplenishmentRule {
            product_id: req.product_id,
            location_id: req.location_id,
            minimum_quantity: req.minimum_quantity.trim(),
            target_quantity: req.target_quantity.trim(),
            unit: req.unit.trim(),
            preferred_supplier_id: req.preferred_supplier_id.as_deref(),
            preferred_supplier_item_id: req.preferred_supplier_item_id.as_deref(),
            preferred_package_quantity: req.preferred_package_quantity.as_deref(),
            preferred_package_unit: req.preferred_package_unit.as_deref(),
            automation_level: req.automation_level.as_str(),
            expiry_suppression_days: req.expiry_suppression_days,
            spend_cap_amount: req.spend_cap_amount.as_deref(),
            spend_cap_currency: req.spend_cap_currency.as_deref(),
        },
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(rule_dto(row)?))
}

#[utoipa::path(
    delete,
    path = "/replenishment/rules/{id}",
    operation_id = "replenishment_rule_delete",
    tag = "replenishment",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn delete_rule(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    if replenishment::delete_rule(&state.db, household_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    post,
    path = "/replenishment/rules/{id}/pause",
    operation_id = "replenishment_rule_pause",
    tag = "replenishment",
    params(("id" = Uuid, Path)),
    request_body = ReplenishmentPauseRuleRequest,
    responses((status = 200, body = ReplenishmentRuleDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn pause_rule(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ReplenishmentPauseRuleRequest>,
) -> ApiResult<Json<ReplenishmentRuleDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let row = replenishment::set_rule_paused(
        &state.db,
        household_id,
        id,
        current.user_id,
        true,
        req.reason.as_deref(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(rule_dto(row)?))
}

#[utoipa::path(
    post,
    path = "/replenishment/rules/{id}/resume",
    operation_id = "replenishment_rule_resume",
    tag = "replenishment",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ReplenishmentRuleDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn resume_rule(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReplenishmentRuleDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let row =
        replenishment::set_rule_paused(&state.db, household_id, id, current.user_id, false, None)
            .await?
            .ok_or(ApiError::NotFound)?;
    Ok(Json(rule_dto(row)?))
}

#[utoipa::path(
    get,
    path = "/replenishment/settings",
    operation_id = "replenishment_settings_get",
    tag = "replenishment",
    responses((status = 200, body = ReplenishmentSettingsDto)),
    security(("bearer" = [])),
)]
pub async fn get_settings(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<ReplenishmentSettingsDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = replenishment::get_or_create_settings(&state.db, household_id).await?;
    Ok(Json(settings_dto(row)))
}

#[utoipa::path(
    put,
    path = "/replenishment/settings",
    operation_id = "replenishment_settings_put",
    tag = "replenishment",
    request_body = ReplenishmentSettingsRequest,
    responses((status = 200, body = ReplenishmentSettingsDto)),
    security(("bearer" = [])),
)]
pub async fn put_settings(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<ReplenishmentSettingsRequest>,
) -> ApiResult<Json<ReplenishmentSettingsDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    validate_spend_cap(
        req.default_spend_cap_amount.as_deref(),
        req.default_spend_cap_currency.as_deref(),
    )?;
    validate_quiet_hours(
        req.quiet_hours_start.as_deref(),
        req.quiet_hours_end.as_deref(),
    )?;
    if req.notification_lead_minutes < 0 {
        return Err(ApiError::BadRequest(
            "notification_lead_minutes must not be negative".into(),
        ));
    }
    let row = replenishment::upsert_settings(
        &state.db,
        household_id,
        current.user_id,
        &UpsertReplenishmentSettings {
            global_disabled: req.global_disabled,
            default_spend_cap_amount: req.default_spend_cap_amount.as_deref(),
            default_spend_cap_currency: req.default_spend_cap_currency.as_deref(),
            notification_lead_minutes: req.notification_lead_minutes,
            quiet_hours_start: req.quiet_hours_start.as_deref(),
            quiet_hours_end: req.quiet_hours_end.as_deref(),
        },
    )
    .await?;
    Ok(Json(settings_dto(row)))
}

#[utoipa::path(
    get,
    path = "/replenishment/suppliers/{supplier_id}/policy",
    operation_id = "replenishment_supplier_policy_get",
    tag = "replenishment",
    params(("supplier_id" = String, Path)),
    responses((status = 200, body = ReplenishmentSupplierPolicyDto)),
    security(("bearer" = [])),
)]
pub async fn get_supplier_policy(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(supplier_id): Path<String>,
) -> ApiResult<Json<ReplenishmentSupplierPolicyDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    ensure_optional_supplier(&state, Some(&supplier_id)).await?;
    if let Some(row) =
        replenishment::find_supplier_policy(&state.db, household_id, &supplier_id).await?
    {
        return Ok(Json(supplier_policy_dto(row)));
    }
    Ok(Json(ReplenishmentSupplierPolicyDto {
        supplier_id,
        disabled: false,
        spend_cap_amount: None,
        spend_cap_currency: None,
        quiet_hours_start: None,
        quiet_hours_end: None,
        updated_at: qm_db::now_utc_rfc3339(),
    }))
}

#[utoipa::path(
    put,
    path = "/replenishment/suppliers/{supplier_id}/policy",
    operation_id = "replenishment_supplier_policy_put",
    tag = "replenishment",
    params(("supplier_id" = String, Path)),
    request_body = ReplenishmentSupplierPolicyRequest,
    responses((status = 200, body = ReplenishmentSupplierPolicyDto)),
    security(("bearer" = [])),
)]
pub async fn put_supplier_policy(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(supplier_id): Path<String>,
    Json(req): Json<ReplenishmentSupplierPolicyRequest>,
) -> ApiResult<Json<ReplenishmentSupplierPolicyDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    ensure_optional_supplier(&state, Some(&supplier_id)).await?;
    validate_spend_cap(
        req.spend_cap_amount.as_deref(),
        req.spend_cap_currency.as_deref(),
    )?;
    validate_quiet_hours(
        req.quiet_hours_start.as_deref(),
        req.quiet_hours_end.as_deref(),
    )?;
    let row = replenishment::upsert_supplier_policy(
        &state.db,
        household_id,
        current.user_id,
        &UpsertSupplierPolicy {
            supplier_id: &supplier_id,
            disabled: req.disabled,
            spend_cap_amount: req.spend_cap_amount.as_deref(),
            spend_cap_currency: req.spend_cap_currency.as_deref(),
            quiet_hours_start: req.quiet_hours_start.as_deref(),
            quiet_hours_end: req.quiet_hours_end.as_deref(),
        },
    )
    .await?;
    Ok(Json(supplier_policy_dto(row)))
}

#[utoipa::path(
    get,
    path = "/replenishment/demand-signals",
    operation_id = "replenishment_demand_signal_list",
    tag = "replenishment",
    responses((status = 200, body = ReplenishmentDemandSignalListResponse)),
    security(("bearer" = [])),
)]
pub async fn list_demand_signals(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<ReplenishmentDemandSignalListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let items = replenishment::list_demand_signals(&state.db, household_id, false)
        .await?
        .into_iter()
        .map(demand_signal_dto)
        .collect::<ApiResult<_>>()?;
    Ok(Json(ReplenishmentDemandSignalListResponse { items }))
}

#[utoipa::path(
    post,
    path = "/replenishment/demand-signals",
    operation_id = "replenishment_demand_signal_create",
    tag = "replenishment",
    request_body = ReplenishmentDemandSignalRequest,
    responses((status = 201, body = ReplenishmentDemandSignalDto)),
    security(("bearer" = [])),
)]
pub async fn create_demand_signal(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<ReplenishmentDemandSignalRequest>,
) -> ApiResult<(StatusCode, Json<ReplenishmentDemandSignalDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let product = load_product_for_write(&state, household_id, req.product_id).await?;
    validate_quantity_for_product(&product, &req.quantity, &req.unit)?;
    if let Some(location_id) = req.location_id {
        ensure_location(&state, household_id, location_id).await?;
    }
    if let Some(desired_on) = req.desired_on.as_deref() {
        validate_date(desired_on)?;
    }
    ensure_optional_supplier(&state, req.supplier_id.as_deref()).await?;
    let metadata_json = json_string(&req.metadata)?;
    let row = replenishment::create_demand_signal(
        &state.db,
        household_id,
        current.user_id,
        &NewDemandSignal {
            product_id: req.product_id,
            location_id: req.location_id,
            signal_type: req.signal_type.as_str(),
            quantity: req.quantity.trim(),
            unit: req.unit.trim(),
            recipe_id: req.recipe_id,
            recipe_version_id: req.recipe_version_id,
            desired_on: req.desired_on.as_deref(),
            supplier_id: req.supplier_id.as_deref(),
            supplier_item_id: req.supplier_item_id.as_deref(),
            note: req.note.as_deref(),
            metadata_json: &metadata_json,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(demand_signal_dto(row)?)))
}

#[utoipa::path(
    patch,
    path = "/replenishment/demand-signals/{id}",
    operation_id = "replenishment_demand_signal_patch",
    tag = "replenishment",
    params(("id" = Uuid, Path)),
    request_body = ReplenishmentPatchDemandSignalRequest,
    responses((status = 200, body = ReplenishmentDemandSignalDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn patch_demand_signal(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ReplenishmentPatchDemandSignalRequest>,
) -> ApiResult<Json<ReplenishmentDemandSignalDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let row = replenishment::update_demand_signal_status(
        &state.db,
        household_id,
        id,
        current.user_id,
        req.status.as_str(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(demand_signal_dto(row)?))
}

#[utoipa::path(
    post,
    path = "/replenishment/cart-drafts",
    operation_id = "replenishment_cart_draft_create",
    tag = "replenishment",
    request_body = ReplenishmentCreateCartDraftRequest,
    responses((status = 201, body = ReplenishmentCreateCartDraftResponse)),
    security(("bearer" = [])),
)]
pub async fn create_cart_draft(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<ReplenishmentCreateCartDraftRequest>,
) -> ApiResult<(StatusCode, Json<ReplenishmentCreateCartDraftResponse>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    ensure_mock_supplier(&state).await?;
    if let Some(supplier_id) = req.supplier_id.as_deref() {
        ensure_optional_supplier(&state, Some(supplier_id)).await?;
    }

    let generated = generate_cart(&state, household_id, current.user_id, &req).await?;
    Ok((StatusCode::CREATED, Json(generated)))
}

#[utoipa::path(
    get,
    path = "/replenishment/cart-runs/{id}",
    operation_id = "replenishment_cart_run_get",
    tag = "replenishment",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ReplenishmentCartRunDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_cart_run(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReplenishmentCartRunDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = replenishment::find_cart_run(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(cart_run_dto(row)?))
}

async fn generate_cart(
    state: &AppState,
    household_id: Uuid,
    actor_id: Uuid,
    req: &ReplenishmentCreateCartDraftRequest,
) -> ApiResult<ReplenishmentCreateCartDraftResponse> {
    let household = qm_db::households::find_by_id(&state.db, household_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let today = household_today(&household.timezone)?;
    let rules = replenishment::list_rules(&state.db, household_id).await?;
    let signals = replenishment::list_demand_signals(&state.db, household_id, true).await?;
    let settings = replenishment::get_or_create_settings(&state.db, household_id).await?;

    let mut generated_lines = Vec::new();
    let mut suppressions = Vec::new();
    for rule in rules {
        if rule.automation_level == replenishment::AUTOMATION_OFF {
            suppressions.push(SuppressionAudit {
                rule_id: rule.id,
                product_id: rule.product_id,
                reason: ReplenishmentSuppressionReasonDto::AutomationOff,
                detail: "rule automation level is off".into(),
            });
            continue;
        }
        if rule.paused_at.is_some() {
            suppressions.push(SuppressionAudit {
                rule_id: rule.id,
                product_id: rule.product_id,
                reason: ReplenishmentSuppressionReasonDto::RulePaused,
                detail: rule
                    .pause_reason
                    .clone()
                    .unwrap_or_else(|| "rule is paused".into()),
            });
            continue;
        }
        if let Some(requested_supplier) = req.supplier_id.as_deref() {
            if rule
                .preferred_supplier_id
                .as_deref()
                .is_some_and(|supplier_id| supplier_id != requested_supplier)
            {
                suppressions.push(SuppressionAudit {
                    rule_id: rule.id,
                    product_id: rule.product_id,
                    reason: ReplenishmentSuppressionReasonDto::SupplierMismatch,
                    detail: format!("rule prefers a different supplier than {requested_supplier}"),
                });
                continue;
            }
        }
        if replenishment::pending_replenishment_exists(&state.db, household_id, rule.product_id)
            .await?
        {
            suppressions.push(SuppressionAudit {
                rule_id: rule.id,
                product_id: rule.product_id,
                reason: ReplenishmentSuppressionReasonDto::PendingReplenishment,
                detail: "active replenishment draft or undelivered order already exists".into(),
            });
            continue;
        }
        match generate_line_for_rule(state, household_id, &rule, &signals, today, req).await {
            Ok(Some(line)) => generated_lines.push(line),
            Ok(None) => suppressions.push(SuppressionAudit {
                rule_id: rule.id,
                product_id: rule.product_id,
                reason: ReplenishmentSuppressionReasonDto::SufficientStock,
                detail: "current stock and demand signals do not require a cart line".into(),
            }),
            Err(err) => suppressions.push(SuppressionAudit {
                rule_id: rule.id,
                product_id: rule.product_id,
                reason: suppression_reason_from_error(&err),
                detail: err.to_string(),
            }),
        }
    }

    let selected_supplier = req
        .supplier_id
        .clone()
        .or_else(|| generated_lines.first().map(|line| line.supplier_id.clone()));
    if let Some(selected_supplier) = selected_supplier.as_deref() {
        generated_lines.retain(|line| {
            if line.supplier_id == selected_supplier {
                true
            } else {
                suppressions.push(SuppressionAudit {
                    rule_id: line.rule.id,
                    product_id: line.rule.product_id,
                    reason: ReplenishmentSuppressionReasonDto::SupplierMismatch,
                    detail: format!("cart generation selected supplier {selected_supplier}"),
                });
                false
            }
        });
    }

    let guardrail_snapshot = guardrail_snapshot_for_lines(
        state,
        household_id,
        &settings,
        selected_supplier.as_deref(),
        &generated_lines,
    )
    .await?;
    let guardrail_decision = guardrail_snapshot["decision"]
        .as_str()
        .unwrap_or(replenishment::GUARDRAIL_BLOCKED)
        .to_owned();
    if settings.global_disabled {
        suppressions.extend(generated_lines.iter().map(|line| SuppressionAudit {
            rule_id: line.rule.id,
            product_id: line.rule.product_id,
            reason: ReplenishmentSuppressionReasonDto::GlobalDisabled,
            detail: "household replenishment is globally disabled".into(),
        }));
        generated_lines.clear();
    }

    let recommendations_json = json_string_ser(&recommendations_for(&generated_lines))?;
    let suppressions_json = json_string_ser(&suppressions)?;
    let guardrail_snapshot_json = json_string(&guardrail_snapshot)?;
    let ai_explanation_json = if req.include_ai_explanation {
        Some(json_string(&json!({
            "status": "deterministic_summary",
            "line_count": generated_lines.len(),
            "suppression_count": suppressions.len()
        }))?)
    } else {
        None
    };

    let mut draft_id = None;
    let mut run_status = replenishment::CART_RUN_STATUS_BLOCKED;
    if !generated_lines.is_empty() && guardrail_decision != replenishment::GUARDRAIL_BLOCKED {
        let supplier_id = selected_supplier.as_deref().ok_or_else(|| {
            ApiError::BadRequest("at least one replenishment line must have a supplier".into())
        })?;
        let supplier_cart = mock_supplier()
            .validate_cart(CartDraft {
                id: Uuid::now_v7(),
                supplier_id: SupplierId::new(supplier_id.to_owned()),
                lines: generated_lines
                    .iter()
                    .map(|line| CartLine {
                        supplier_item_id: line.supplier_item_id.clone(),
                        product_id: Some(line.rule.product_id),
                        quantity: decimal_string(line.quantity),
                        unit: line.unit.clone(),
                        note: Some(format!("replenishment rule {}", line.rule.id)),
                    })
                    .collect(),
                status: CartStatus::Draft,
                intervention: InterventionState::None,
            })
            .await
            .map_err(supplier_error)?;
        let line_quantities = generated_lines
            .iter()
            .map(|line| decimal_string(line.quantity))
            .collect::<Vec<_>>();
        let lines = generated_lines
            .iter()
            .zip(line_quantities.iter())
            .map(|(line, quantity)| NewCartLine {
                product_id: Some(line.rule.product_id),
                supplier_item_id: &line.supplier_item_id,
                quantity,
                unit: line.unit.as_deref(),
                note: Some("generated by replenishment"),
            })
            .collect();
        let (draft, _) = suppliers::create_cart_draft(
            &state.db,
            household_id,
            actor_id,
            &NewCartDraft {
                account_id: None,
                supplier_id,
                status: cart_status_str(supplier_cart.status),
                source: replenishment::CART_SOURCE_REPLENISHMENT,
                intervention_state: intervention_str(supplier_cart.intervention),
                review_notes: Some("Generated by replenishment rules"),
                lines,
            },
        )
        .await?;
        draft_id = Some(draft.id);
        run_status = replenishment::CART_RUN_STATUS_DRAFT_CREATED;
    }

    let run = replenishment::create_cart_run(
        &state.db,
        household_id,
        actor_id,
        &NewCartRun {
            draft_id,
            supplier_id: selected_supplier.as_deref(),
            status: run_status,
            source: replenishment::CART_SOURCE_REPLENISHMENT,
            guardrail_decision: &guardrail_decision,
            guardrail_snapshot_json: &guardrail_snapshot_json,
            recommendations_json: &recommendations_json,
            suppressions_json: &suppressions_json,
            ai_explanation_json: ai_explanation_json.as_deref(),
        },
    )
    .await?;

    Ok(ReplenishmentCreateCartDraftResponse {
        draft_id,
        run: cart_run_dto(run)?,
    })
}

async fn generate_line_for_rule(
    state: &AppState,
    household_id: Uuid,
    rule: &ReplenishmentRuleRow,
    signals: &[ReplenishmentDemandSignalRow],
    today: Date,
    _req: &ReplenishmentCreateCartDraftRequest,
) -> ApiResult<Option<GeneratedLine>> {
    let product = load_product_for_write(state, household_id, rule.product_id).await?;
    validate_quantity_for_product(&product, &rule.minimum_quantity, &rule.unit)?;
    validate_quantity_for_product(&product, &rule.target_quantity, &rule.unit)?;
    let current = current_quantity(state, household_id, rule, &product, today).await?;
    let mut demand = active_signal_quantity(&product, rule, signals)?;
    let (supplier_id, supplier_item_id) =
        preferred_supplier_item(state, household_id, rule).await?;
    let catalog = catalog_item(state, &supplier_id, &supplier_item_id).await?;
    if let Some(lead_time) = catalog.lead_time_max_days {
        demand +=
            historical_lead_time_demand(state, household_id, rule, &product, lead_time).await?;
    }

    let minimum = parse_decimal(&rule.minimum_quantity, "minimum_quantity")? + demand;
    if current >= minimum {
        return Ok(None);
    }
    let target = parse_decimal(&rule.target_quantity, "target_quantity")? + demand;
    let deficit = (target - current).max(Decimal::ZERO);
    if deficit <= Decimal::ZERO {
        return Ok(None);
    }

    let (quantity, unit) = order_quantity(
        rule,
        &product,
        deficit,
        catalog.pack_quantity.as_deref(),
        catalog.pack_unit.as_deref(),
    )?;
    let estimated_price = catalog
        .price_amount
        .as_deref()
        .map(|amount| parse_decimal(amount, "price_amount").map(|price| price * quantity))
        .transpose()?;
    Ok(Some(GeneratedLine {
        rule: rule.clone(),
        supplier_id,
        supplier_item_id,
        quantity,
        unit,
        estimated_price,
        estimated_currency: catalog.price_currency,
    }))
}

async fn current_quantity(
    state: &AppState,
    household_id: Uuid,
    rule: &ReplenishmentRuleRow,
    product: &qm_db::products::ProductRow,
    today: Date,
) -> ApiResult<Decimal> {
    let mut current = Decimal::ZERO;
    let stock = replenishment::active_stock_for_product(
        &state.db,
        household_id,
        rule.product_id,
        rule.location_id,
    )
    .await?;
    for batch in stock {
        if let Some(days) = rule.expiry_suppression_days {
            if let Some(expires_on) = batch.expires_on.as_deref() {
                let expires = validate_date(expires_on)?;
                let suppress_until = today
                    .checked_add(days.days())
                    .map_err(|err| ApiError::Internal(anyhow::Error::from(err)))?;
                if expires <= suppress_until {
                    return Err(ApiError::BadRequest(
                        "expiring stock is still available for this rule".into(),
                    ));
                }
            }
        }
        let qty = parse_decimal(&batch.quantity, "stock quantity")?;
        current += convert_quantity(qty, &batch.unit, &rule.unit, product)?;
    }
    Ok(current)
}

fn active_signal_quantity(
    product: &qm_db::products::ProductRow,
    rule: &ReplenishmentRuleRow,
    signals: &[ReplenishmentDemandSignalRow],
) -> ApiResult<Decimal> {
    signals
        .iter()
        .filter(|signal| {
            signal.product_id == rule.product_id
                && (signal.location_id.is_none()
                    || rule.location_id.is_none()
                    || signal.location_id == rule.location_id)
        })
        .try_fold(Decimal::ZERO, |acc, signal| {
            let qty = parse_decimal(&signal.quantity, "demand quantity")?;
            Ok(acc + convert_quantity(qty, &signal.unit, &rule.unit, product)?)
        })
}

async fn historical_lead_time_demand(
    state: &AppState,
    household_id: Uuid,
    rule: &ReplenishmentRuleRow,
    product: &qm_db::products::ProductRow,
    lead_time_days: i64,
) -> ApiResult<Decimal> {
    if lead_time_days <= 0 {
        return Ok(Decimal::ZERO);
    }
    let since = Timestamp::now()
        .checked_sub((HISTORY_DAYS * 24 * 60 * 60).seconds())
        .map_err(|err| ApiError::Internal(anyhow::Error::from(err)))?;
    let rows = replenishment::consumption_for_product_since(
        &state.db,
        household_id,
        rule.product_id,
        &qm_db::time::format_timestamp(since),
    )
    .await?;
    let total = rows.into_iter().try_fold(Decimal::ZERO, |acc, row| {
        let qty = parse_decimal(&row.quantity_delta, "consumption quantity")?.abs();
        Ok::<Decimal, ApiError>(acc + convert_quantity(qty, &row.unit, &rule.unit, product)?)
    })?;
    Ok(total * Decimal::from(lead_time_days) / Decimal::from(HISTORY_DAYS))
}

async fn preferred_supplier_item(
    state: &AppState,
    household_id: Uuid,
    rule: &ReplenishmentRuleRow,
) -> ApiResult<(String, String)> {
    if let (Some(supplier_id), Some(item_id)) = (
        rule.preferred_supplier_id.clone(),
        rule.preferred_supplier_item_id.clone(),
    ) {
        return Ok((supplier_id, item_id));
    }
    let mapping = replenishment::find_confirmed_mapping_for_product(
        &state.db,
        household_id,
        rule.product_id,
        rule.preferred_supplier_id.as_deref(),
    )
    .await?
    .ok_or_else(|| ApiError::BadRequest("missing confirmed supplier mapping".into()))?;
    Ok((mapping.supplier_id, mapping.supplier_item_id))
}

async fn catalog_item(
    state: &AppState,
    supplier_id: &str,
    supplier_item_id: &str,
) -> ApiResult<CachedCatalogItem> {
    if let Some(row) =
        suppliers::find_catalog_item(&state.db, supplier_id, supplier_item_id).await?
    {
        return Ok(CachedCatalogItem {
            price_amount: row.price_amount,
            price_currency: row.price_currency,
            pack_quantity: row.pack_quantity,
            pack_unit: row.pack_unit,
            lead_time_max_days: row.lead_time_max_days.or(row.lead_time_min_days),
        });
    }
    if supplier_id != suppliers::SUPPLIER_MOCK {
        return Ok(CachedCatalogItem::default());
    }
    let item = mock_supplier()
        .item_detail(supplier_item_id)
        .await
        .map_err(supplier_error)?;
    let row = persist_catalog_item(state, &item).await?;
    Ok(CachedCatalogItem {
        price_amount: row.price_amount,
        price_currency: row.price_currency,
        pack_quantity: row.pack_quantity,
        pack_unit: row.pack_unit,
        lead_time_max_days: row.lead_time_max_days.or(row.lead_time_min_days),
    })
}

#[derive(Default)]
struct CachedCatalogItem {
    price_amount: Option<String>,
    price_currency: Option<String>,
    pack_quantity: Option<String>,
    pack_unit: Option<String>,
    lead_time_max_days: Option<i64>,
}

fn order_quantity(
    rule: &ReplenishmentRuleRow,
    product: &qm_db::products::ProductRow,
    deficit: Decimal,
    catalog_pack_quantity: Option<&str>,
    catalog_pack_unit: Option<&str>,
) -> ApiResult<(Decimal, Option<String>)> {
    let package_quantity = rule
        .preferred_package_quantity
        .as_deref()
        .or(catalog_pack_quantity);
    let Some(package_quantity) = package_quantity else {
        return Ok((deficit, Some(rule.unit.clone())));
    };
    let package_unit = rule
        .preferred_package_unit
        .as_deref()
        .or(catalog_pack_unit)
        .unwrap_or(rule.unit.as_str());
    let package_qty = parse_decimal(package_quantity, "preferred_package_quantity")?;
    let package_in_rule_unit = convert_quantity(package_qty, package_unit, &rule.unit, product)?;
    if package_in_rule_unit <= Decimal::ZERO {
        return Err(ApiError::BadRequest(
            "preferred_package_quantity must be positive".into(),
        ));
    }
    let packages = (deficit / package_in_rule_unit).ceil().max(Decimal::ONE);
    Ok((packages, Some("piece".into())))
}

async fn guardrail_snapshot_for_lines(
    state: &AppState,
    household_id: Uuid,
    settings: &ReplenishmentSettingsRow,
    supplier_id: Option<&str>,
    lines: &[GeneratedLine],
) -> ApiResult<Value> {
    let total = lines.iter().map(|line| line.estimated_price).try_fold(
        Some(Decimal::ZERO),
        |acc, price| match (acc, price) {
            (Some(total), Some(price)) => Ok(Some(total + price)),
            _ => Ok::<_, ApiError>(None),
        },
    )?;
    let currency = lines
        .iter()
        .find_map(|line| line.estimated_currency.as_deref())
        .unwrap_or("USD")
        .to_owned();
    let mut reasons = Vec::new();
    if settings.global_disabled {
        reasons.push("global_disabled");
    }
    let policy = match supplier_id {
        Some(supplier_id) => {
            replenishment::find_supplier_policy(&state.db, household_id, supplier_id).await?
        }
        None => None,
    };
    if policy.as_ref().is_some_and(|policy| policy.disabled) {
        reasons.push("supplier_disabled");
    }
    let cap = policy
        .as_ref()
        .and_then(|policy| policy.spend_cap_amount.as_deref())
        .or(settings.default_spend_cap_amount.as_deref());
    if let (Some(total), Some(cap)) = (total, cap) {
        if total > parse_decimal(cap, "spend cap")? {
            reasons.push("budget_exceeded");
        }
    }
    if total.is_none()
        && lines
            .iter()
            .any(|line| line.rule.automation_level == replenishment::AUTOMATION_TRUSTED_AUTO_SUBMIT)
    {
        reasons.push("unknown_price");
    }
    let all_trusted = !lines.is_empty()
        && lines.iter().all(|line| {
            line.rule.automation_level == replenishment::AUTOMATION_TRUSTED_AUTO_SUBMIT
        });
    let decision = if !reasons.is_empty() {
        replenishment::GUARDRAIL_BLOCKED
    } else if all_trusted {
        replenishment::GUARDRAIL_ALLOWED
    } else {
        replenishment::GUARDRAIL_NEEDS_APPROVAL
    };
    Ok(json!({
        "decision": decision,
        "reasons": reasons,
        "estimated_total_amount": total.map(decimal_string),
        "estimated_total_currency": currency,
        "all_rules_trusted": all_trusted,
        "line_count": lines.len()
    }))
}

fn recommendations_for(lines: &[GeneratedLine]) -> Vec<RecommendationAudit> {
    lines
        .iter()
        .map(|line| RecommendationAudit {
            rule_id: line.rule.id,
            product_id: line.rule.product_id,
            supplier_id: line.supplier_id.clone(),
            supplier_item_id: line.supplier_item_id.clone(),
            quantity: decimal_string(line.quantity),
            unit: line.unit.clone(),
            estimated_price_amount: line.estimated_price.map(decimal_string),
            estimated_price_currency: line.estimated_currency.clone(),
            automation_level: line.rule.automation_level.clone(),
        })
        .collect()
}

fn suppression_reason_from_error(err: &ApiError) -> ReplenishmentSuppressionReasonDto {
    let message = err.to_string();
    if message.contains("expiring stock") {
        ReplenishmentSuppressionReasonDto::ExpiringStockAvailable
    } else if message.contains("missing confirmed supplier mapping") {
        ReplenishmentSuppressionReasonDto::MissingSupplierMapping
    } else {
        ReplenishmentSuppressionReasonDto::InvalidRule
    }
}

async fn validate_rule_request(
    state: &AppState,
    household_id: Uuid,
    req: &ReplenishmentRuleRequest,
) -> ApiResult<()> {
    let product = load_product_for_write(state, household_id, req.product_id).await?;
    validate_quantity_for_product(&product, &req.minimum_quantity, &req.unit)?;
    validate_quantity_for_product(&product, &req.target_quantity, &req.unit)?;
    if parse_decimal(&req.target_quantity, "target_quantity")?
        < parse_decimal(&req.minimum_quantity, "minimum_quantity")?
    {
        return Err(ApiError::BadRequest(
            "target_quantity must be greater than or equal to minimum_quantity".into(),
        ));
    }
    if let Some(location_id) = req.location_id {
        ensure_location(state, household_id, location_id).await?;
    }
    if let Some(days) = req.expiry_suppression_days {
        if days < 0 {
            return Err(ApiError::BadRequest(
                "expiry_suppression_days must not be negative".into(),
            ));
        }
    }
    validate_spend_cap(
        req.spend_cap_amount.as_deref(),
        req.spend_cap_currency.as_deref(),
    )?;
    if req.preferred_supplier_id.is_none() && req.preferred_supplier_item_id.is_some() {
        return Err(ApiError::BadRequest(
            "preferred_supplier_id is required with preferred_supplier_item_id".into(),
        ));
    }
    if let (Some(quantity), Some(unit)) = (
        req.preferred_package_quantity.as_deref(),
        req.preferred_package_unit.as_deref(),
    ) {
        validate_quantity_for_product(&product, quantity, unit)?;
    }
    Ok(())
}

async fn load_product_for_write(
    state: &AppState,
    household_id: Uuid,
    product_id: Uuid,
) -> ApiResult<qm_db::products::ProductRow> {
    qm_db::products::find_for_household(&state.db, household_id, product_id)
        .await?
        .ok_or(ApiError::NotFound)
}

async fn ensure_location(state: &AppState, household_id: Uuid, location_id: Uuid) -> ApiResult<()> {
    qm_db::locations::find(&state.db, household_id, location_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(())
}

fn validate_quantity_for_product(
    product: &qm_db::products::ProductRow,
    quantity: &str,
    unit: &str,
) -> ApiResult<()> {
    let quantity = parse_decimal(quantity, "quantity")?;
    if quantity <= Decimal::ZERO {
        return Err(ApiError::BadRequest("quantity must be positive".into()));
    }
    let unit = units::lookup(unit).map_err(|err| ApiError::BadRequest(err.to_string()))?;
    let family = UnitFamily::from_str_ci(&product.family)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("unknown product unit family")))?;
    if unit.family != family {
        return Err(ApiError::BadRequest(format!(
            "unit {} is incompatible with product family {}",
            unit.code, product.family
        )));
    }
    Ok(())
}

fn convert_quantity(
    quantity: Decimal,
    from: &str,
    to: &str,
    product: &qm_db::products::ProductRow,
) -> ApiResult<Decimal> {
    validate_quantity_for_product(product, "1", from)?;
    validate_quantity_for_product(product, "1", to)?;
    units::convert(quantity, from, to).map_err(|err| ApiError::BadRequest(err.to_string()))
}

fn parse_decimal(value: &str, field: &str) -> ApiResult<Decimal> {
    Decimal::from_str(value.trim())
        .map_err(|_| ApiError::BadRequest(format!("{field} must be a decimal number")))
}

fn decimal_string(value: Decimal) -> String {
    value.normalize().to_string()
}

fn validate_spend_cap(amount: Option<&str>, currency: Option<&str>) -> ApiResult<()> {
    if let Some(amount) = amount {
        if parse_decimal(amount, "spend cap")? < Decimal::ZERO {
            return Err(ApiError::BadRequest(
                "spend cap must not be negative".into(),
            ));
        }
        if currency.is_none() {
            return Err(ApiError::BadRequest(
                "spend cap currency is required with spend cap amount".into(),
            ));
        }
    }
    Ok(())
}

fn validate_quiet_hours(start: Option<&str>, end: Option<&str>) -> ApiResult<()> {
    for value in [start, end].into_iter().flatten() {
        let valid = value.len() == 5
            && value.as_bytes()[2] == b':'
            && value[..2].parse::<u8>().is_ok_and(|hour| hour < 24)
            && value[3..].parse::<u8>().is_ok_and(|minute| minute < 60);
        if !valid {
            return Err(ApiError::BadRequest(
                "quiet hours must be HH:MM in 24-hour time".into(),
            ));
        }
    }
    Ok(())
}

fn validate_date(value: &str) -> ApiResult<Date> {
    Date::from_str(value)
        .map_err(|_| ApiError::BadRequest(format!("date must be YYYY-MM-DD (got {value})")))
}

fn household_today(timezone: &str) -> ApiResult<Date> {
    let time_zone = tz::db()
        .get(timezone)
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("invalid household timezone")))?;
    Ok(Timestamp::now().to_zoned(time_zone).date())
}

async fn ensure_optional_supplier(state: &AppState, supplier_id: Option<&str>) -> ApiResult<()> {
    ensure_mock_supplier(state).await?;
    if let Some(supplier_id) = supplier_id {
        suppliers::find_supplier(&state.db, supplier_id)
            .await?
            .ok_or(ApiError::NotFound)?;
    }
    Ok(())
}

async fn ensure_mock_supplier(state: &AppState) -> ApiResult<()> {
    let descriptor = mock_supplier().descriptor();
    let capabilities_json =
        serde_json::to_string(&descriptor.capabilities).map_err(internal_json)?;
    let requirements_json =
        serde_json::to_string(&descriptor.requirements).map_err(internal_json)?;
    let regions_json =
        serde_json::to_string(&descriptor.supported_regions).map_err(internal_json)?;
    suppliers::upsert_supplier(
        &state.db,
        &NewSupplier {
            id: descriptor.id.as_str(),
            display_name: &descriptor.display_name,
            capabilities_json: &capabilities_json,
            requirements_json: &requirements_json,
            supported_regions_json: &regions_json,
            terms_url: descriptor.terms_url.as_deref(),
            needs_network: descriptor.needs_network,
            needs_browser: descriptor.needs_browser,
        },
    )
    .await?;
    Ok(())
}

fn mock_supplier() -> MockSupplierIntegration {
    MockSupplierIntegration::demo()
}

async fn persist_catalog_item(
    state: &AppState,
    item: &CatalogItem,
) -> ApiResult<qm_db::suppliers::SupplierCatalogItemRow> {
    suppliers::upsert_catalog_item(
        &state.db,
        &NewCatalogItem {
            supplier_id: item.supplier_id.as_str(),
            supplier_item_id: &item.supplier_item_id,
            name: &item.name,
            brand: item.brand.as_deref(),
            image_url: item.image_url.as_deref(),
            detail_url: item.detail_url.as_deref(),
            availability: availability_str(&item.availability),
            price_amount: item.price.as_ref().map(|price| price.amount.as_str()),
            price_currency: item.price.as_ref().map(|price| price.currency.as_str()),
            pack_quantity: item.pack_size.as_ref().map(|pack| pack.quantity.as_str()),
            pack_unit: item.pack_size.as_ref().map(|pack| pack.unit.as_str()),
            lead_time_min_days: item.lead_time.as_ref().map(|lead| lead.min_days),
            lead_time_max_days: item.lead_time.as_ref().and_then(|lead| lead.max_days),
            minimum_order_quantity: item
                .minimum_order_quantity
                .as_ref()
                .map(|moq| moq.quantity.as_str()),
            minimum_order_unit: item
                .minimum_order_quantity
                .as_ref()
                .map(|moq| moq.unit.as_str()),
            metadata_json: &json_string(&item.metadata)?,
        },
    )
    .await
    .map_err(ApiError::from)
}

fn rule_dto(row: ReplenishmentRuleRow) -> ApiResult<ReplenishmentRuleDto> {
    Ok(ReplenishmentRuleDto {
        id: row.id,
        product_id: row.product_id,
        location_id: row.location_id,
        minimum_quantity: row.minimum_quantity,
        target_quantity: row.target_quantity,
        unit: row.unit,
        preferred_supplier_id: row.preferred_supplier_id,
        preferred_supplier_item_id: row.preferred_supplier_item_id,
        preferred_package_quantity: row.preferred_package_quantity,
        preferred_package_unit: row.preferred_package_unit,
        automation_level: ReplenishmentAutomationLevelDto::from_str(&row.automation_level)?,
        expiry_suppression_days: row.expiry_suppression_days,
        paused_at: row.paused_at,
        pause_reason: row.pause_reason,
        spend_cap_amount: row.spend_cap_amount,
        spend_cap_currency: row.spend_cap_currency,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn settings_dto(row: ReplenishmentSettingsRow) -> ReplenishmentSettingsDto {
    ReplenishmentSettingsDto {
        global_disabled: row.global_disabled,
        default_spend_cap_amount: row.default_spend_cap_amount,
        default_spend_cap_currency: row.default_spend_cap_currency,
        notification_lead_minutes: row.notification_lead_minutes,
        quiet_hours_start: row.quiet_hours_start,
        quiet_hours_end: row.quiet_hours_end,
        updated_at: row.updated_at,
    }
}

fn supplier_policy_dto(row: ReplenishmentSupplierPolicyRow) -> ReplenishmentSupplierPolicyDto {
    ReplenishmentSupplierPolicyDto {
        supplier_id: row.supplier_id,
        disabled: row.disabled,
        spend_cap_amount: row.spend_cap_amount,
        spend_cap_currency: row.spend_cap_currency,
        quiet_hours_start: row.quiet_hours_start,
        quiet_hours_end: row.quiet_hours_end,
        updated_at: row.updated_at,
    }
}

fn demand_signal_dto(row: ReplenishmentDemandSignalRow) -> ApiResult<ReplenishmentDemandSignalDto> {
    Ok(ReplenishmentDemandSignalDto {
        id: row.id,
        product_id: row.product_id,
        location_id: row.location_id,
        signal_type: ReplenishmentDemandSignalTypeDto::from_str(&row.signal_type)?,
        status: ReplenishmentDemandSignalStatusDto::from_str(&row.status)?,
        quantity: row.quantity,
        unit: row.unit,
        recipe_id: row.recipe_id,
        recipe_version_id: row.recipe_version_id,
        desired_on: row.desired_on,
        supplier_id: row.supplier_id,
        supplier_item_id: row.supplier_item_id,
        note: row.note,
        metadata: parse_json(&row.metadata_json)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn cart_run_dto(row: ReplenishmentCartRunRow) -> ApiResult<ReplenishmentCartRunDto> {
    Ok(ReplenishmentCartRunDto {
        id: row.id,
        draft_id: row.draft_id,
        order_id: row.order_id,
        supplier_id: row.supplier_id,
        status: ReplenishmentCartRunStatusDto::from_str(&row.status)?,
        source: row.source,
        guardrail_decision: ReplenishmentGuardrailDecisionDto::from_str(&row.guardrail_decision)?,
        guardrail_snapshot: parse_json(&row.guardrail_snapshot_json)?,
        recommendations: parse_json(&row.recommendations_json)?,
        suppressions: parse_json(&row.suppressions_json)?,
        ai_explanation: row
            .ai_explanation_json
            .as_deref()
            .map(parse_json)
            .transpose()?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn availability_str(value: &Availability) -> &'static str {
    match value {
        Availability::InStock => "in_stock",
        Availability::Limited => "limited",
        Availability::Unavailable => "unavailable",
        Availability::Unknown => "unknown",
    }
}

fn cart_status_str(value: CartStatus) -> &'static str {
    match value {
        CartStatus::Draft => "draft",
        CartStatus::NeedsReview => "needs_review",
        CartStatus::Ready => "ready",
        CartStatus::Submitted => "submitted",
        CartStatus::Cancelled => "cancelled",
    }
}

fn intervention_str(value: InterventionState) -> &'static str {
    match value {
        InterventionState::None => "none",
        InterventionState::ConsentRequired => "consent_required",
        InterventionState::LoginRequired => "login_required",
        InterventionState::BrowserHandoffRequired => "browser_handoff_required",
        InterventionState::ManualHandoffRequired => "manual_handoff_required",
    }
}

fn json_string(value: &Value) -> ApiResult<String> {
    serde_json::to_string(value).map_err(internal_json)
}

fn json_string_ser<T: Serialize>(value: &T) -> ApiResult<String> {
    serde_json::to_string(value).map_err(internal_json)
}

fn parse_json(value: &str) -> ApiResult<Value> {
    serde_json::from_str(value).map_err(|err| ApiError::Internal(err.into()))
}

fn internal_json(err: serde_json::Error) -> ApiError {
    ApiError::Internal(err.into())
}

fn supplier_error(err: qm_suppliers::SupplierError) -> ApiError {
    match err {
        qm_suppliers::SupplierError::NotConfigured => {
            ApiError::ServiceUnavailable(err.redacted_message())
        }
        qm_suppliers::SupplierError::Unsupported(_) => ApiError::BadRequest(err.redacted_message()),
        qm_suppliers::SupplierError::InterventionRequired(_) => {
            ApiError::BadRequest(err.redacted_message())
        }
        qm_suppliers::SupplierError::Timeout
        | qm_suppliers::SupplierError::RateLimited
        | qm_suppliers::SupplierError::CircuitOpen
        | qm_suppliers::SupplierError::Transient { .. } => {
            ApiError::ServiceUnavailable(err.redacted_message())
        }
        qm_suppliers::SupplierError::Permanent { .. } => {
            ApiError::BadRequest(err.redacted_message())
        }
    }
}

macro_rules! dto_from_str {
    ($ty:ty, {$($value:literal => $variant:expr),+ $(,)?}, $label:literal) => {
        impl FromStr for $ty {
            type Err = ApiError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($value => Ok($variant),)+
                    other => Err(ApiError::Internal(anyhow::anyhow!(
                        "unknown {} in DB row: {other}",
                        $label
                    ))),
                }
            }
        }
    };
}

impl ReplenishmentAutomationLevelDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::Off => replenishment::AUTOMATION_OFF,
            Self::Suggestions => replenishment::AUTOMATION_SUGGESTIONS,
            Self::ConfirmToSubmit => replenishment::AUTOMATION_CONFIRM_TO_SUBMIT,
            Self::TrustedAutoSubmit => replenishment::AUTOMATION_TRUSTED_AUTO_SUBMIT,
        }
    }
}

impl ReplenishmentDemandSignalTypeDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::ManualShopping => replenishment::DEMAND_SIGNAL_MANUAL_SHOPPING,
            Self::UpcomingRecipe => replenishment::DEMAND_SIGNAL_UPCOMING_RECIPE,
        }
    }
}

impl ReplenishmentDemandSignalStatusDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => replenishment::DEMAND_SIGNAL_ACTIVE,
            Self::Dismissed => replenishment::DEMAND_SIGNAL_DISMISSED,
            Self::Fulfilled => replenishment::DEMAND_SIGNAL_FULFILLED,
        }
    }
}

dto_from_str!(ReplenishmentAutomationLevelDto, {
    "off" => ReplenishmentAutomationLevelDto::Off,
    "suggestions" => ReplenishmentAutomationLevelDto::Suggestions,
    "confirm_to_submit" => ReplenishmentAutomationLevelDto::ConfirmToSubmit,
    "trusted_auto_submit" => ReplenishmentAutomationLevelDto::TrustedAutoSubmit,
}, "replenishment automation level");

dto_from_str!(ReplenishmentDemandSignalTypeDto, {
    "manual_shopping" => ReplenishmentDemandSignalTypeDto::ManualShopping,
    "upcoming_recipe" => ReplenishmentDemandSignalTypeDto::UpcomingRecipe,
}, "replenishment demand signal type");

dto_from_str!(ReplenishmentDemandSignalStatusDto, {
    "active" => ReplenishmentDemandSignalStatusDto::Active,
    "dismissed" => ReplenishmentDemandSignalStatusDto::Dismissed,
    "fulfilled" => ReplenishmentDemandSignalStatusDto::Fulfilled,
}, "replenishment demand signal status");

dto_from_str!(ReplenishmentGuardrailDecisionDto, {
    "allowed" => ReplenishmentGuardrailDecisionDto::Allowed,
    "needs_approval" => ReplenishmentGuardrailDecisionDto::NeedsApproval,
    "blocked" => ReplenishmentGuardrailDecisionDto::Blocked,
}, "replenishment guardrail decision");

dto_from_str!(ReplenishmentCartRunStatusDto, {
    "draft_created" => ReplenishmentCartRunStatusDto::DraftCreated,
    "blocked" => ReplenishmentCartRunStatusDto::Blocked,
    "submitted" => ReplenishmentCartRunStatusDto::Submitted,
}, "replenishment cart run status");
