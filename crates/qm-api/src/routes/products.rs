use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
    Json, Router,
};
use qm_core::units::UnitFamily;
use qm_db::products::{ProductRow, ProductUpdate};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    barcode,
    error::{ApiError, ApiResult},
    openfoodfacts::{self, OffResult, OpenFoodFactsClient},
    rate_limit::RateLimitLayerState,
    routes::patch::{
        reject_remove, reject_value_for_remove, string_value, JsonPatchDocument, JsonPatchOperation,
    },
    types::ProductSource,
    AppState,
};

pub fn router(rate_limit_state: RateLimitLayerState) -> Router<AppState> {
    Router::new()
        .route("/products/search", get(search))
        .route(
            "/products/by-barcode/{barcode}",
            get(by_barcode).route_layer(middleware::from_fn_with_state(
                rate_limit_state,
                crate::rate_limit::enforce,
            )),
        )
        .route("/products", get(list).post(create))
        .route(
            "/products/{id}",
            get(get_one).patch(update).delete(delete_one),
        )
        .route("/products/{id}/refresh", post(refresh))
        .route("/products/{id}/restore", post(restore))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProductDto {
    pub id: Uuid,
    pub name: String,
    pub brand: Option<String>,
    pub family: UnitFamily,
    pub preferred_unit: String,
    pub image_url: Option<String>,
    pub barcode: Option<String>,
    pub source: ProductSource,
    /// RFC-3339 timestamp when the product was soft-deleted. Present only
    /// when the caller explicitly asked for deleted rows (e.g. via
    /// `/api/v1/products/search?include_deleted=true` or the history timeline).
    pub deleted_at: Option<String>,
}

impl TryFrom<ProductRow> for ProductDto {
    type Error = ApiError;

    fn try_from(p: ProductRow) -> Result<Self, Self::Error> {
        let family = UnitFamily::from_str_ci(&p.family).ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "unknown family '{}' in DB row for product {}",
                p.family,
                p.id,
            ))
        })?;
        let source: ProductSource = p.source.parse()?;
        Ok(Self {
            id: p.id,
            name: p.name,
            brand: p.brand,
            family,
            preferred_unit: p.preferred_unit,
            image_url: p.image_url,
            barcode: p.off_barcode,
            source,
            deleted_at: p.deleted_at,
        })
    }
}

fn products_into_dtos(rows: Vec<ProductRow>) -> ApiResult<Vec<ProductDto>> {
    rows.into_iter().map(ProductDto::try_from).collect()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProductRequest {
    pub name: String,
    pub brand: Option<String>,
    pub family: UnitFamily,
    /// Optional display unit override. Must belong to `family`. Defaults to
    /// the family's base unit (`g` / `ml` / `piece`) when omitted.
    pub preferred_unit: Option<String>,
    pub barcode: Option<String>,
    pub image_url: Option<String>,
}

pub type UpdateProductRequest = JsonPatchDocument;

#[derive(Debug, Default)]
struct ProductPatch {
    name: Option<String>,
    brand: Option<Option<String>>,
    family: Option<UnitFamily>,
    preferred_unit: Option<String>,
    image_url: Option<Option<String>>,
}

impl ProductPatch {
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
            "/name" => self.name = Some(string_value("name", value)?),
            "/brand" => self.brand = Some(Some(string_value("brand", value)?)),
            "/family" => {
                let value = string_value("family", value)?;
                self.family = Some(UnitFamily::from_str_ci(&value).ok_or_else(|| {
                    ApiError::BadRequest(format!("unknown product family: {value}"))
                })?);
            }
            "/preferred_unit" => self.preferred_unit = Some(string_value("preferred_unit", value)?),
            "/image_url" => self.image_url = Some(Some(string_value("image_url", value)?)),
            other => {
                return Err(ApiError::BadRequest(format!(
                    "unknown product patch path: {other}"
                )));
            }
        }
        Ok(())
    }

    fn remove(&mut self, path: &str, value: Option<&serde_json::Value>) -> ApiResult<()> {
        match path {
            "/brand" => {
                reject_value_for_remove("brand", value)?;
                self.brand = Some(None);
            }
            "/image_url" => {
                reject_value_for_remove("image_url", value)?;
                self.image_url = Some(None);
            }
            "/name" => return Err(reject_remove("name")),
            "/family" => return Err(reject_remove("family")),
            "/preferred_unit" => return Err(reject_remove("preferred_unit")),
            other => {
                return Err(ApiError::BadRequest(format!(
                    "unknown product patch path: {other}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<i64>,
    /// When true, include soft-deleted manual products. Soft-deleted rows
    /// have `deleted_at` populated; clients can render them muted.
    pub include_deleted: Option<bool>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
    /// When true, include soft-deleted manual products. Soft-deleted rows
    /// have `deleted_at` populated; clients can render them muted.
    pub include_deleted: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProductSearchResponse {
    pub items: Vec<ProductDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BarcodeLookupResponse {
    pub product: ProductDto,
    /// `cache` when served from our DB, `openfoodfacts` when fetched just now.
    pub source: &'static str,
}

// ----- handlers -----

#[utoipa::path(
    get,
    path = "/products",
    operation_id = "product_list",
    tag = "products",
    params(ListQuery),
    responses(
        (status = 200, body = ProductSearchResponse),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<ProductSearchResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let include_deleted = q.include_deleted.unwrap_or(false);
    let rows = qm_db::products::list_visible(
        &state.db,
        household_id,
        q.q.as_deref(),
        limit,
        include_deleted,
    )
    .await?;
    Ok(Json(ProductSearchResponse {
        items: products_into_dtos(rows)?,
    }))
}

#[utoipa::path(
    get,
    path = "/products/search",
    operation_id = "product_search",
    tag = "products",
    params(SearchQuery),
    responses(
        (status = 200, body = ProductSearchResponse),
        (status = 401, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn search(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<ProductSearchResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let query = q.q.trim();
    if query.is_empty() {
        return Ok(Json(ProductSearchResponse { items: Vec::new() }));
    }
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let include_deleted = q.include_deleted.unwrap_or(false);
    let rows = qm_db::products::search_with_deleted(
        &state.db,
        household_id,
        query,
        limit,
        include_deleted,
    )
    .await?;
    Ok(Json(ProductSearchResponse {
        items: products_into_dtos(rows)?,
    }))
}

#[utoipa::path(
    get,
    path = "/products/by-barcode/{barcode}",
    operation_id = "product_by_barcode",
    tag = "products",
    params(("barcode" = String, Path, description = "EAN-8/12/13/14; non-digits are stripped and UPC-A is zero-padded")),
    responses(
        (status = 200, body = BarcodeLookupResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 429, body = crate::error::ApiErrorBody),
        (status = 502, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn by_barcode(
    State(state): State<AppState>,
    _current: CurrentUser,
    Path(raw_barcode): Path<String>,
) -> ApiResult<Json<BarcodeLookupResponse>> {
    let barcode = barcode::normalise(&raw_barcode)?;

    let now = jiff::Timestamp::now();
    let cached = qm_db::barcode_cache::get(&state.db, &barcode).await?;

    if let Some(entry) = &cached {
        if entry.is_fresh(
            now,
            state.config.off_positive_ttl_days,
            state.config.off_negative_ttl_days,
        ) {
            if entry.miss {
                return Err(ApiError::NotFound);
            }
            if let Some(pid) = entry.product_id {
                let product = qm_db::products::find_by_id(&state.db, pid)
                    .await?
                    .ok_or(ApiError::NotFound)?;
                return Ok(Json(BarcodeLookupResponse {
                    product: product.try_into()?,
                    source: "cache",
                }));
            }
        }
    }

    fetch_and_cache(&state, &barcode).await
}

#[utoipa::path(
    post,
    path = "/products",
    operation_id = "product_create",
    tag = "products",
    request_body = CreateProductRequest,
    responses(
        (status = 201, body = ProductDto),
        (status = 400, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateProductRequest>,
) -> ApiResult<(StatusCode, Json<ProductDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;

    let name = req.name.trim();
    if name.is_empty() || name.len() > 256 {
        return Err(ApiError::BadRequest(
            "product name must be 1..=256 chars".into(),
        ));
    }
    if let Some(pu) = req.preferred_unit.as_deref() {
        let u = qm_core::units::lookup(pu).map_err(|_| ApiError::UnknownUnit(pu.to_owned()))?;
        if u.family != req.family {
            return Err(ApiError::UnitFamilyMismatch {
                product_family: req.family.as_str().to_owned(),
                unit: pu.to_owned(),
            });
        }
    }

    let brand_trim = req
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let barcode_trim = req
        .barcode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let image_url_trim = req
        .image_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let row = qm_db::products::create_manual(
        &state.db,
        household_id,
        name,
        brand_trim,
        req.family.as_str(),
        req.preferred_unit.as_deref(),
        barcode_trim,
        image_url_trim,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(row.try_into()?)))
}

#[utoipa::path(
    get,
    path = "/products/{id}",
    operation_id = "product_get",
    tag = "products",
    params(("id" = Uuid, Path, description = "Product ID")),
    responses(
        (status = 200, body = ProductDto),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn get_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProductDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::products::find_including_deleted(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if row.source == qm_db::products::SOURCE_MANUAL
        && row.created_by_household_id != Some(household_id)
    {
        return Err(ApiError::NotFound);
    }
    Ok(Json(row.try_into()?))
}

#[utoipa::path(
    patch,
    path = "/products/{id}",
    operation_id = "product_update",
    tag = "products",
    params(("id" = Uuid, Path)),
    request_body = Vec<JsonPatchOperation>,
    responses(
        (status = 200, body = ProductDto),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProductRequest>,
) -> ApiResult<Json<ProductDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let req = ProductPatch::parse(req)?;

    let existing = qm_db::products::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // OFF products are read-only from the client; only refresh is allowed.
    if existing.source == qm_db::products::SOURCE_OFF {
        return Err(ApiError::OffProductReadOnly);
    }

    // Manual product: only the owning household may edit.
    if existing.created_by_household_id != Some(household_id) {
        return Err(ApiError::NotFound);
    }

    // Validation on provided fields.
    if let Some(name) = req.name.as_deref() {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.len() > 256 {
            return Err(ApiError::BadRequest(
                "product name must be 1..=256 chars".into(),
            ));
        }
    }
    let existing_family = UnitFamily::from_str_ci(&existing.family).ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!(
            "unknown family '{}' in DB row for product {}",
            existing.family,
            existing.id,
        ))
    })?;

    if let Some(fam) = req.family {
        if fam != existing_family {
            let conflicts = qm_db::stock::conflicting_units_for_family_change(
                &state.db,
                existing.id,
                fam.as_str(),
            )
            .await?;
            if !conflicts.is_empty() {
                return Err(ApiError::ProductHasIncompatibleStock {
                    conflicting_units: conflicts,
                });
            }
        }
    }

    // If both family and preferred_unit are changing, validate in the new
    // family; otherwise validate preferred_unit against the existing family.
    let effective_family = req.family.unwrap_or(existing_family);
    if let Some(pu) = req.preferred_unit.as_deref() {
        let u = qm_core::units::lookup(pu).map_err(|_| ApiError::UnknownUnit(pu.to_owned()))?;
        if u.family != effective_family {
            return Err(ApiError::UnitFamilyMismatch {
                product_family: effective_family.as_str().to_owned(),
                unit: pu.to_owned(),
            });
        }
    }

    let name_trim = req.name.as_deref().map(str::trim);
    let brand_inner: Option<Option<&str>> = req.brand.as_ref().map(|inner| {
        inner
            .as_deref()
            .and_then(|value| Some(value.trim()).filter(|s| !s.is_empty()))
    });
    let image_inner: Option<Option<&str>> = req.image_url.as_ref().map(|inner| {
        inner
            .as_deref()
            .and_then(|value| Some(value.trim()).filter(|s| !s.is_empty()))
    });
    let family_str = req.family.map(UnitFamily::as_str);
    let preferred_unit = req.preferred_unit.as_deref();

    let updated = qm_db::products::update(
        &state.db,
        id,
        &ProductUpdate {
            name: name_trim,
            brand: brand_inner,
            family: family_str,
            preferred_unit,
            image_url: image_inner,
        },
    )
    .await?;

    Ok(Json(updated.try_into()?))
}

#[utoipa::path(
    delete,
    path = "/products/{id}",
    operation_id = "product_delete",
    tag = "products",
    params(("id" = Uuid, Path)),
    responses(
        (status = 204),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let existing = qm_db::products::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if existing.source == qm_db::products::SOURCE_OFF {
        return Err(ApiError::OffProductReadOnly);
    }
    if existing.created_by_household_id != Some(household_id) {
        return Err(ApiError::NotFound);
    }
    if qm_db::stock::has_active_stock_for_product(&state.db, id).await? {
        return Err(ApiError::ProductHasStock);
    }
    qm_db::products::soft_delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/products/{id}/refresh",
    operation_id = "product_refresh",
    tag = "products",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = ProductDto),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 502, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn refresh(
    State(state): State<AppState>,
    _current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProductDto>> {
    let existing = qm_db::products::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if existing.source != qm_db::products::SOURCE_OFF {
        return Err(ApiError::ManualProductNotRefreshable);
    }
    let Some(barcode) = existing.off_barcode.clone() else {
        return Err(ApiError::ManualProductNotRefreshable);
    };

    // Fetch OFF first so we can check for family conflicts before
    // touching local state.
    let off = OpenFoodFactsClient::new(
        state.http.clone(),
        state.off_breaker.clone(),
        state.config.clone(),
    );
    let off_product = match off.fetch(&barcode).await {
        OffResult::Found(p) => p,
        OffResult::NotFound => return Err(ApiError::NotFound),
        OffResult::Upstream(_) => return Err(ApiError::BadGateway),
    };
    let family = openfoodfacts::infer_family(off_product.quantity_unit.as_deref());

    // Guard: if OFF's inference changes the family, don't silently adopt it
    // when active batches would become cross-family. Same check the PATCH
    // handler runs.
    if family.as_str() != existing.family {
        let conflicts = qm_db::stock::conflicting_units_for_family_change(
            &state.db,
            existing.id,
            family.as_str(),
        )
        .await?;
        if !conflicts.is_empty() {
            return Err(ApiError::ProductHasIncompatibleStock {
                conflicting_units: conflicts,
            });
        }
    }

    // Safe to land the new data.
    qm_db::products::invalidate_barcode_cache_for(&state.db, id).await?;
    let preferred = qm_db::products::base_unit_for_family(family.as_str());
    let row = qm_db::products::upsert_from_off(
        &state.db,
        &off_product.barcode,
        &off_product.name,
        off_product.brand.as_deref(),
        family.as_str(),
        Some(preferred),
        off_product.image_url.as_deref(),
    )
    .await?;
    qm_db::barcode_cache::put_hit(&state.db, &barcode, row.id).await?;

    Ok(Json(row.try_into()?))
}

#[utoipa::path(
    post,
    path = "/products/{id}/restore",
    operation_id = "product_restore",
    tag = "products",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = ProductDto),
        (status = 403, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn restore(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProductDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let existing = qm_db::products::find_including_deleted(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if existing.source == qm_db::products::SOURCE_OFF {
        return Err(ApiError::OffProductReadOnly);
    }
    if existing.created_by_household_id != Some(household_id) {
        return Err(ApiError::NotFound);
    }
    if existing.deleted_at.is_none() {
        return Err(ApiError::Conflict("product is not deleted".into()));
    }

    qm_db::products::restore(&state.db, id).await?;
    let refreshed = qm_db::products::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(refreshed.try_into()?))
}

// ----- helpers -----

async fn fetch_and_cache(
    state: &AppState,
    barcode: &str,
) -> ApiResult<Json<BarcodeLookupResponse>> {
    let off = OpenFoodFactsClient::new(
        state.http.clone(),
        state.off_breaker.clone(),
        state.config.clone(),
    );
    match off.fetch(barcode).await {
        OffResult::Found(p) => {
            let family = openfoodfacts::infer_family(p.quantity_unit.as_deref());
            let preferred = qm_db::products::base_unit_for_family(family.as_str());
            let row = qm_db::products::upsert_from_off(
                &state.db,
                &p.barcode,
                &p.name,
                p.brand.as_deref(),
                family.as_str(),
                Some(preferred),
                p.image_url.as_deref(),
            )
            .await?;
            qm_db::barcode_cache::put_hit(&state.db, barcode, row.id).await?;
            Ok(Json(BarcodeLookupResponse {
                product: row.try_into()?,
                source: "openfoodfacts",
            }))
        }
        OffResult::NotFound => {
            qm_db::barcode_cache::put_miss(&state.db, barcode).await?;
            Err(ApiError::NotFound)
        }
        OffResult::Upstream(_) => Err(ApiError::BadGateway),
    }
}
