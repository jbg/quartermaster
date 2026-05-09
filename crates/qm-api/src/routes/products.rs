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
    openfoodfacts::{
        self, OffContributionForm, OffResult, OffWriteCredentials, OffWriteResult,
        OpenFoodFactsClient,
    },
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
        .route(
            "/products/{id}/off-contribution-preview",
            get(off_contribution_preview),
        )
        .route("/products/{id}/off-contribution", post(off_contribution))
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
    /// Amount in `package_unit` for one retail package when known from the catalogue.
    pub package_quantity: Option<String>,
    /// Unit for `package_quantity`; belongs to the same family as the product.
    pub package_unit: Option<String>,
    /// Maximum days this product should remain open before being discarded.
    pub max_open_days: Option<i64>,
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
            package_quantity: p.package_quantity,
            package_unit: p.package_unit,
            max_open_days: p.max_open_days,
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
    /// Amount in `package_unit` for one retail package.
    pub package_quantity: Option<String>,
    /// Unit for `package_quantity`; belongs to the same family as the product.
    pub package_unit: Option<String>,
    pub max_open_days: Option<i64>,
}

pub type UpdateProductRequest = JsonPatchDocument;

#[derive(Debug, Default)]
struct ProductPatch {
    name: Option<String>,
    brand: Option<Option<String>>,
    family: Option<UnitFamily>,
    preferred_unit: Option<String>,
    image_url: Option<Option<String>>,
    max_open_days: Option<Option<i64>>,
    package_quantity: Option<Option<String>>,
    package_unit: Option<Option<String>>,
}

impl ProductPatch {
    fn is_off_local_correction_only(&self) -> bool {
        self.image_url.is_none() && self.max_open_days.is_none()
    }

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
            "/package_quantity" => {
                self.package_quantity = Some(Some(string_value("package_quantity", value)?));
            }
            "/package_unit" => {
                self.package_unit = Some(Some(string_value("package_unit", value)?));
            }
            "/max_open_days" => {
                self.max_open_days = Some(Some(max_open_days_value("max_open_days", value)?));
            }
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
            "/package_quantity" => {
                reject_value_for_remove("package_quantity", value)?;
                self.package_quantity = Some(None);
            }
            "/package_unit" => {
                reject_value_for_remove("package_unit", value)?;
                self.package_unit = Some(None);
            }
            "/max_open_days" => {
                reject_value_for_remove("max_open_days", value)?;
                self.max_open_days = Some(None);
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

#[derive(Debug, Serialize, ToSchema)]
pub struct OffContributionFieldChange {
    pub field: String,
    pub current_value: Option<String>,
    pub off_value: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OffContributionPreviewResponse {
    pub eligible: bool,
    pub credentials_configured: bool,
    pub credentials_present: bool,
    pub changed_fields: Vec<OffContributionFieldChange>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OffContributionResponse {
    pub status: &'static str,
    pub status_verbose: String,
    pub submitted_fields: Vec<String>,
    pub product: ProductDto,
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
    if let Some(days) = req.max_open_days {
        validate_max_open_days(days)?;
    }
    let package = validate_package_size(
        req.package_quantity.as_deref(),
        req.package_unit.as_deref(),
        req.family,
    )?;

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

    let row = qm_db::products::create_manual_with_max_open_days(
        &state.db,
        household_id,
        name,
        brand_trim,
        req.family.as_str(),
        req.preferred_unit.as_deref(),
        barcode_trim,
        image_url_trim,
        package.as_ref().map(|(quantity, _)| quantity.as_str()),
        package.as_ref().map(|(_, unit)| unit.as_str()),
        req.max_open_days,
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

    if existing.source == qm_db::products::SOURCE_OFF && !req.is_off_local_correction_only() {
        return Err(ApiError::OffProductReadOnly);
    }

    if existing.source == qm_db::products::SOURCE_MANUAL
        && existing.created_by_household_id != Some(household_id)
    {
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

    let effective_family = req.family.unwrap_or(existing_family);
    let effective_preferred_unit = req
        .preferred_unit
        .as_deref()
        .unwrap_or(existing.preferred_unit.as_str());
    let u = qm_core::units::lookup(effective_preferred_unit)
        .map_err(|_| ApiError::UnknownUnit(effective_preferred_unit.to_owned()))?;
    if u.family != effective_family {
        return Err(ApiError::UnitFamilyMismatch {
            product_family: effective_family.as_str().to_owned(),
            unit: effective_preferred_unit.to_owned(),
        });
    }
    if let Some(Some(days)) = req.max_open_days {
        validate_max_open_days(days)?;
    }
    let package_quantity = match &req.package_quantity {
        Some(Some(value)) => Some(Some(value.as_str())),
        Some(None) => Some(None),
        None => None,
    };
    let package_unit = match &req.package_unit {
        Some(Some(value)) => Some(Some(value.as_str())),
        Some(None) => Some(None),
        None => None,
    };
    let effective_package = validate_patch_package_size(
        package_quantity,
        package_unit,
        existing.package_quantity.as_deref(),
        existing.package_unit.as_deref(),
        effective_family,
    )?;
    let convert_piece_stock_to_package_unit = if let Some(fam) = req.family {
        if fam != existing_family {
            let conflicts = qm_db::stock::conflicting_units_for_family_change(
                &state.db,
                existing.id,
                fam.as_str(),
            )
            .await?;
            if conflicts.is_empty() {
                false
            } else if existing_family == UnitFamily::Count
                && fam != UnitFamily::Count
                && conflicts.len() == 1
                && conflicts[0] == "piece"
                && effective_package.is_some()
            {
                true
            } else {
                return Err(ApiError::ProductHasIncompatibleStock {
                    conflicting_units: conflicts,
                });
            }
        } else {
            false
        }
    } else {
        false
    };

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
            max_open_days: req.max_open_days,
            package_quantity: Some(
                effective_package
                    .as_ref()
                    .map(|(quantity, _)| quantity.as_str()),
            ),
            package_unit: Some(effective_package.as_ref().map(|(_, unit)| unit.as_str())),
            package_size_local_override: req
                .package_quantity
                .as_ref()
                .or(req.package_unit.as_ref())
                .map(|_| true),
            name_local_override: req.name.as_ref().map(|_| true),
            brand_local_override: req.brand.as_ref().map(|_| true),
            family_local_override: if req.family.is_some() || req.preferred_unit.is_some() {
                Some(true)
            } else {
                None
            },
        },
    )
    .await?;

    if convert_piece_stock_to_package_unit {
        if let Some((package_quantity, package_unit)) = effective_package.as_ref() {
            qm_db::stock::convert_active_piece_stock_to_package_unit(
                &state.db,
                id,
                package_quantity,
                package_unit,
            )
            .await?;
        }
    }

    let updated = if convert_piece_stock_to_package_unit {
        qm_db::products::find_by_id(&state.db, id)
            .await?
            .ok_or(ApiError::NotFound)?
    } else {
        updated
    };

    Ok(Json(updated.try_into()?))
}

fn max_open_days_value(field: &str, value: Option<&serde_json::Value>) -> ApiResult<i64> {
    let value = value.ok_or_else(|| ApiError::BadRequest(format!("{field} is required")))?;
    value
        .as_i64()
        .ok_or_else(|| ApiError::BadRequest(format!("{field} must be an integer")))
}

fn validate_max_open_days(days: i64) -> ApiResult<()> {
    if days <= 0 {
        return Err(ApiError::BadRequest(
            "max_open_days must be greater than zero".into(),
        ));
    }
    Ok(())
}

fn validate_package_size(
    quantity: Option<&str>,
    unit: Option<&str>,
    family: UnitFamily,
) -> ApiResult<Option<(String, String)>> {
    let quantity = quantity.map(str::trim).filter(|s| !s.is_empty());
    let unit = unit.map(str::trim).filter(|s| !s.is_empty());
    match (quantity, unit) {
        (None, None) => Ok(None),
        (Some(_), None) | (None, Some(_)) => Err(ApiError::BadRequest(
            "package_quantity and package_unit must be provided together".into(),
        )),
        (Some(quantity), Some(unit)) => {
            let parsed = quantity
                .parse::<rust_decimal::Decimal>()
                .map_err(|_| ApiError::BadRequest("package_quantity must be a decimal".into()))?;
            if parsed <= rust_decimal::Decimal::ZERO {
                return Err(ApiError::BadRequest(
                    "package_quantity must be greater than zero".into(),
                ));
            }
            let u =
                qm_core::units::lookup(unit).map_err(|_| ApiError::UnknownUnit(unit.to_owned()))?;
            if u.family != family {
                return Err(ApiError::UnitFamilyMismatch {
                    product_family: family.as_str().to_owned(),
                    unit: unit.to_owned(),
                });
            }
            Ok(Some((quantity.to_owned(), unit.to_owned())))
        }
    }
}

fn validate_patch_package_size(
    quantity_patch: Option<Option<&str>>,
    unit_patch: Option<Option<&str>>,
    existing_quantity: Option<&str>,
    existing_unit: Option<&str>,
    family: UnitFamily,
) -> ApiResult<Option<(String, String)>> {
    let quantity = match quantity_patch {
        Some(inner) => inner,
        None => existing_quantity,
    };
    let unit = match unit_patch {
        Some(inner) => inner,
        None => existing_unit,
    };
    validate_package_size(quantity, unit, family)
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
    let package = openfoodfacts::normalize_package(
        off_product.quantity.as_deref(),
        off_product.quantity_unit.as_deref(),
    );

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
        package.as_ref().map(|(quantity, _)| quantity.as_str()),
        package.as_ref().map(|(_, unit)| unit.as_str()),
    )
    .await?;
    qm_db::barcode_cache::put_hit(&state.db, &barcode, row.id).await?;

    Ok(Json(row.try_into()?))
}

#[utoipa::path(
    get,
    path = "/products/{id}/off-contribution-preview",
    operation_id = "product_off_contribution_preview",
    tag = "products",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = OffContributionPreviewResponse),
        (status = 404, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn off_contribution_preview(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OffContributionPreviewResponse>> {
    let product = qm_db::products::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let credentials = qm_db::off_credentials::get(&state.db, current.user_id).await?;
    Ok(Json(OffContributionPreviewResponse {
        eligible: product.source == qm_db::products::SOURCE_OFF && product.off_barcode.is_some(),
        credentials_configured: state.config.off_credential_encryption_key.is_some(),
        credentials_present: credentials.is_some(),
        changed_fields: off_contribution_changes(&product),
    }))
}

#[utoipa::path(
    post,
    path = "/products/{id}/off-contribution",
    operation_id = "product_off_contribution",
    tag = "products",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, body = OffContributionResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 401, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
        (status = 409, body = crate::error::ApiErrorBody),
        (status = 428, body = crate::error::ApiErrorBody),
        (status = 502, body = crate::error::ApiErrorBody),
        (status = 503, body = crate::error::ApiErrorBody),
    ),
    security(("bearer" = [])),
)]
pub async fn off_contribution(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OffContributionResponse>> {
    let product = qm_db::products::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if product.source != qm_db::products::SOURCE_OFF {
        return Err(ApiError::ManualProductNotRefreshable);
    }
    let barcode = product
        .off_barcode
        .clone()
        .ok_or(ApiError::ManualProductNotRefreshable)?;
    let changes = off_contribution_changes(&product);
    if changes.is_empty() {
        return Err(ApiError::OffContributionNoChanges);
    }
    let credentials =
        crate::routes::accounts::load_openfoodfacts_credentials(&state, current.user_id).await?;

    let submit_name = changes.iter().any(|change| change.field == "product_name");
    let submit_brand = changes.iter().any(|change| change.field == "brands");
    let submit_package = changes.iter().any(|change| {
        change.field == "product_quantity" || change.field == "product_quantity_unit"
    });

    let off = OpenFoodFactsClient::new(
        state.http.clone(),
        state.off_breaker.clone(),
        state.config.clone(),
    );
    let result = off
        .contribute(
            &OffWriteCredentials {
                username: credentials.username,
                password: credentials.password,
            },
            &OffContributionForm {
                barcode,
                product_name: submit_name.then(|| product.name.clone()),
                brands: submit_brand.then(|| product.brand.clone()),
                product_quantity: submit_package
                    .then(|| product.package_quantity.clone())
                    .flatten(),
                product_quantity_unit: submit_package
                    .then(|| product.package_unit.clone())
                    .flatten(),
                app_uuid: openfoodfacts::app_uuid_for_user(current.user_id),
            },
        )
        .await;

    let status_verbose = match result {
        OffWriteResult::Saved { status_verbose } => status_verbose,
        OffWriteResult::AuthFailed => return Err(ApiError::OffAuthenticationFailed),
        OffWriteResult::NotFound => return Err(ApiError::NotFound),
        OffWriteResult::Upstream(_) => return Err(ApiError::BadGateway),
    };

    let updated = qm_db::products::mark_off_contributed(
        &state.db,
        id,
        submit_name,
        submit_brand,
        submit_package,
    )
    .await?;
    qm_db::products::invalidate_barcode_cache_for(&state.db, id).await?;

    Ok(Json(OffContributionResponse {
        status: "saved",
        status_verbose,
        submitted_fields: changes.into_iter().map(|change| change.field).collect(),
        product: updated.try_into()?,
    }))
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

fn off_contribution_changes(product: &ProductRow) -> Vec<OffContributionFieldChange> {
    if product.source != qm_db::products::SOURCE_OFF {
        return Vec::new();
    }
    let mut changes = Vec::new();
    let off_name = product.off_name.as_deref().unwrap_or(&product.name);
    if product.name.trim() != off_name.trim() {
        changes.push(OffContributionFieldChange {
            field: "product_name".into(),
            current_value: Some(product.name.clone()),
            off_value: Some(off_name.to_owned()),
        });
    }
    let brand_current = product.brand.as_deref().unwrap_or("").trim();
    let brand_off = product.off_brand.as_deref().unwrap_or("").trim();
    if brand_current != brand_off {
        changes.push(OffContributionFieldChange {
            field: "brands".into(),
            current_value: product.brand.clone(),
            off_value: product.off_brand.clone(),
        });
    }
    let package_current = product.package_quantity.as_deref().unwrap_or("").trim();
    let package_off = product.off_package_quantity.as_deref().unwrap_or("").trim();
    let unit_current = product.package_unit.as_deref().unwrap_or("").trim();
    let unit_off = product.off_package_unit.as_deref().unwrap_or("").trim();
    if package_current != package_off || unit_current != unit_off {
        changes.push(OffContributionFieldChange {
            field: "product_quantity".into(),
            current_value: product.package_quantity.clone(),
            off_value: product.off_package_quantity.clone(),
        });
        changes.push(OffContributionFieldChange {
            field: "product_quantity_unit".into(),
            current_value: product.package_unit.clone(),
            off_value: product.off_package_unit.clone(),
        });
    }
    changes
}

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
            let package =
                openfoodfacts::normalize_package(p.quantity.as_deref(), p.quantity_unit.as_deref());
            let row = qm_db::products::upsert_from_off(
                &state.db,
                &p.barcode,
                &p.name,
                p.brand.as_deref(),
                family.as_str(),
                Some(preferred),
                p.image_url.as_deref(),
                package.as_ref().map(|(quantity, _)| quantity.as_str()),
                package.as_ref().map(|(_, unit)| unit.as_str()),
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
