use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use qm_db::products::{ProductRow, ProductUpdate};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    barcode,
    error::{ApiError, ApiResult},
    openfoodfacts::{self, OffResult, OpenFoodFactsClient},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/products/search", get(search))
        .route("/products/by-barcode/{barcode}", get(by_barcode))
        .route("/products", post(create))
        .route(
            "/products/{id}",
            get(get_one).patch(update).delete(delete_one),
        )
        .route("/products/{id}/refresh", post(refresh))
}

/// Deserializer helper for explicit-null semantics on optional-clearable
/// fields, mirroring the same pattern used on `UpdateStockRequest`.
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
pub struct ProductDto {
    pub id: Uuid,
    pub name: String,
    pub brand: Option<String>,
    pub family: String,
    pub preferred_unit: String,
    pub image_url: Option<String>,
    pub barcode: Option<String>,
    pub source: String,
}

impl From<ProductRow> for ProductDto {
    fn from(p: ProductRow) -> Self {
        Self {
            id: p.id,
            name: p.name,
            brand: p.brand,
            family: p.family,
            preferred_unit: p.preferred_unit,
            image_url: p.image_url,
            barcode: p.off_barcode,
            source: p.source,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProductRequest {
    pub name: String,
    pub brand: Option<String>,
    /// One of `mass`, `volume`, `count`.
    pub family: String,
    /// Optional display unit override. Must belong to `family`. Defaults to
    /// the family's base unit (`g` / `ml` / `piece`) when omitted.
    pub preferred_unit: Option<String>,
    pub barcode: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option::deserialize")]
    pub brand: Option<Option<String>>,
    pub family: Option<String>,
    pub preferred_unit: Option<String>,
    #[serde(default, deserialize_with = "double_option::deserialize")]
    pub image_url: Option<Option<String>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<i64>,
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
    path = "/products/search",
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
    let rows = qm_db::products::search(&state.db, household_id, query, limit).await?;
    Ok(Json(ProductSearchResponse {
        items: rows.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/products/by-barcode/{barcode}",
    tag = "products",
    params(("barcode" = String, Path, description = "EAN-8/12/13/14; non-digits are stripped and UPC-A is zero-padded")),
    responses(
        (status = 200, body = BarcodeLookupResponse),
        (status = 400, body = crate::error::ApiErrorBody),
        (status = 404, body = crate::error::ApiErrorBody),
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

    let now = chrono::Utc::now();
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
                    product: product.into(),
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
    validate_family(&req.family)?;
    if let Some(pu) = req.preferred_unit.as_deref() {
        let u = qm_core::units::lookup(pu).map_err(|_| ApiError::UnknownUnit(pu.to_owned()))?;
        if u.family.as_str() != req.family {
            return Err(ApiError::UnitFamilyMismatch {
                product_family: req.family.clone(),
                unit: pu.to_owned(),
            });
        }
    }

    let brand_trim = req.brand.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let barcode_trim = req
        .barcode
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let row = qm_db::products::create_manual(
        &state.db,
        household_id,
        name,
        brand_trim,
        &req.family,
        req.preferred_unit.as_deref(),
        barcode_trim,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(row.into())))
}

#[utoipa::path(
    get,
    path = "/products/{id}",
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
    let row = qm_db::products::find_by_id(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if row.source == qm_db::products::SOURCE_MANUAL
        && row.created_by_household_id != Some(household_id)
    {
        return Err(ApiError::NotFound);
    }
    Ok(Json(row.into()))
}

#[utoipa::path(
    patch,
    path = "/products/{id}",
    tag = "products",
    params(("id" = Uuid, Path)),
    request_body = UpdateProductRequest,
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
    if let Some(fam) = req.family.as_deref() {
        validate_family(fam)?;
        if fam != existing.family {
            let conflicts = qm_db::stock::conflicting_units_for_family_change(
                &state.db,
                existing.id,
                fam,
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
    let effective_family = req.family.as_deref().unwrap_or(&existing.family);
    if let Some(pu) = req.preferred_unit.as_deref() {
        let u = qm_core::units::lookup(pu).map_err(|_| ApiError::UnknownUnit(pu.to_owned()))?;
        if u.family.as_str() != effective_family {
            return Err(ApiError::UnitFamilyMismatch {
                product_family: effective_family.to_owned(),
                unit: pu.to_owned(),
            });
        }
    }

    let name_trim = req.name.as_deref().map(str::trim);
    let brand_inner: Option<Option<&str>> = req.brand.as_ref().map(|inner| {
        inner
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    });
    let image_inner: Option<Option<&str>> = req.image_url.as_ref().map(|inner| {
        inner
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    });

    let updated = qm_db::products::update(
        &state.db,
        id,
        &ProductUpdate {
            name: name_trim,
            brand: brand_inner,
            family: req.family.as_deref(),
            preferred_unit: req.preferred_unit.as_deref(),
            image_url: image_inner,
        },
    )
    .await?;

    Ok(Json(updated.into()))
}

#[utoipa::path(
    delete,
    path = "/products/{id}",
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
    qm_db::products::invalidate_barcode_cache_for(&state.db, id).await?;

    let response = fetch_and_cache(&state, &barcode).await?;
    Ok(Json(response.0.product))
}

// ----- helpers -----

async fn fetch_and_cache(state: &AppState, barcode: &str) -> ApiResult<Json<BarcodeLookupResponse>> {
    let off = OpenFoodFactsClient::new(state.http.clone());
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
                product: row.into(),
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

fn validate_family(f: &str) -> ApiResult<()> {
    if matches!(f, "mass" | "volume" | "count") {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!(
            "family must be one of mass, volume, count (got {f})",
        )))
    }
}
