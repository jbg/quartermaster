use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::NaiveDate;
use qm_core::batch::plan_consumption;
use qm_db::products::ProductRow;
use qm_db::stock::{StockBatchWithProduct, StockFilter, StockUpdate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    routes::products::ProductDto,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stock", get(list).post(create))
        .route("/stock/consume", post(consume))
        .route("/stock/{id}", get(get_one).patch(update).delete(delete_one))
}

/// Deserializer helper: treats a missing field as `None` and a present-null
/// field as `Some(None)`. Used on `UpdateStockRequest` optional-clearable
/// fields so clients can distinguish "don't touch" from "clear".
mod double_option {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Ok(Some(Option::<T>::deserialize(deserializer)?))
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StockBatchDto {
    pub id: Uuid,
    pub product: ProductDto,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

impl From<StockBatchWithProduct> for StockBatchDto {
    fn from(j: StockBatchWithProduct) -> Self {
        Self {
            id: j.batch.id,
            product: j.product.into(),
            location_id: j.batch.location_id,
            quantity: j.batch.quantity,
            unit: j.batch.unit,
            expires_on: j.batch.expires_on,
            opened_on: j.batch.opened_on,
            note: j.batch.note,
            created_at: j.batch.created_at,
        }
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateStockRequest {
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub location_id: Option<Uuid>,
    /// Use `null` inside a JSON field to explicitly clear the expiry date;
    /// omit the field entirely to leave it unchanged.
    #[serde(default, deserialize_with = "double_option::deserialize")]
    pub expires_on: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option::deserialize")]
    pub opened_on: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option::deserialize")]
    pub note: Option<Option<String>>,
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
    pub quantity: String,
    pub unit: String,
    pub depleted: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsumeResponse {
    pub consumed: Vec<ConsumedBatchDto>,
}

// ----- handlers -----

#[utoipa::path(
    get,
    path = "/stock",
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
        .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
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
    Ok(Json(StockListResponse {
        items: rows.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/stock/{id}",
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
    let row = qm_db::stock::get(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let product = qm_db::products::find_by_id(&state.db, row.product_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(StockBatchWithProduct { batch: row, product }.into()))
}

#[utoipa::path(
    post,
    path = "/stock",
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
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(StockBatchWithProduct { batch: row, product }.into()),
    ))
}

#[utoipa::path(
    patch,
    path = "/stock/{id}",
    tag = "stock",
    params(("id" = Uuid, Path)),
    request_body = UpdateStockRequest,
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

    let existing = qm_db::stock::get(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let product = load_product_for_write(&state, household_id, existing.product_id).await?;

    if let Some(q) = req.quantity.as_deref() {
        validate_positive_decimal(q)?;
    }
    if let Some(unit) = req.unit.as_deref() {
        validate_unit_family(unit, &product.family)?;
    }
    if let Some(loc) = req.location_id {
        validate_location(&state, household_id, loc).await?;
    }
    if let Some(Some(d)) = req.expires_on.as_ref() {
        validate_iso_date(d)?;
    }
    if let Some(Some(d)) = req.opened_on.as_ref() {
        validate_iso_date(d)?;
    }

    let upd = StockUpdate {
        quantity: req.quantity.as_deref(),
        unit: req.unit.as_deref(),
        location_id: req.location_id,
        expires_on: req.expires_on.as_ref().map(|o| o.as_deref()),
        opened_on: req.opened_on.as_ref().map(|o| o.as_deref()),
        note: req.note.as_ref().map(|o| o.as_deref()),
    };
    let row = qm_db::stock::update(&state.db, household_id, id, &upd).await?;
    Ok(Json(StockBatchWithProduct { batch: row, product }.into()))
}

#[utoipa::path(
    delete,
    path = "/stock/{id}",
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
    let removed = qm_db::stock::delete(&state.db, household_id, id).await?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[utoipa::path(
    post,
    path = "/stock/consume",
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

    let batches = qm_db::stock::list_active_batches(
        &state.db,
        household_id,
        product.id,
        req.location_id,
    )
    .await?;
    let refs = batches
        .iter()
        .map(|b| b.to_batch_ref())
        .collect::<Result<Vec<_>, _>>()?;

    let quantity = Decimal::from_str(&req.quantity)
        .map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?;

    let plan = match plan_consumption(refs, quantity, &req.unit) {
        Ok(p) => p,
        Err(qm_core::QmError::InsufficientStock { requested, available }) => {
            return Err(ApiError::InsufficientStock {
                requested: requested.to_string(),
                available: available.to_string(),
            });
        }
        Err(err) => return Err(ApiError::Domain(err)),
    };

    qm_db::stock::apply_consumption(&state.db, household_id, &plan).await?;

    // Report the plan back in a shape that mirrors what each batch was asked
    // to give up, expressed in the batch's own unit (matches our domain).
    let consumed = plan
        .into_iter()
        .zip(batches.iter())
        .map(|(c, b)| ConsumedBatchDto {
            batch_id: c.batch_id,
            quantity: c.quantity.to_string(),
            unit: b.unit.clone(),
            depleted: c.depletes,
        })
        .collect();

    Ok(Json(ConsumeResponse { consumed }))
}

// ----- helpers -----

fn validate_positive_decimal(s: &str) -> ApiResult<()> {
    let q = Decimal::from_str(s).map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?;
    if q <= Decimal::ZERO {
        return Err(ApiError::BadRequest("quantity must be greater than zero".into()));
    }
    Ok(())
}

fn validate_iso_date(s: &str) -> ApiResult<()> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
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

async fn validate_location(state: &AppState, household_id: Uuid, location_id: Uuid) -> ApiResult<()> {
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
    // Manual products are household-private; OFF products are shared.
    if product.source == qm_db::products::SOURCE_MANUAL
        && product.created_by_household_id != Some(household_id)
    {
        return Err(ApiError::NotFound);
    }
    Ok(product)
}

