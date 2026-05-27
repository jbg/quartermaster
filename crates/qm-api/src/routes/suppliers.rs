use std::str::FromStr;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use qm_db::suppliers::{
    self, NewCartDraft, NewCartLine, NewCatalogItem, NewMapping, NewOrder, NewSupplier,
    NewSupplierAccount, ProductSupplierMappingRow, SupplierAccountRow, SupplierAccountSecretRow,
    SupplierCartDraftRow, SupplierCartLineRow, SupplierCatalogItemRow, SupplierOrderRow,
};
use qm_suppliers::{
    Availability, CartDraft, CartLine, CartStatus, CatalogItem, CatalogSearchRequest,
    InterventionState, MockSupplierIntegration, OrderStatus, SupplierCapability,
    SupplierDescriptor, SupplierId, SupplierIntegration,
};
use rand::RngExt;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierSubmitMode {
    ManualApproval,
    TrustedAuto,
}

pub async fn submit_cart_draft_internal(
    db: &qm_db::Database,
    household_id: Uuid,
    actor_id: Uuid,
    id: Uuid,
    mode: SupplierSubmitMode,
) -> ApiResult<SupplierOrderRow> {
    let (draft, lines) = suppliers::find_cart_draft(db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    validate_replenishment_submission(db, household_id, &draft, mode).await?;
    let submission = mock_supplier()
        .submit_order(CartDraft {
            id: draft.id,
            supplier_id: SupplierId::new(draft.supplier_id.clone()),
            lines: lines
                .iter()
                .map(|line| CartLine {
                    supplier_item_id: line.supplier_item_id.clone(),
                    product_id: line.product_id,
                    quantity: line.quantity.clone(),
                    unit: line.unit.clone(),
                    note: line.note.clone(),
                })
                .collect(),
            status: CartStatus::Ready,
            intervention: intervention_state(&draft.intervention_state)?,
        })
        .await
        .map_err(supplier_error)?;
    let summary = json_string(&submission.raw_summary)?;
    let order = suppliers::create_order(
        db,
        household_id,
        actor_id,
        &NewOrder {
            draft_id: Some(draft.id),
            account_id: draft.account_id,
            supplier_id: &draft.supplier_id,
            supplier_order_id: Some(&submission.supplier_order_id),
            status: order_status_str(submission.status),
            review_url: submission.review_url.as_deref(),
            redacted_summary_json: &summary,
            submitted_at: Some(&qm_db::now_utc_rfc3339()),
        },
    )
    .await?;
    if draft.source == qm_db::replenishment::CART_SOURCE_REPLENISHMENT {
        let snapshot = json_string(&json!({
            "decision": qm_db::replenishment::GUARDRAIL_ALLOWED,
            "mode": match mode {
                SupplierSubmitMode::ManualApproval => "manual_approval",
                SupplierSubmitMode::TrustedAuto => "trusted_auto",
            },
            "submitted_at": qm_db::now_utc_rfc3339()
        }))?;
        let _ = qm_db::replenishment::mark_cart_run_submitted(
            db,
            household_id,
            draft.id,
            order.id,
            &snapshot,
        )
        .await?;
    }
    Ok(order)
}

async fn validate_replenishment_submission(
    db: &qm_db::Database,
    household_id: Uuid,
    draft: &SupplierCartDraftRow,
    mode: SupplierSubmitMode,
) -> ApiResult<()> {
    if draft.source != qm_db::replenishment::CART_SOURCE_REPLENISHMENT {
        if mode == SupplierSubmitMode::TrustedAuto {
            return Err(ApiError::BadRequest(
                "trusted auto-submit is only available for replenishment drafts".into(),
            ));
        }
        return Ok(());
    }

    let run = qm_db::replenishment::find_cart_run_for_draft(db, household_id, draft.id)
        .await?
        .ok_or_else(|| {
            ApiError::BadRequest("replenishment draft is missing its audit run".into())
        })?;
    let settings = qm_db::replenishment::get_or_create_settings(db, household_id).await?;
    let policy =
        qm_db::replenishment::find_supplier_policy(db, household_id, &draft.supplier_id).await?;
    let recommendations = parse_json(&run.recommendations_json)?;
    let mut reasons = Vec::new();
    if settings.global_disabled {
        reasons.push("global_disabled");
    }
    if policy.as_ref().is_some_and(|policy| policy.disabled) {
        reasons.push("supplier_disabled");
    }
    if let Some(cap) = policy
        .as_ref()
        .and_then(|policy| policy.spend_cap_amount.as_deref())
        .or(settings.default_spend_cap_amount.as_deref())
    {
        if let Some(total) = recommendation_total(&recommendations)? {
            if total > parse_decimal(cap, "spend cap")? {
                reasons.push("budget_exceeded");
            }
        }
    }
    if mode == SupplierSubmitMode::TrustedAuto {
        if draft.status != qm_db::suppliers::CART_STATUS_READY {
            reasons.push("draft_not_ready");
        }
        if draft.intervention_state != "none" {
            reasons.push("human_intervention_required");
        }
        if !recommendations_all_trusted(&recommendations) {
            reasons.push("rules_not_trusted");
        }
        if recommendation_total(&recommendations)?.is_none() {
            reasons.push("unknown_price");
        }
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!(
            "supplier order submission blocked by replenishment guardrails: {}",
            reasons.join(", ")
        )))
    }
}

fn recommendation_total(value: &Value) -> ApiResult<Option<Decimal>> {
    let Some(items) = value.as_array() else {
        return Ok(Some(Decimal::ZERO));
    };
    let mut total = Decimal::ZERO;
    for item in items {
        let Some(amount) = item.get("estimated_price_amount").and_then(Value::as_str) else {
            return Ok(None);
        };
        total += parse_decimal(amount, "estimated_price_amount")?;
    }
    Ok(Some(total))
}

fn recommendations_all_trusted(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        !items.is_empty()
            && items.iter().all(|item| {
                item.get("automation_level").and_then(Value::as_str)
                    == Some(qm_db::replenishment::AUTOMATION_TRUSTED_AUTO_SUBMIT)
            })
    })
}

fn parse_decimal(value: &str, field: &str) -> ApiResult<Decimal> {
    Decimal::from_str(value.trim())
        .map_err(|_| ApiError::BadRequest(format!("{field} must be a decimal number")))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/suppliers/capabilities", get(capabilities))
        .route(
            "/suppliers/accounts",
            get(list_accounts).post(create_account),
        )
        .route(
            "/suppliers/accounts/{id}",
            get(get_account)
                .patch(update_account)
                .delete(delete_account),
        )
        .route(
            "/suppliers/accounts/{id}/secrets/{secret_name}",
            put(put_secret).delete(delete_secret),
        )
        .route("/suppliers/catalog/search", get(search_catalog))
        .route(
            "/suppliers/catalog/items/{supplier_item_id}",
            get(get_catalog_item),
        )
        .route(
            "/products/{product_id}/supplier-mappings",
            get(list_mappings).put(put_mapping),
        )
        .route(
            "/products/{product_id}/supplier-mappings/{mapping_id}",
            delete(delete_mapping),
        )
        .route("/suppliers/cart-drafts", post(create_cart_draft))
        .route(
            "/suppliers/cart-drafts/{id}",
            get(get_cart_draft).patch(patch_cart_draft),
        )
        .route(
            "/suppliers/cart-drafts/{id}/submit",
            post(submit_cart_draft),
        )
        .route("/suppliers/orders", get(list_orders))
        .route("/suppliers/orders/{id}", get(get_order))
        .route("/suppliers/orders/{id}/receive", post(receive_order))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierCapabilityDto {
    CatalogSearch,
    ItemDetail,
    CartDraft,
    OrderSubmit,
    OrderStatus,
    Cancellation,
    ReceivingHints,
    BrowserAutomation,
    ManualHandoff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierAccountStatusDto {
    Active,
    NeedsConfiguration,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierSecretKindDto {
    Password,
    ApiToken,
    CookieJar,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierMappingConfidenceDto {
    Suggested,
    Confirmed,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierCartStatusDto {
    Draft,
    NeedsReview,
    Ready,
    Submitted,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierOrderStatusDto {
    Draft,
    Submitted,
    Confirmed,
    InProgress,
    Delivered,
    Cancelled,
    Failed,
    HumanInterventionRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierInterventionStateDto {
    None,
    ConsentRequired,
    LoginRequired,
    BrowserHandoffRequired,
    ManualHandoffRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupplierDebugArtifactKindDto {
    Screenshot,
    Html,
    HttpExchange,
    Text,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierCapabilitiesResponse {
    pub suppliers: Vec<SupplierDescriptorDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierDescriptorDto {
    pub id: String,
    pub display_name: String,
    pub capabilities: Vec<SupplierCapabilityDto>,
    pub requirements: Value,
    pub supported_regions: Value,
    pub terms_url: Option<String>,
    pub needs_network: bool,
    pub needs_browser: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierAccountListResponse {
    pub items: Vec<SupplierAccountDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierAccountDto {
    pub id: Uuid,
    pub supplier_id: String,
    pub display_name: String,
    pub status: SupplierAccountStatusDto,
    pub region: Option<Value>,
    pub config: Value,
    pub consent_accepted_at: Option<String>,
    pub secrets: Vec<SupplierSecretDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierSecretDto {
    pub secret_name: String,
    pub secret_kind: SupplierSecretKindDto,
    pub redacted_hint: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierCreateAccountRequest {
    pub supplier_id: String,
    pub display_name: String,
    pub status: Option<SupplierAccountStatusDto>,
    pub region: Option<Value>,
    #[serde(default)]
    pub config: Value,
    pub consent_accepted_at: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierUpdateAccountRequest {
    pub display_name: String,
    pub status: SupplierAccountStatusDto,
    pub region: Option<Value>,
    #[serde(default)]
    pub config: Value,
    pub consent_accepted_at: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierPutSecretRequest {
    pub secret_kind: SupplierSecretKindDto,
    pub value: String,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct SupplierCatalogSearchQuery {
    pub supplier_id: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierCatalogSearchResponse {
    pub items: Vec<SupplierCatalogItemDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierCatalogItemDto {
    pub supplier_id: String,
    pub supplier_item_id: String,
    pub name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub detail_url: Option<String>,
    pub availability: String,
    pub price_amount: Option<String>,
    pub price_currency: Option<String>,
    pub pack_quantity: Option<String>,
    pub pack_unit: Option<String>,
    pub lead_time_min_days: Option<i64>,
    pub lead_time_max_days: Option<i64>,
    pub minimum_order_quantity: Option<String>,
    pub minimum_order_unit: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierMappingListResponse {
    pub items: Vec<SupplierMappingDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierMappingDto {
    pub id: Uuid,
    pub product_id: Uuid,
    pub supplier_id: String,
    pub supplier_item_id: String,
    pub confidence: SupplierMappingConfidenceDto,
    pub confirmed_at: Option<String>,
    pub substitute_policy: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierPutMappingRequest {
    pub supplier_id: String,
    pub supplier_item_id: String,
    pub confidence: SupplierMappingConfidenceDto,
    pub confirmed_at: Option<String>,
    #[serde(default)]
    pub substitute_policy: Value,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierCreateCartDraftRequest {
    pub account_id: Option<Uuid>,
    pub supplier_id: String,
    #[serde(default = "default_cart_source")]
    pub source: String,
    pub review_notes: Option<String>,
    pub lines: Vec<SupplierCreateCartLineRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierCreateCartLineRequest {
    pub product_id: Option<Uuid>,
    pub supplier_item_id: String,
    pub quantity: String,
    pub unit: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierPatchCartDraftRequest {
    pub status: SupplierCartStatusDto,
    pub intervention_state: SupplierInterventionStateDto,
    pub review_notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierCartDraftDto {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub supplier_id: String,
    pub status: SupplierCartStatusDto,
    pub source: String,
    pub intervention_state: SupplierInterventionStateDto,
    pub review_notes: Option<String>,
    pub lines: Vec<SupplierCartLineDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierCartLineDto {
    pub id: Uuid,
    pub product_id: Option<Uuid>,
    pub supplier_item_id: String,
    pub quantity: String,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierOrderListResponse {
    pub items: Vec<SupplierOrderDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SupplierOrderDto {
    pub id: Uuid,
    pub draft_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub supplier_id: String,
    pub supplier_order_id: Option<String>,
    pub status: SupplierOrderStatusDto,
    pub review_url: Option<String>,
    pub redacted_summary: Value,
    pub submitted_at: Option<String>,
    pub delivered_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierReceiveOrderRequest {
    pub lines: Vec<SupplierReceiveLineRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SupplierReceiveLineRequest {
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
    pub note: Option<String>,
}

#[utoipa::path(
    get,
    path = "/suppliers/capabilities",
    operation_id = "supplier_capabilities",
    tag = "suppliers",
    responses((status = 200, body = SupplierCapabilitiesResponse)),
    security(("bearer" = [])),
)]
async fn capabilities(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<SupplierCapabilitiesResponse>> {
    current.household_id.ok_or(ApiError::Forbidden)?;
    ensure_mock_supplier(&state).await?;
    Ok(Json(SupplierCapabilitiesResponse {
        suppliers: vec![descriptor_dto(mock_supplier().descriptor())],
    }))
}

#[utoipa::path(
    get,
    path = "/suppliers/accounts",
    operation_id = "supplier_account_list",
    tag = "suppliers",
    responses((status = 200, body = SupplierAccountListResponse)),
    security(("bearer" = [])),
)]
async fn list_accounts(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<SupplierAccountListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    ensure_mock_supplier(&state).await?;
    let rows = suppliers::list_accounts(&state.db, household_id).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(account_dto(&state, row).await?);
    }
    Ok(Json(SupplierAccountListResponse { items }))
}

#[utoipa::path(
    post,
    path = "/suppliers/accounts",
    operation_id = "supplier_account_create",
    tag = "suppliers",
    request_body = SupplierCreateAccountRequest,
    responses((status = 201, body = SupplierAccountDto)),
    security(("bearer" = [])),
)]
async fn create_account(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<SupplierCreateAccountRequest>,
) -> ApiResult<(StatusCode, Json<SupplierAccountDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    ensure_mock_supplier(&state).await?;
    validate_supplier(&state, &req.supplier_id).await?;
    let config_json = json_string(&req.config)?;
    let region_json = optional_json_string(req.region.as_ref())?;
    let row = suppliers::create_account(
        &state.db,
        household_id,
        current.user_id,
        &NewSupplierAccount {
            supplier_id: &req.supplier_id,
            display_name: req.display_name.trim(),
            status: req
                .status
                .unwrap_or(SupplierAccountStatusDto::NeedsConfiguration)
                .as_str(),
            region_json: region_json.as_deref(),
            config_json: &config_json,
            consent_accepted_at: req.consent_accepted_at.as_deref(),
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(account_dto(&state, row).await?)))
}

#[utoipa::path(
    get,
    path = "/suppliers/accounts/{id}",
    operation_id = "supplier_account_get",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = SupplierAccountDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn get_account(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SupplierAccountDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = suppliers::find_account(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(account_dto(&state, row).await?))
}

#[utoipa::path(
    patch,
    path = "/suppliers/accounts/{id}",
    operation_id = "supplier_account_update",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    request_body = SupplierUpdateAccountRequest,
    responses((status = 200, body = SupplierAccountDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn update_account(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SupplierUpdateAccountRequest>,
) -> ApiResult<Json<SupplierAccountDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let config_json = json_string(&req.config)?;
    let region_json = optional_json_string(req.region.as_ref())?;
    let row = suppliers::update_account(
        &state.db,
        household_id,
        id,
        current.user_id,
        req.display_name.trim(),
        req.status.as_str(),
        region_json.as_deref(),
        &config_json,
        req.consent_accepted_at.as_deref(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(account_dto(&state, row).await?))
}

#[utoipa::path(
    delete,
    path = "/suppliers/accounts/{id}",
    operation_id = "supplier_account_delete",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn delete_account(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    if suppliers::delete_account(&state.db, household_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    put,
    path = "/suppliers/accounts/{id}/secrets/{secret_name}",
    operation_id = "supplier_account_secret_put",
    tag = "suppliers",
    params(("id" = Uuid, Path), ("secret_name" = String, Path)),
    request_body = SupplierPutSecretRequest,
    responses((status = 200, body = SupplierSecretDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn put_secret(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((id, secret_name)): Path<(Uuid, String)>,
    Json(req): Json<SupplierPutSecretRequest>,
) -> ApiResult<Json<SupplierSecretDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let encrypted = encrypt_supplier_secret(&state, household_id, id, &secret_name, &req.value)?;
    let redacted_hint = redacted_hint(&req.value);
    let row = suppliers::upsert_account_secret(
        &state.db,
        household_id,
        id,
        &secret_name,
        req.secret_kind.as_str(),
        &encrypted,
        redacted_hint.as_deref(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(secret_dto(row)?))
}

#[utoipa::path(
    delete,
    path = "/suppliers/accounts/{id}/secrets/{secret_name}",
    operation_id = "supplier_account_secret_delete",
    tag = "suppliers",
    params(("id" = Uuid, Path), ("secret_name" = String, Path)),
    responses((status = 204), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn delete_secret(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((id, secret_name)): Path<(Uuid, String)>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    if suppliers::delete_account_secret(&state.db, household_id, id, &secret_name).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    get,
    path = "/suppliers/catalog/search",
    operation_id = "supplier_catalog_search",
    tag = "suppliers",
    params(SupplierCatalogSearchQuery),
    responses((status = 200, body = SupplierCatalogSearchResponse)),
    security(("bearer" = [])),
)]
async fn search_catalog(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<SupplierCatalogSearchQuery>,
) -> ApiResult<Json<SupplierCatalogSearchResponse>> {
    current.household_id.ok_or(ApiError::Forbidden)?;
    ensure_mock_supplier(&state).await?;
    let supplier_id = query
        .supplier_id
        .as_deref()
        .unwrap_or(suppliers::SUPPLIER_MOCK);
    if supplier_id != suppliers::SUPPLIER_MOCK {
        return Err(ApiError::NotFound);
    }
    let result = mock_supplier()
        .search_catalog(CatalogSearchRequest {
            query: query.q.unwrap_or_default(),
            region: None,
            limit: query.limit.unwrap_or(25).clamp(1, 100),
        })
        .await
        .map_err(supplier_error)?;
    let mut items = Vec::with_capacity(result.items.len());
    for item in result.items {
        let row = persist_catalog_item(&state, &item).await?;
        items.push(catalog_item_dto(row)?);
    }
    Ok(Json(SupplierCatalogSearchResponse { items }))
}

#[utoipa::path(
    get,
    path = "/suppliers/catalog/items/{supplier_item_id}",
    operation_id = "supplier_catalog_item_get",
    tag = "suppliers",
    params(("supplier_item_id" = String, Path), SupplierCatalogSearchQuery),
    responses((status = 200, body = SupplierCatalogItemDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn get_catalog_item(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(supplier_item_id): Path<String>,
    Query(query): Query<SupplierCatalogSearchQuery>,
) -> ApiResult<Json<SupplierCatalogItemDto>> {
    current.household_id.ok_or(ApiError::Forbidden)?;
    ensure_mock_supplier(&state).await?;
    let supplier_id = query
        .supplier_id
        .as_deref()
        .unwrap_or(suppliers::SUPPLIER_MOCK);
    if supplier_id != suppliers::SUPPLIER_MOCK {
        return Err(ApiError::NotFound);
    }
    let item = mock_supplier()
        .item_detail(&supplier_item_id)
        .await
        .map_err(supplier_error)?;
    let row = persist_catalog_item(&state, &item).await?;
    Ok(Json(catalog_item_dto(row)?))
}

#[utoipa::path(
    get,
    path = "/products/{product_id}/supplier-mappings",
    operation_id = "product_supplier_mapping_list",
    tag = "suppliers",
    params(("product_id" = Uuid, Path)),
    responses((status = 200, body = SupplierMappingListResponse)),
    security(("bearer" = [])),
)]
async fn list_mappings(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(product_id): Path<Uuid>,
) -> ApiResult<Json<SupplierMappingListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let items = suppliers::list_mappings_for_product(&state.db, household_id, product_id)
        .await?
        .into_iter()
        .map(mapping_dto)
        .collect::<ApiResult<_>>()?;
    Ok(Json(SupplierMappingListResponse { items }))
}

#[utoipa::path(
    put,
    path = "/products/{product_id}/supplier-mappings",
    operation_id = "product_supplier_mapping_put",
    tag = "suppliers",
    params(("product_id" = Uuid, Path)),
    request_body = SupplierPutMappingRequest,
    responses((status = 200, body = SupplierMappingDto)),
    security(("bearer" = [])),
)]
async fn put_mapping(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(product_id): Path<Uuid>,
    Json(req): Json<SupplierPutMappingRequest>,
) -> ApiResult<Json<SupplierMappingDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    validate_supplier(&state, &req.supplier_id).await?;
    if qm_db::products::find_by_id(&state.db, product_id)
        .await?
        .is_none()
    {
        return Err(ApiError::NotFound);
    }
    let substitute_policy_json = json_string(&req.substitute_policy)?;
    let row = suppliers::upsert_mapping(
        &state.db,
        household_id,
        current.user_id,
        &NewMapping {
            product_id,
            supplier_id: &req.supplier_id,
            supplier_item_id: &req.supplier_item_id,
            confidence: req.confidence.as_str(),
            confirmed_at: req.confirmed_at.as_deref(),
            substitute_policy_json: &substitute_policy_json,
        },
    )
    .await?;
    Ok(Json(mapping_dto(row)?))
}

#[utoipa::path(
    delete,
    path = "/products/{product_id}/supplier-mappings/{mapping_id}",
    operation_id = "product_supplier_mapping_delete",
    tag = "suppliers",
    params(("product_id" = Uuid, Path), ("mapping_id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn delete_mapping(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((product_id, mapping_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    if suppliers::delete_mapping(&state.db, household_id, product_id, mapping_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    post,
    path = "/suppliers/cart-drafts",
    operation_id = "supplier_cart_draft_create",
    tag = "suppliers",
    request_body = SupplierCreateCartDraftRequest,
    responses((status = 201, body = SupplierCartDraftDto)),
    security(("bearer" = [])),
)]
async fn create_cart_draft(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<SupplierCreateCartDraftRequest>,
) -> ApiResult<(StatusCode, Json<SupplierCartDraftDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    validate_supplier(&state, &req.supplier_id).await?;
    let supplier_cart = mock_supplier()
        .validate_cart(CartDraft {
            id: Uuid::now_v7(),
            supplier_id: SupplierId::new(req.supplier_id.clone()),
            lines: req
                .lines
                .iter()
                .map(|line| CartLine {
                    supplier_item_id: line.supplier_item_id.clone(),
                    product_id: line.product_id,
                    quantity: line.quantity.clone(),
                    unit: line.unit.clone(),
                    note: line.note.clone(),
                })
                .collect(),
            status: CartStatus::Draft,
            intervention: InterventionState::None,
        })
        .await
        .map_err(supplier_error)?;
    let lines = req
        .lines
        .iter()
        .map(|line| NewCartLine {
            product_id: line.product_id,
            supplier_item_id: &line.supplier_item_id,
            quantity: &line.quantity,
            unit: line.unit.as_deref(),
            note: line.note.as_deref(),
        })
        .collect();
    let (draft, lines) = suppliers::create_cart_draft(
        &state.db,
        household_id,
        current.user_id,
        &NewCartDraft {
            account_id: req.account_id,
            supplier_id: &req.supplier_id,
            status: cart_status_str(supplier_cart.status),
            source: &req.source,
            intervention_state: intervention_str(supplier_cart.intervention),
            review_notes: req.review_notes.as_deref(),
            lines,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(cart_draft_dto(draft, lines)?)))
}

#[utoipa::path(
    get,
    path = "/suppliers/cart-drafts/{id}",
    operation_id = "supplier_cart_draft_get",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = SupplierCartDraftDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn get_cart_draft(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SupplierCartDraftDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let (draft, lines) = suppliers::find_cart_draft(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(cart_draft_dto(draft, lines)?))
}

#[utoipa::path(
    patch,
    path = "/suppliers/cart-drafts/{id}",
    operation_id = "supplier_cart_draft_patch",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    request_body = SupplierPatchCartDraftRequest,
    responses((status = 200, body = SupplierCartDraftDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn patch_cart_draft(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SupplierPatchCartDraftRequest>,
) -> ApiResult<Json<SupplierCartDraftDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let (draft, lines) = suppliers::update_cart_status(
        &state.db,
        household_id,
        id,
        current.user_id,
        req.status.as_str(),
        req.intervention_state.as_str(),
        req.review_notes.as_deref(),
    )
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Json(cart_draft_dto(draft, lines)?))
}

#[utoipa::path(
    post,
    path = "/suppliers/cart-drafts/{id}/submit",
    operation_id = "supplier_cart_draft_submit",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    responses((status = 201, body = SupplierOrderDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn submit_cart_draft(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<(StatusCode, Json<SupplierOrderDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let order = submit_cart_draft_internal(
        &state.db,
        household_id,
        current.user_id,
        id,
        SupplierSubmitMode::ManualApproval,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(order_dto(order)?)))
}

#[utoipa::path(
    get,
    path = "/suppliers/orders",
    operation_id = "supplier_order_list",
    tag = "suppliers",
    responses((status = 200, body = SupplierOrderListResponse)),
    security(("bearer" = [])),
)]
async fn list_orders(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<SupplierOrderListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let items = suppliers::list_orders(&state.db, household_id)
        .await?
        .into_iter()
        .map(order_dto)
        .collect::<ApiResult<_>>()?;
    Ok(Json(SupplierOrderListResponse { items }))
}

#[utoipa::path(
    get,
    path = "/suppliers/orders/{id}",
    operation_id = "supplier_order_get",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = SupplierOrderDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn get_order(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SupplierOrderDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let order = suppliers::find_order(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(order_dto(order)?))
}

#[utoipa::path(
    post,
    path = "/suppliers/orders/{id}/receive",
    operation_id = "supplier_order_receive",
    tag = "suppliers",
    params(("id" = Uuid, Path)),
    request_body = SupplierReceiveOrderRequest,
    responses((status = 200, body = SupplierOrderDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
async fn receive_order(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SupplierReceiveOrderRequest>,
) -> ApiResult<Json<SupplierOrderDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    suppliers::find_order(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    for line in req.lines {
        qm_db::stock::create(
            &state.db,
            household_id,
            line.product_id,
            line.location_id,
            &line.quantity,
            &line.unit,
            None,
            line.expires_on.as_deref(),
            None,
            line.note.as_deref(),
            current.user_id,
            None,
        )
        .await?;
    }
    let order = suppliers::mark_order_delivered(&state.db, household_id, id, current.user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(order_dto(order)?))
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

async fn validate_supplier(state: &AppState, supplier_id: &str) -> ApiResult<()> {
    ensure_mock_supplier(state).await?;
    suppliers::find_supplier(&state.db, supplier_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(())
}

fn mock_supplier() -> MockSupplierIntegration {
    MockSupplierIntegration::demo()
}

async fn persist_catalog_item(
    state: &AppState,
    item: &CatalogItem,
) -> ApiResult<SupplierCatalogItemRow> {
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

async fn account_dto(state: &AppState, row: SupplierAccountRow) -> ApiResult<SupplierAccountDto> {
    let secrets = suppliers::list_account_secrets(&state.db, row.id)
        .await?
        .into_iter()
        .map(secret_dto)
        .collect::<ApiResult<_>>()?;
    Ok(SupplierAccountDto {
        id: row.id,
        supplier_id: row.supplier_id,
        display_name: row.display_name,
        status: SupplierAccountStatusDto::from_str(&row.status)?,
        region: parse_optional_json(row.region_json.as_deref())?,
        config: parse_json(&row.config_json)?,
        consent_accepted_at: row.consent_accepted_at,
        secrets,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn secret_dto(row: SupplierAccountSecretRow) -> ApiResult<SupplierSecretDto> {
    Ok(SupplierSecretDto {
        secret_name: row.secret_name,
        secret_kind: SupplierSecretKindDto::from_str(&row.secret_kind)?,
        redacted_hint: row.redacted_hint,
        updated_at: row.updated_at,
    })
}

fn descriptor_dto(descriptor: SupplierDescriptor) -> SupplierDescriptorDto {
    SupplierDescriptorDto {
        id: descriptor.id.0,
        display_name: descriptor.display_name,
        capabilities: descriptor
            .capabilities
            .into_iter()
            .map(capability_dto)
            .collect(),
        requirements: serde_json::to_value(descriptor.requirements).unwrap_or(Value::Null),
        supported_regions: serde_json::to_value(descriptor.supported_regions)
            .unwrap_or(Value::Null),
        terms_url: descriptor.terms_url,
        needs_network: descriptor.needs_network,
        needs_browser: descriptor.needs_browser,
    }
}

fn catalog_item_dto(row: SupplierCatalogItemRow) -> ApiResult<SupplierCatalogItemDto> {
    Ok(SupplierCatalogItemDto {
        supplier_id: row.supplier_id,
        supplier_item_id: row.supplier_item_id,
        name: row.name,
        brand: row.brand,
        image_url: row.image_url,
        detail_url: row.detail_url,
        availability: row.availability,
        price_amount: row.price_amount,
        price_currency: row.price_currency,
        pack_quantity: row.pack_quantity,
        pack_unit: row.pack_unit,
        lead_time_min_days: row.lead_time_min_days,
        lead_time_max_days: row.lead_time_max_days,
        minimum_order_quantity: row.minimum_order_quantity,
        minimum_order_unit: row.minimum_order_unit,
        metadata: parse_json(&row.metadata_json)?,
    })
}

fn mapping_dto(row: ProductSupplierMappingRow) -> ApiResult<SupplierMappingDto> {
    Ok(SupplierMappingDto {
        id: row.id,
        product_id: row.product_id,
        supplier_id: row.supplier_id,
        supplier_item_id: row.supplier_item_id,
        confidence: SupplierMappingConfidenceDto::from_str(&row.confidence)?,
        confirmed_at: row.confirmed_at,
        substitute_policy: parse_json(&row.substitute_policy_json)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn cart_draft_dto(
    draft: SupplierCartDraftRow,
    lines: Vec<SupplierCartLineRow>,
) -> ApiResult<SupplierCartDraftDto> {
    Ok(SupplierCartDraftDto {
        id: draft.id,
        account_id: draft.account_id,
        supplier_id: draft.supplier_id,
        status: SupplierCartStatusDto::from_str(&draft.status)?,
        source: draft.source,
        intervention_state: SupplierInterventionStateDto::from_str(&draft.intervention_state)?,
        review_notes: draft.review_notes,
        lines: lines
            .into_iter()
            .map(|line| SupplierCartLineDto {
                id: line.id,
                product_id: line.product_id,
                supplier_item_id: line.supplier_item_id,
                quantity: line.quantity,
                unit: line.unit,
                note: line.note,
                sort_order: line.sort_order,
            })
            .collect(),
        created_at: draft.created_at,
        updated_at: draft.updated_at,
    })
}

fn order_dto(row: SupplierOrderRow) -> ApiResult<SupplierOrderDto> {
    Ok(SupplierOrderDto {
        id: row.id,
        draft_id: row.draft_id,
        account_id: row.account_id,
        supplier_id: row.supplier_id,
        supplier_order_id: row.supplier_order_id,
        status: SupplierOrderStatusDto::from_str(&row.status)?,
        review_url: row.review_url,
        redacted_summary: parse_json(&row.redacted_summary_json)?,
        submitted_at: row.submitted_at,
        delivered_at: row.delivered_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn capability_dto(value: SupplierCapability) -> SupplierCapabilityDto {
    match value {
        SupplierCapability::CatalogSearch => SupplierCapabilityDto::CatalogSearch,
        SupplierCapability::ItemDetail => SupplierCapabilityDto::ItemDetail,
        SupplierCapability::CartDraft => SupplierCapabilityDto::CartDraft,
        SupplierCapability::OrderSubmit => SupplierCapabilityDto::OrderSubmit,
        SupplierCapability::OrderStatus => SupplierCapabilityDto::OrderStatus,
        SupplierCapability::Cancellation => SupplierCapabilityDto::Cancellation,
        SupplierCapability::ReceivingHints => SupplierCapabilityDto::ReceivingHints,
        SupplierCapability::BrowserAutomation => SupplierCapabilityDto::BrowserAutomation,
        SupplierCapability::ManualHandoff => SupplierCapabilityDto::ManualHandoff,
    }
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

fn order_status_str(value: OrderStatus) -> &'static str {
    match value {
        OrderStatus::Draft => "draft",
        OrderStatus::Submitted => "submitted",
        OrderStatus::Confirmed => "confirmed",
        OrderStatus::InProgress => "in_progress",
        OrderStatus::Delivered => "delivered",
        OrderStatus::Cancelled => "cancelled",
        OrderStatus::Failed => "failed",
        OrderStatus::HumanInterventionRequired => "human_intervention_required",
    }
}

fn intervention_state(value: &str) -> ApiResult<InterventionState> {
    Ok(match value {
        "none" => InterventionState::None,
        "consent_required" => InterventionState::ConsentRequired,
        "login_required" => InterventionState::LoginRequired,
        "browser_handoff_required" => InterventionState::BrowserHandoffRequired,
        "manual_handoff_required" => InterventionState::ManualHandoffRequired,
        other => {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "unknown supplier intervention state in DB row: {other}"
            )))
        }
    })
}

impl SupplierAccountStatusDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::NeedsConfiguration => "needs_configuration",
            Self::Disabled => "disabled",
        }
    }
}

impl FromStr for SupplierAccountStatusDto {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "needs_configuration" => Ok(Self::NeedsConfiguration),
            "disabled" => Ok(Self::Disabled),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown supplier account status in DB row: {other}"
            ))),
        }
    }
}

impl SupplierSecretKindDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::ApiToken => "api_token",
            Self::CookieJar => "cookie_jar",
            Self::Other => "other",
        }
    }
}

impl FromStr for SupplierSecretKindDto {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "password" => Ok(Self::Password),
            "api_token" => Ok(Self::ApiToken),
            "cookie_jar" => Ok(Self::CookieJar),
            "other" => Ok(Self::Other),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown supplier secret kind in DB row: {other}"
            ))),
        }
    }
}

impl SupplierMappingConfidenceDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::Suggested => "suggested",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}

impl FromStr for SupplierMappingConfidenceDto {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "suggested" => Ok(Self::Suggested),
            "confirmed" => Ok(Self::Confirmed),
            "rejected" => Ok(Self::Rejected),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown supplier mapping confidence in DB row: {other}"
            ))),
        }
    }
}

impl SupplierCartStatusDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::NeedsReview => "needs_review",
            Self::Ready => "ready",
            Self::Submitted => "submitted",
            Self::Cancelled => "cancelled",
        }
    }
}

impl FromStr for SupplierCartStatusDto {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "needs_review" => Ok(Self::NeedsReview),
            "ready" => Ok(Self::Ready),
            "submitted" => Ok(Self::Submitted),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown supplier cart status in DB row: {other}"
            ))),
        }
    }
}

impl FromStr for SupplierOrderStatusDto {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "submitted" => Ok(Self::Submitted),
            "confirmed" => Ok(Self::Confirmed),
            "in_progress" => Ok(Self::InProgress),
            "delivered" => Ok(Self::Delivered),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            "human_intervention_required" => Ok(Self::HumanInterventionRequired),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown supplier order status in DB row: {other}"
            ))),
        }
    }
}

impl SupplierInterventionStateDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ConsentRequired => "consent_required",
            Self::LoginRequired => "login_required",
            Self::BrowserHandoffRequired => "browser_handoff_required",
            Self::ManualHandoffRequired => "manual_handoff_required",
        }
    }
}

impl FromStr for SupplierInterventionStateDto {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" => Ok(Self::None),
            "consent_required" => Ok(Self::ConsentRequired),
            "login_required" => Ok(Self::LoginRequired),
            "browser_handoff_required" => Ok(Self::BrowserHandoffRequired),
            "manual_handoff_required" => Ok(Self::ManualHandoffRequired),
            other => Err(ApiError::Internal(anyhow::anyhow!(
                "unknown supplier intervention state in DB row: {other}"
            ))),
        }
    }
}

fn encrypt_supplier_secret(
    state: &AppState,
    household_id: Uuid,
    account_id: Uuid,
    secret_name: &str,
    value: &str,
) -> ApiResult<String> {
    let secret = state
        .config
        .supplier_credential_encryption_key
        .as_deref()
        .ok_or_else(|| {
            ApiError::ServiceUnavailable(
                "supplier credential storage is not configured on this server".into(),
            )
        })?;
    let digest = Sha256::digest(secret.as_bytes());
    let cipher = Aes256Gcm::new_from_slice(&digest)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("supplier cipher init: {err}")))?;
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill(&mut nonce_bytes);
    let aad = format!("{household_id}:{account_id}:{secret_name}");
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            aes_gcm::aead::Payload {
                msg: value.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("supplier credential encrypt: {err}")))?;
    let mut payload = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    Ok(STANDARD_NO_PAD.encode(payload))
}

fn redacted_hint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() <= 6 {
        Some("***".into())
    } else {
        Some(format!(
            "{}...{}",
            &trimmed[..3],
            &trimmed[trimmed.len().saturating_sub(3)..]
        ))
    }
}

fn default_cart_source() -> String {
    "manual".into()
}

fn parse_json(value: &str) -> ApiResult<Value> {
    serde_json::from_str(value)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("invalid supplier JSON: {err}")))
}

fn parse_optional_json(value: Option<&str>) -> ApiResult<Option<Value>> {
    value.map(parse_json).transpose()
}

fn json_string(value: &Value) -> ApiResult<String> {
    serde_json::to_string(value)
        .map_err(|err| ApiError::Internal(anyhow::anyhow!("serializing supplier JSON: {err}")))
}

fn optional_json_string(value: Option<&Value>) -> ApiResult<Option<String>> {
    value.map(json_string).transpose()
}

fn supplier_error(err: qm_suppliers::SupplierError) -> ApiError {
    match err {
        qm_suppliers::SupplierError::NotConfigured => {
            ApiError::ServiceUnavailable("supplier is not configured".into())
        }
        qm_suppliers::SupplierError::Unsupported(_) => {
            ApiError::BadRequest("supplier capability is not available".into())
        }
        qm_suppliers::SupplierError::InterventionRequired(state) => {
            ApiError::Conflict(format!("supplier requires human intervention: {state:?}"))
        }
        qm_suppliers::SupplierError::Timeout | qm_suppliers::SupplierError::CircuitOpen => {
            ApiError::ServiceUnavailable(err.redacted_message())
        }
        qm_suppliers::SupplierError::RateLimited => ApiError::RateLimited,
        qm_suppliers::SupplierError::Transient { .. } => {
            ApiError::ServiceUnavailable(err.redacted_message())
        }
        qm_suppliers::SupplierError::Permanent { .. } => {
            ApiError::BadRequest(err.redacted_message())
        }
    }
}

fn internal_json(err: serde_json::Error) -> ApiError {
    ApiError::Internal(anyhow::anyhow!("serializing supplier descriptor: {err}"))
}
