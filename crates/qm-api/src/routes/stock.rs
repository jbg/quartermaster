use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
    Json, Router,
};
use jiff::civil::Date;
use qm_core::batch::plan_consumption;
use qm_db::products::ProductRow;
use qm_db::stock::{
    RestoreError, StockBatchRow, StockBatchWithProduct, StockFilter, StockMetadataUpdate,
};
use qm_db::stock_events::TimelineEntryRow;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    rate_limit::RateLimitLayerState,
    routes::patch::{
        reject_remove, reject_value_for_remove, string_value, JsonPatchDocument, JsonPatchOperation,
    },
    routes::products::ProductDto,
    types::StockEventType,
    AppState,
};

pub fn router(rate_limit_state: RateLimitLayerState) -> Router<AppState> {
    Router::new()
        .route("/stock", get(list).post(create))
        .route("/stock/consume", post(consume))
        .route(
            "/stock/events",
            get(list_events).route_layer(middleware::from_fn_with_state(
                rate_limit_state.clone(),
                crate::rate_limit::enforce,
            )),
        )
        .route("/stock/restore-many", post(restore_many))
        .route("/stock/{id}", get(get_one).patch(update).delete(delete_one))
        .route(
            "/stock/{id}/events",
            get(list_events_for_batch).route_layer(middleware::from_fn_with_state(
                rate_limit_state,
                crate::rate_limit::enforce,
            )),
        )
        .route("/stock/{id}/restore", post(restore_one))
}

impl From<RestoreError> for ApiError {
    fn from(e: RestoreError) -> Self {
        match e {
            RestoreError::NotFound => ApiError::NotFound,
            RestoreError::NotRestorable => ApiError::BatchNotRestorable {
                unrestorable_ids: Vec::new(),
            },
            RestoreError::NotRestorableMany(ids) => ApiError::BatchNotRestorable {
                unrestorable_ids: ids,
            },
            RestoreError::Database(err) => ApiError::Database(err),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockBatchDto {
    pub id: Uuid,
    pub product: ProductDto,
    pub location_id: Uuid,
    pub location_name: String,
    pub initial_quantity: String,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub depleted_at: Option<String>,
}

impl TryFrom<StockBatchWithProduct> for StockBatchDto {
    type Error = ApiError;

    fn try_from(j: StockBatchWithProduct) -> Result<Self, Self::Error> {
        Ok(Self {
            id: j.batch.id,
            product: j.product.try_into()?,
            location_id: j.batch.location_id,
            location_name: j.location_name,
            initial_quantity: j.batch.initial_quantity,
            quantity: j.batch.quantity,
            unit: j.batch.unit,
            expires_on: j.batch.expires_on,
            opened_on: j.batch.opened_on,
            note: j.batch.note,
            created_at: j.batch.created_at,
            depleted_at: j.batch.depleted_at,
        })
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct StockListQuery {
    pub location_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    /// ISO-8601 date (YYYY-MM-DD). When set, only batches expiring strictly
    /// before this date are returned. Undated batches are excluded.
    pub expiring_before: Option<String>,
    /// When true, also include batches that have been fully consumed
    /// (`depleted_at IS NOT NULL`).
    pub include_depleted: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockListResponse {
    pub items: Vec<StockBatchDto>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateStockRequest {
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
}

pub type UpdateStockRequest = JsonPatchDocument;

#[derive(Debug, Default)]
struct StockPatch {
    quantity: Option<String>,
    location_id: Option<Uuid>,
    expires_on: Option<Option<String>>,
    opened_on: Option<Option<String>>,
    note: Option<Option<String>>,
}

impl StockPatch {
    fn parse(operations: Vec<JsonPatchOperation>) -> ApiResult<Self> {
        let mut patch = Self::default();
        for operation in operations {
            match operation.op.as_str() {
                "replace" => patch.replace(&operation.path, operation.value.as_ref())?,
                "remove" => patch.remove(&operation.path, operation.value.as_ref())?,
                other => {
                    return Err(ApiError::BadRequest(format!(
                        "unsupported JSON Patch operation: {other}"
                    )));
                }
            }
        }
        Ok(patch)
    }

    fn replace(&mut self, path: &str, value: Option<&serde_json::Value>) -> ApiResult<()> {
        match path {
            "/quantity" => self.quantity = Some(string_value("quantity", value)?),
            "/location_id" => {
                let value = string_value("location_id", value)?;
                self.location_id = Some(
                    Uuid::parse_str(&value)
                        .map_err(|_| ApiError::BadRequest("location_id must be a UUID".into()))?,
                );
            }
            "/expires_on" => self.expires_on = Some(Some(string_value("expires_on", value)?)),
            "/opened_on" => self.opened_on = Some(Some(string_value("opened_on", value)?)),
            "/note" => self.note = Some(Some(string_value("note", value)?)),
            other => {
                return Err(ApiError::BadRequest(format!(
                    "unknown stock patch path: {other}"
                )))
            }
        }
        Ok(())
    }

    fn remove(&mut self, path: &str, value: Option<&serde_json::Value>) -> ApiResult<()> {
        match path {
            "/expires_on" => {
                reject_value_for_remove("expires_on", value)?;
                self.expires_on = Some(None);
            }
            "/opened_on" => {
                reject_value_for_remove("opened_on", value)?;
                self.opened_on = Some(None);
            }
            "/note" => {
                reject_value_for_remove("note", value)?;
                self.note = Some(None);
            }
            "/quantity" => return Err(reject_remove("quantity")),
            "/location_id" => return Err(reject_remove("location_id")),
            other => {
                return Err(ApiError::BadRequest(format!(
                    "unknown stock patch path: {other}"
                )))
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConsumeRequest {
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub quantity: String,
    pub unit: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsumedBatchDto {
    pub batch_id: Uuid,
    /// Amount taken from this batch, in the batch's own unit.
    pub quantity: String,
    pub unit: String,
    /// Same amount, converted to the unit the caller requested in the
    /// `ConsumeRequest`. Lets the client display "200 ml consumed" even
    /// when the underlying batch stores litres.
    pub quantity_in_requested_unit: String,
    pub requested_unit: String,
    pub depleted: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsumeResponse {
    pub consumed: Vec<ConsumedBatchDto>,
    /// Correlates the `consume` events this call wrote to the ledger.
    pub consume_request_id: Uuid,
}

// ----- handlers -----

#[utoipa::path(
    get,
    path = "/stock",
    operation_id = "stock_list",
    tag = "stock",
    params(StockListQuery),
    responses(
        (status = 200, body = StockListResponse),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<StockListQuery>,
) -> ApiResult<Json<StockListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let expiring_before = q
        .expiring_before
        .as_deref()
        .map(Date::from_str)
        .transpose()
        .map_err(|_| ApiError::BadRequest("expiring_before must be YYYY-MM-DD".into()))?;

    let filter = StockFilter {
        location_id: q.location_id,
        product_id: q.product_id,
        expiring_before,
        include_depleted: q.include_depleted.unwrap_or(false),
        include_undated_when_expiring_filter: false,
    };

    let rows = qm_db::stock::list(&state.db, household_id, &filter).await?;
    let items: Vec<StockBatchDto> = rows
        .into_iter()
        .map(StockBatchDto::try_from)
        .collect::<ApiResult<_>>()?;
    Ok(Json(StockListResponse { items }))
}

#[utoipa::path(
    get,
    path = "/stock/{id}",
    operation_id = "stock_get",
    tag = "stock",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = StockBatchDto),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn get_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<StockBatchDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::stock::get_with_product(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(row.try_into()?))
}

#[utoipa::path(
    post,
    path = "/stock",
    operation_id = "stock_create",
    tag = "stock",
    request_body = CreateStockRequest,
    responses(
        (status = 201, body = StockBatchDto),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateStockRequest>,
) -> ApiResult<(StatusCode, Json<StockBatchDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;

    validate_positive_decimal(&req.quantity)?;
    let product = load_product_for_write(&state, household_id, req.product_id).await?;
    validate_unit_family(&req.unit, &product.family)?;
    validate_location(&state, household_id, req.location_id).await?;
    if let Some(d) = req.expires_on.as_deref() {
        validate_iso_date(d)?;
    }
    if let Some(d) = req.opened_on.as_deref() {
        validate_iso_date(d)?;
    }

    let row = qm_db::stock::create(
        &state.db,
        household_id,
        product.id,
        req.location_id,
        &req.quantity,
        &req.unit,
        req.expires_on.as_deref(),
        req.opened_on.as_deref(),
        req.note.as_deref(),
        current.user_id,
        Some(&state.config.expiry_reminder_policy),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(stock_dto(&state, household_id, row.id, false).await?),
    ))
}

#[utoipa::path(
    patch,
    path = "/stock/{id}",
    operation_id = "stock_update",
    tag = "stock",
    params(("id" = Uuid, Path)),
    request_body = Vec<JsonPatchOperation>,
    responses(
        (status = 200, body = StockBatchDto),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStockRequest>,
) -> ApiResult<Json<StockBatchDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let req = StockPatch::parse(req)?;

    let existing = qm_db::stock::get(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if existing.depleted_at.is_some() {
        return Err(ApiError::BadRequest(
            "depleted stock cannot be edited; restore it before editing".into(),
        ));
    }
    let _product = load_product_for_write(&state, household_id, existing.product_id).await?;

    let expires_on = req.expires_on.as_ref().map(|o| o.as_deref());
    let opened_on = req.opened_on.as_ref().map(|o| o.as_deref());
    let note = req.note.as_ref().map(|o| o.as_deref());

    if let Some(q) = req.quantity.as_deref() {
        // Allow quantity=0 via adjust (same as discard semantically). The
        // stricter `validate_positive_decimal` only applies at create time.
        Decimal::from_str(q)
            .map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?;
    }
    if let Some(loc) = req.location_id {
        validate_location(&state, household_id, loc).await?;
    }
    if let Some(d) = expires_on.flatten() {
        validate_iso_date(d)?;
    }
    if let Some(d) = opened_on.flatten() {
        validate_iso_date(d)?;
    }

    // Quantity changes funnel through `adjust` (writes an event); metadata
    // changes go through `update_metadata` (no event). Both can appear in
    // the same PATCH call.
    if let Some(qty) = req.quantity.as_deref() {
        qm_db::stock::adjust(
            &state.db,
            household_id,
            id,
            qty,
            current.user_id,
            None,
            Some(&state.config.expiry_reminder_policy),
        )
        .await?;
    }

    let metadata = StockMetadataUpdate {
        location_id: req.location_id,
        expires_on,
        opened_on,
        note,
    };
    let has_metadata = req.location_id.is_some()
        || req.expires_on.is_some()
        || req.opened_on.is_some()
        || req.note.is_some();
    if has_metadata {
        qm_db::stock::update_metadata(
            &state.db,
            household_id,
            id,
            &metadata,
            Some(&state.config.expiry_reminder_policy),
        )
        .await?;
    }

    Ok(Json(stock_dto(&state, household_id, id, false).await?))
}

#[utoipa::path(
    delete,
    path = "/stock/{id}",
    operation_id = "stock_delete",
    tag = "stock",
    params(("id" = Uuid, Path)),
    responses(
        (status = 204),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let found = qm_db::stock::discard(
        &state.db,
        household_id,
        id,
        current.user_id,
        None,
        Some(&state.config.expiry_reminder_policy),
    )
    .await?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    post,
    path = "/stock/consume",
    operation_id = "stock_consume",
    tag = "stock",
    request_body = ConsumeRequest,
    responses(
        (status = 200, body = ConsumeResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn consume(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<ConsumeRequest>,
) -> ApiResult<Json<ConsumeResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let product = load_product_for_write(&state, household_id, req.product_id).await?;
    validate_unit_family(&req.unit, &product.family)?;
    validate_positive_decimal(&req.quantity)?;

    let batches =
        qm_db::stock::list_active_batches(&state.db, household_id, product.id, req.location_id)
            .await?;
    let refs = batches
        .iter()
        .map(|b| b.to_batch_ref())
        .collect::<Result<Vec<_>, _>>()?;

    let quantity = Decimal::from_str(&req.quantity)
        .map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?;

    let plan = match plan_consumption(refs, quantity, &req.unit) {
        Ok(p) => p,
        Err(qm_core::QmError::InsufficientStock {
            requested,
            available,
        }) => {
            return Err(ApiError::InsufficientStock {
                requested: requested.to_string(),
                available: available.to_string(),
            });
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };

    let consume_request_id = qm_db::stock::apply_consumption(
        &state.db,
        household_id,
        &plan,
        current.user_id,
        Some(&state.config.expiry_reminder_policy),
    )
    .await?;

    // Zip the plan back with the batches so we can report the consumption
    // in both the batch's unit and the user's requested unit.
    let mut consumed = Vec::with_capacity(plan.len());
    for (c, batch) in plan.into_iter().zip(batches.iter()) {
        // qm_core::units::convert returns quantity in the `to` unit.
        let in_requested =
            qm_core::units::convert(c.quantity, &batch.unit, &req.unit).unwrap_or(c.quantity);
        consumed.push(ConsumedBatchDto {
            batch_id: c.batch_id,
            quantity: c.quantity.to_string(),
            unit: batch.unit.clone(),
            quantity_in_requested_unit: in_requested.to_string(),
            requested_unit: req.unit.clone(),
            depleted: c.depletes,
        });
    }

    Ok(Json(ConsumeResponse {
        consumed,
        consume_request_id,
    }))
}

// ----- history / restore -----

#[derive(Debug, Serialize, ToSchema)]
pub struct StockEventDto {
    pub id: Uuid,
    pub event_type: StockEventType,
    /// Signed decimal in `unit`.
    pub quantity_delta: String,
    pub unit: String,
    /// The batch's current expiry date (YYYY-MM-DD), if any. Lets clients
    /// render "expiring tomorrow" context on history rows.
    pub batch_expires_on: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub created_by_username: Option<String>,
    pub batch_id: Uuid,
    pub product: ProductDto,
    /// Shared by all rows written by a single `POST /api/v1/stock/consume` call.
    pub consume_request_id: Option<Uuid>,
}

impl TryFrom<TimelineEntryRow> for StockEventDto {
    type Error = ApiError;

    fn try_from(r: TimelineEntryRow) -> Result<Self, Self::Error> {
        let event_type: StockEventType = r.event.event_type.parse()?;
        Ok(Self {
            id: r.event.id,
            event_type,
            quantity_delta: r.event.quantity_delta,
            unit: r.batch_unit,
            batch_expires_on: r.batch_expires_on,
            note: r.event.note,
            created_at: r.event.created_at,
            created_by_username: r.created_by_username,
            batch_id: r.event.batch_id,
            product: r.product.try_into()?,
            consume_request_id: r.event.consume_request_id,
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockEventListResponse {
    pub items: Vec<StockEventDto>,
    /// Pagination cursor — the last returned item's `created_at` when a
    /// full page came back, otherwise `None`. Always pair with
    /// `next_before_id` to fetch the next page; the pair avoids the
    /// same-millisecond tiebreak problem that a timestamp alone can miss.
    pub next_before: Option<String>,
    /// Pagination cursor — the last returned item's `id` when a full
    /// page came back, otherwise `None`.
    pub next_before_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct EventListQuery {
    pub before_created_at: Option<String>,
    pub before_id: Option<Uuid>,
    pub limit: Option<i64>,
}

const DEFAULT_EVENT_LIMIT: i64 = 50;
const MAX_EVENT_LIMIT: i64 = 200;

#[utoipa::path(
    get,
    path = "/stock/events",
    operation_id = "stock_list_events",
    tag = "stock",
    params(EventListQuery),
    responses(
        (status = 200, body = StockEventListResponse),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list_events(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<EventListQuery>,
) -> ApiResult<Json<StockEventListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let limit = q
        .limit
        .unwrap_or(DEFAULT_EVENT_LIMIT)
        .clamp(1, MAX_EVENT_LIMIT);
    let rows = qm_db::stock_events::list_timeline(
        &state.db,
        household_id,
        None,
        q.before_created_at.as_deref(),
        q.before_id,
        limit,
    )
    .await?;
    Ok(Json(build_event_response(rows, limit)?))
}

#[utoipa::path(
    get,
    path = "/stock/{id}/events",
    operation_id = "stock_list_batch_events",
    tag = "stock",
    params(
        ("id" = Uuid, Path),
        EventListQuery,
    ),
    responses(
        (status = 200, body = StockEventListResponse),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list_events_for_batch(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<EventListQuery>,
) -> ApiResult<Json<StockEventListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    // Enforce access by the household-scoped batch lookup before querying events.
    qm_db::stock::get(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let limit = q
        .limit
        .unwrap_or(DEFAULT_EVENT_LIMIT)
        .clamp(1, MAX_EVENT_LIMIT);
    let rows = qm_db::stock_events::list_timeline(
        &state.db,
        household_id,
        Some(id),
        q.before_created_at.as_deref(),
        q.before_id,
        limit,
    )
    .await?;
    Ok(Json(build_event_response(rows, limit)?))
}

fn build_event_response(
    rows: Vec<TimelineEntryRow>,
    limit: i64,
) -> ApiResult<StockEventListResponse> {
    let (next_before, next_before_id) = if (rows.len() as i64) >= limit {
        rows.last()
            .map(|r| (Some(r.event.created_at.clone()), Some(r.event.id)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };
    let items: Vec<StockEventDto> = rows
        .into_iter()
        .map(StockEventDto::try_from)
        .collect::<ApiResult<_>>()?;
    Ok(StockEventListResponse {
        items,
        next_before,
        next_before_id,
    })
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RestoreManyRequest {
    /// Batch IDs to restore. All restored atomically: if any isn't
    /// recoverable (wasn't discarded, or already restored), the whole
    /// request rolls back with `batch_not_restorable`.
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RestoreManyResponse {
    pub restored: Vec<StockBatchDto>,
}

const MAX_RESTORE_MANY: usize = 100;

#[utoipa::path(
    post,
    path = "/stock/restore-many",
    operation_id = "stock_restore_many",
    tag = "stock",
    request_body = RestoreManyRequest,
    responses(
        (status = 200, body = RestoreManyResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn restore_many(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<RestoreManyRequest>,
) -> ApiResult<Json<RestoreManyResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    if req.ids.is_empty() {
        return Err(ApiError::BadRequest("ids must not be empty".into()));
    }
    if req.ids.len() > MAX_RESTORE_MANY {
        return Err(ApiError::BadRequest(format!(
            "too many ids in one request (max {MAX_RESTORE_MANY})",
        )));
    }

    let rows = qm_db::stock::restore_many(
        &state.db,
        household_id,
        &req.ids,
        current.user_id,
        Some(&state.config.expiry_reminder_policy),
    )
    .await?;

    // Join each batch with its product for the response DTO.
    let mut restored = Vec::with_capacity(rows.len());
    for row in rows {
        restored.push(stock_dto(&state, household_id, row.id, true).await?);
    }
    Ok(Json(RestoreManyResponse { restored }))
}

#[utoipa::path(
    post,
    path = "/stock/{id}/restore",
    operation_id = "stock_restore",
    tag = "stock",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = StockBatchDto),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn restore_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<StockBatchDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::stock::restore(
        &state.db,
        household_id,
        id,
        current.user_id,
        Some(&state.config.expiry_reminder_policy),
    )
    .await?;
    Ok(Json(stock_dto(&state, household_id, row.id, true).await?))
}

// ----- helpers -----

async fn stock_dto(
    state: &AppState,
    household_id: Uuid,
    id: Uuid,
    include_deleted_product: bool,
) -> ApiResult<StockBatchDto> {
    let row = if include_deleted_product {
        qm_db::stock::get_with_product_including_deleted(&state.db, household_id, id).await?
    } else {
        qm_db::stock::get_with_product(&state.db, household_id, id).await?
    };
    row.ok_or(ApiError::NotFound)?.try_into()
}

fn validate_positive_decimal(s: &str) -> ApiResult<()> {
    let q = Decimal::from_str(s)
        .map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?;
    if q <= Decimal::ZERO {
        return Err(ApiError::BadRequest(
            "quantity must be greater than zero".into(),
        ));
    }
    Ok(())
}

fn validate_iso_date(s: &str) -> ApiResult<()> {
    Date::from_str(s)
        .map(|_| ())
        .map_err(|_| ApiError::BadRequest(format!("date must be YYYY-MM-DD (got {s})")))
}

fn validate_unit_family(unit: &str, product_family: &str) -> ApiResult<()> {
    let u = qm_core::units::lookup(unit).map_err(|_| ApiError::UnknownUnit(unit.to_owned()))?;
    if u.family.as_str() != product_family {
        return Err(ApiError::UnitFamilyMismatch {
            product_family: product_family.to_owned(),
            unit: unit.to_owned(),
        });
    }
    Ok(())
}

async fn validate_location(
    state: &AppState,
    household_id: Uuid,
    location_id: Uuid,
) -> ApiResult<()> {
    qm_db::locations::find(&state.db, household_id, location_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(())
}

async fn load_product_for_write(
    state: &AppState,
    household_id: Uuid,
    product_id: Uuid,
) -> ApiResult<ProductRow> {
    let product = qm_db::products::find_by_id(&state.db, product_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if product.source == qm_db::products::SOURCE_MANUAL
        && product.created_by_household_id != Some(household_id)
    {
        return Err(ApiError::NotFound);
    }
    Ok(product)
}

#[allow(dead_code)]
type _KeepStockBatchRowInScope = StockBatchRow;
