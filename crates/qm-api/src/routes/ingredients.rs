use std::str::FromStr;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use qm_core::units::UnitFamily;
use qm_db::ingredients::{
    IngredientAvailabilityRow, IngredientProductMappingRow, IngredientRow,
    NewIngredientProductMapping, NewProductRecipeMetadata, ProductRecipeMetadataRow,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    types::{ConversionProvenance, IngredientMatchKind},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ingredients", get(list).post(create))
        .route(
            "/ingredients/{id}",
            get(get_one).put(update).delete(delete_one),
        )
        .route("/ingredients/{id}/availability", get(availability))
        .route("/ingredients/{id}/product-mappings", post(create_mapping))
        .route(
            "/ingredients/{id}/product-mappings/{mapping_id}",
            delete(delete_mapping),
        )
        .route(
            "/products/{id}/recipe-metadata",
            get(get_product_metadata).put(put_product_metadata),
        )
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct IngredientListQuery {
    pub q: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngredientListResponse {
    pub items: Vec<IngredientDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngredientDto {
    pub id: Uuid,
    pub display_name: String,
    pub category: Option<String>,
    pub default_family: Option<UnitFamily>,
    pub aliases: Vec<String>,
    pub dietary_tags: Vec<String>,
    pub allergen_tags: Vec<String>,
    pub notes: Option<String>,
    pub mappings: Vec<IngredientProductMappingDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngredientProductMappingDto {
    pub id: Uuid,
    pub ingredient_id: Uuid,
    pub product_id: Uuid,
    pub rank: i64,
    pub match_kind: IngredientMatchKind,
    pub match_metadata: Value,
    pub conversion: Option<IngredientProductConversionDto>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IngredientProductConversionDto {
    pub recipe_quantity: StructuredQuantityDto,
    pub inventory_quantity: StructuredQuantityDto,
    pub provenance: ConversionProvenance,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StructuredQuantityDto {
    pub amount: Option<String>,
    pub unit: Option<String>,
    pub family: Option<UnitFamily>,
    pub range: Option<QuantityRangeDto>,
    #[serde(default)]
    pub to_taste: bool,
    pub preparation_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QuantityRangeDto {
    pub min: String,
    pub max: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateIngredientRequest {
    pub display_name: String,
    pub category: Option<String>,
    pub default_family: Option<UnitFamily>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub dietary_tags: Vec<String>,
    #[serde(default)]
    pub allergen_tags: Vec<String>,
    pub notes: Option<String>,
}

pub type UpdateIngredientRequest = CreateIngredientRequest;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateIngredientProductMappingRequest {
    pub product_id: Uuid,
    #[serde(default)]
    pub rank: i64,
    pub match_kind: IngredientMatchKind,
    #[serde(default)]
    pub match_metadata: Value,
    pub conversion: Option<IngredientProductConversionDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngredientAvailabilityResponse {
    pub items: Vec<IngredientAvailabilityDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngredientAvailabilityDto {
    pub ingredient_id: Uuid,
    pub mapping_id: Uuid,
    pub product_id: Uuid,
    pub product_name: String,
    pub location_id: Uuid,
    pub location_name: String,
    pub batch_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProductRecipeMetadataDto {
    pub product_id: Uuid,
    pub edible_yield_percent: Option<String>,
    pub drained_quantity: Option<String>,
    pub drained_unit: Option<String>,
    pub density_recipe_quantity: Option<String>,
    pub density_recipe_unit: Option<String>,
    pub density_inventory_quantity: Option<String>,
    pub density_inventory_unit: Option<String>,
    pub density_provenance: Option<ConversionProvenance>,
    pub preparation_state: Option<String>,
    pub counts_as_aliases: Vec<String>,
    pub notes: Option<String>,
    pub updated_at: Option<String>,
}

pub type UpsertProductRecipeMetadataRequest = ProductRecipeMetadataDto;

#[utoipa::path(
    get,
    path = "/ingredients",
    operation_id = "ingredient_list",
    tag = "ingredients",
    params(IngredientListQuery),
    responses((status = 200, body = IngredientListResponse)),
    security(("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(q): Query<IngredientListQuery>,
) -> ApiResult<Json<IngredientListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let rows = qm_db::ingredients::list(&state.db, household_id, q.q.as_deref(), limit).await?;
    let items = ingredients_into_dtos(&state, household_id, rows).await?;
    Ok(Json(IngredientListResponse { items }))
}

#[utoipa::path(
    post,
    path = "/ingredients",
    operation_id = "ingredient_create",
    tag = "ingredients",
    request_body = CreateIngredientRequest,
    responses((status = 201, body = IngredientDto)),
    security(("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateIngredientRequest>,
) -> ApiResult<(StatusCode, Json<IngredientDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let new = validate_ingredient_request(req)?;
    let new_row = (&new).into();
    let row = qm_db::ingredients::create(&state.db, household_id, &new_row).await?;
    let dto = ingredient_into_dto(&state, household_id, row).await?;
    Ok((StatusCode::CREATED, Json(dto)))
}

#[utoipa::path(
    get,
    path = "/ingredients/{id}",
    operation_id = "ingredient_get",
    tag = "ingredients",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = IngredientDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<IngredientDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::ingredients::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ingredient_into_dto(&state, household_id, row).await?))
}

#[utoipa::path(
    put,
    path = "/ingredients/{id}",
    operation_id = "ingredient_update",
    tag = "ingredients",
    params(("id" = Uuid, Path)),
    request_body = UpdateIngredientRequest,
    responses((status = 200, body = IngredientDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateIngredientRequest>,
) -> ApiResult<Json<IngredientDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let upd = validate_ingredient_request(req)?;
    let upd_row = (&upd).into();
    let row = qm_db::ingredients::update(&state.db, household_id, id, &upd_row)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ingredient_into_dto(&state, household_id, row).await?))
}

#[utoipa::path(
    delete,
    path = "/ingredients/{id}",
    operation_id = "ingredient_delete",
    tag = "ingredients",
    params(("id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn delete_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    if !qm_db::ingredients::delete(&state.db, household_id, id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/ingredients/{id}/product-mappings",
    operation_id = "ingredient_product_mapping_create",
    tag = "ingredients",
    params(("id" = Uuid, Path)),
    request_body = CreateIngredientProductMappingRequest,
    responses((status = 201, body = IngredientProductMappingDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn create_mapping(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateIngredientProductMappingRequest>,
) -> ApiResult<(StatusCode, Json<IngredientProductMappingDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    ensure_ingredient_exists(&state, household_id, id).await?;
    qm_db::products::find_for_household(&state.db, household_id, req.product_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let sanitized = validate_mapping_request(req)?;
    let new_row = (&sanitized).into();
    let row = qm_db::ingredients::create_mapping(&state.db, household_id, id, &new_row).await?;
    Ok((StatusCode::CREATED, Json(mapping_into_dto(row)?)))
}

#[utoipa::path(
    delete,
    path = "/ingredients/{id}/product-mappings/{mapping_id}",
    operation_id = "ingredient_product_mapping_delete",
    tag = "ingredients",
    params(("id" = Uuid, Path), ("mapping_id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn delete_mapping(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((id, mapping_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    if !qm_db::ingredients::delete_mapping(&state.db, household_id, id, mapping_id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/ingredients/{id}/availability",
    operation_id = "ingredient_availability",
    tag = "ingredients",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = IngredientAvailabilityResponse), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn availability(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<IngredientAvailabilityResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    ensure_ingredient_exists(&state, household_id, id).await?;
    let rows = qm_db::ingredients::list_availability(&state.db, household_id, id).await?;
    Ok(Json(IngredientAvailabilityResponse {
        items: rows.into_iter().map(availability_into_dto).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/products/{id}/recipe-metadata",
    operation_id = "product_recipe_metadata_get",
    tag = "ingredients",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = ProductRecipeMetadataDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_product_metadata(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProductRecipeMetadataDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    ensure_product_exists(&state, household_id, id).await?;
    let Some(row) = qm_db::ingredients::find_product_metadata(&state.db, household_id, id).await?
    else {
        return Err(ApiError::NotFound);
    };
    Ok(Json(product_metadata_into_dto(row)?))
}

#[utoipa::path(
    put,
    path = "/products/{id}/recipe-metadata",
    operation_id = "product_recipe_metadata_put",
    tag = "ingredients",
    params(("id" = Uuid, Path)),
    request_body = UpsertProductRecipeMetadataRequest,
    responses((status = 200, body = ProductRecipeMetadataDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn put_product_metadata(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpsertProductRecipeMetadataRequest>,
) -> ApiResult<Json<ProductRecipeMetadataDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    ensure_product_exists(&state, household_id, id).await?;
    let sanitized = validate_product_metadata_request(id, req)?;
    let new_row = (&sanitized).into();
    let row =
        qm_db::ingredients::upsert_product_metadata(&state.db, household_id, id, &new_row).await?;
    Ok(Json(product_metadata_into_dto(row)?))
}

async fn ingredients_into_dtos(
    state: &AppState,
    household_id: Uuid,
    rows: Vec<IngredientRow>,
) -> ApiResult<Vec<IngredientDto>> {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(ingredient_into_dto(state, household_id, row).await?);
    }
    Ok(items)
}

async fn ingredient_into_dto(
    state: &AppState,
    household_id: Uuid,
    row: IngredientRow,
) -> ApiResult<IngredientDto> {
    let mappings = qm_db::ingredients::list_mappings(&state.db, household_id, row.id).await?;
    let default_family = match row.default_family.as_deref() {
        Some(family) => Some(UnitFamily::from_str_ci(family).ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "unknown ingredient default family in DB row: {family}",
            ))
        })?),
        None => None,
    };
    Ok(IngredientDto {
        id: row.id,
        display_name: row.display_name,
        category: row.category,
        default_family,
        aliases: json_string_vec(&row.aliases_json)?,
        dietary_tags: json_string_vec(&row.dietary_tags_json)?,
        allergen_tags: json_string_vec(&row.allergen_tags_json)?,
        notes: row.notes,
        mappings: mappings
            .into_iter()
            .map(mapping_into_dto)
            .collect::<ApiResult<_>>()?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn mapping_into_dto(row: IngredientProductMappingRow) -> ApiResult<IngredientProductMappingDto> {
    let match_kind = IngredientMatchKind::from_str(&row.match_kind)?;
    let match_metadata = serde_json::from_str(&row.match_metadata_json).map_err(|err| {
        ApiError::Internal(anyhow::anyhow!(
            "invalid ingredient mapping metadata JSON for {}: {err}",
            row.id
        ))
    })?;
    let conversion = match row.conversion_provenance.as_deref() {
        Some(value) => Some(IngredientProductConversionDto {
            recipe_quantity: StructuredQuantityDto {
                amount: row.recipe_amount,
                unit: row.recipe_unit,
                family: parse_optional_family(row.recipe_family.as_deref())?,
                range: optional_range(row.recipe_range_min, row.recipe_range_max),
                to_taste: row.recipe_to_taste,
                preparation_note: row.recipe_preparation_note,
            },
            inventory_quantity: StructuredQuantityDto {
                amount: row.inventory_amount,
                unit: row.inventory_unit,
                family: parse_optional_family(row.inventory_family.as_deref())?,
                range: optional_range(row.inventory_range_min, row.inventory_range_max),
                to_taste: row.inventory_to_taste,
                preparation_note: row.inventory_preparation_note,
            },
            provenance: ConversionProvenance::from_str(value)?,
            notes: row.conversion_notes,
        }),
        None => None,
    };
    Ok(IngredientProductMappingDto {
        id: row.id,
        ingredient_id: row.ingredient_id,
        product_id: row.product_id,
        rank: row.rank,
        match_kind,
        match_metadata,
        conversion,
        created_at: row.created_at,
    })
}

fn availability_into_dto(row: IngredientAvailabilityRow) -> IngredientAvailabilityDto {
    IngredientAvailabilityDto {
        ingredient_id: row.ingredient_id,
        mapping_id: row.mapping_id,
        product_id: row.product_id,
        product_name: row.product_name,
        location_id: row.location_id,
        location_name: row.location_name,
        batch_id: row.batch_id,
        quantity: row.quantity,
        unit: row.unit,
        expires_on: row.expires_on,
    }
}

fn product_metadata_into_dto(row: ProductRecipeMetadataRow) -> ApiResult<ProductRecipeMetadataDto> {
    Ok(ProductRecipeMetadataDto {
        product_id: row.product_id,
        edible_yield_percent: row.edible_yield_percent,
        drained_quantity: row.drained_quantity,
        drained_unit: row.drained_unit,
        density_recipe_quantity: row.density_recipe_quantity,
        density_recipe_unit: row.density_recipe_unit,
        density_inventory_quantity: row.density_inventory_quantity,
        density_inventory_unit: row.density_inventory_unit,
        density_provenance: row
            .density_provenance
            .as_deref()
            .map(ConversionProvenance::from_str)
            .transpose()?,
        preparation_state: row.preparation_state,
        counts_as_aliases: json_string_vec(&row.counts_as_aliases_json)?,
        notes: row.notes,
        updated_at: Some(row.updated_at),
    })
}

struct SanitizedIngredient {
    display_name: String,
    category: Option<String>,
    default_family: Option<UnitFamily>,
    aliases_json: String,
    dietary_tags_json: String,
    allergen_tags_json: String,
    notes: Option<String>,
}

impl<'a> From<&'a SanitizedIngredient> for qm_db::ingredients::NewIngredient<'a> {
    fn from(value: &'a SanitizedIngredient) -> Self {
        Self {
            display_name: &value.display_name,
            category: value.category.as_deref(),
            default_family: value.default_family.map(UnitFamily::as_str),
            aliases_json: &value.aliases_json,
            dietary_tags_json: &value.dietary_tags_json,
            allergen_tags_json: &value.allergen_tags_json,
            notes: value.notes.as_deref(),
        }
    }
}

fn validate_ingredient_request(req: CreateIngredientRequest) -> ApiResult<SanitizedIngredient> {
    let display_name = required_text("display_name", req.display_name, 256)?;
    Ok(SanitizedIngredient {
        display_name,
        category: optional_text("category", req.category, 128)?,
        default_family: req.default_family,
        aliases_json: serde_json::to_string(&validate_text_list("aliases", req.aliases, 64, 128)?)
            .map_err(|err| ApiError::Internal(err.into()))?,
        dietary_tags_json: serde_json::to_string(&validate_text_list(
            "dietary_tags",
            req.dietary_tags,
            64,
            64,
        )?)
        .map_err(|err| ApiError::Internal(err.into()))?,
        allergen_tags_json: serde_json::to_string(&validate_text_list(
            "allergen_tags",
            req.allergen_tags,
            64,
            64,
        )?)
        .map_err(|err| ApiError::Internal(err.into()))?,
        notes: optional_text("notes", req.notes, 2048)?,
    })
}

struct SanitizedMapping {
    product_id: Uuid,
    rank: i64,
    match_kind: IngredientMatchKind,
    match_metadata_json: String,
    recipe: Option<SanitizedQuantity>,
    inventory: Option<SanitizedQuantity>,
    conversion_provenance: Option<ConversionProvenance>,
    conversion_notes: Option<String>,
}

impl<'a> From<&'a SanitizedMapping> for NewIngredientProductMapping<'a> {
    fn from(value: &'a SanitizedMapping) -> Self {
        Self {
            product_id: value.product_id,
            rank: value.rank,
            match_kind: value.match_kind.as_str(),
            match_metadata_json: &value.match_metadata_json,
            recipe_amount: value.recipe.as_ref().and_then(|q| q.amount.as_deref()),
            recipe_unit: value.recipe.as_ref().and_then(|q| q.unit.as_deref()),
            recipe_family: value
                .recipe
                .as_ref()
                .and_then(|q| q.family.map(UnitFamily::as_str)),
            recipe_range_min: value.recipe.as_ref().and_then(|q| q.range_min.as_deref()),
            recipe_range_max: value.recipe.as_ref().and_then(|q| q.range_max.as_deref()),
            recipe_to_taste: value.recipe.as_ref().is_some_and(|q| q.to_taste),
            recipe_preparation_note: value
                .recipe
                .as_ref()
                .and_then(|q| q.preparation_note.as_deref()),
            inventory_amount: value.inventory.as_ref().and_then(|q| q.amount.as_deref()),
            inventory_unit: value.inventory.as_ref().and_then(|q| q.unit.as_deref()),
            inventory_family: value
                .inventory
                .as_ref()
                .and_then(|q| q.family.map(UnitFamily::as_str)),
            inventory_range_min: value
                .inventory
                .as_ref()
                .and_then(|q| q.range_min.as_deref()),
            inventory_range_max: value
                .inventory
                .as_ref()
                .and_then(|q| q.range_max.as_deref()),
            inventory_to_taste: value.inventory.as_ref().is_some_and(|q| q.to_taste),
            inventory_preparation_note: value
                .inventory
                .as_ref()
                .and_then(|q| q.preparation_note.as_deref()),
            conversion_provenance: value
                .conversion_provenance
                .map(ConversionProvenance::as_str),
            conversion_notes: value.conversion_notes.as_deref(),
        }
    }
}

fn validate_mapping_request(
    req: CreateIngredientProductMappingRequest,
) -> ApiResult<SanitizedMapping> {
    let conversion = req.conversion.map(validate_conversion).transpose()?;
    let (recipe, inventory, conversion_provenance, conversion_notes) = match conversion {
        Some(value) => (
            Some(value.recipe),
            Some(value.inventory),
            Some(value.provenance),
            value.notes,
        ),
        None => (None, None, None, None),
    };
    let match_metadata_json =
        serde_json::to_string(&req.match_metadata).map_err(|err| ApiError::Internal(err.into()))?;
    Ok(SanitizedMapping {
        product_id: req.product_id,
        rank: req.rank.clamp(0, 10_000),
        match_kind: req.match_kind,
        match_metadata_json,
        recipe,
        inventory,
        conversion_provenance,
        conversion_notes,
    })
}

struct SanitizedConversion {
    recipe: SanitizedQuantity,
    inventory: SanitizedQuantity,
    provenance: ConversionProvenance,
    notes: Option<String>,
}

fn validate_conversion(value: IngredientProductConversionDto) -> ApiResult<SanitizedConversion> {
    let recipe = validate_quantity("recipe_quantity", value.recipe_quantity, true)?;
    let inventory = validate_quantity("inventory_quantity", value.inventory_quantity, true)?;
    Ok(SanitizedConversion {
        recipe,
        inventory,
        provenance: value.provenance,
        notes: optional_text("conversion.notes", value.notes, 512)?,
    })
}

#[derive(Debug)]
struct SanitizedQuantity {
    amount: Option<String>,
    unit: Option<String>,
    family: Option<UnitFamily>,
    range_min: Option<String>,
    range_max: Option<String>,
    to_taste: bool,
    preparation_note: Option<String>,
}

fn validate_quantity(
    field: &str,
    value: StructuredQuantityDto,
    require_ratio_quantity: bool,
) -> ApiResult<SanitizedQuantity> {
    let amount = match value.amount {
        Some(amount) => Some(validate_positive_decimal(
            &format!("{field}.amount"),
            amount,
        )?),
        None => None,
    };
    let unit = optional_text(&format!("{field}.unit"), value.unit, 64)?;
    if require_ratio_quantity && (amount.is_none() || unit.is_none()) {
        return Err(ApiError::BadRequest(format!(
            "{field} requires amount and unit for conversion metadata"
        )));
    }
    let unit_family = if let Some(unit) = unit.as_deref() {
        Some(
            qm_core::units::lookup(unit)
                .map_err(|_| ApiError::UnknownUnit(unit.to_owned()))?
                .family,
        )
    } else {
        None
    };
    if let (Some(expected), Some(actual)) = (value.family, unit_family) {
        if expected != actual {
            return Err(ApiError::UnitFamilyMismatch {
                product_family: expected.as_str().to_owned(),
                unit: unit.unwrap_or_default(),
            });
        }
    }
    let (range_min, range_max) = match value.range {
        Some(range) => (
            Some(validate_positive_decimal(
                &format!("{field}.range.min"),
                range.min,
            )?),
            Some(validate_positive_decimal(
                &format!("{field}.range.max"),
                range.max,
            )?),
        ),
        None => (None, None),
    };
    Ok(SanitizedQuantity {
        amount,
        unit,
        family: value.family.or(unit_family),
        range_min,
        range_max,
        to_taste: value.to_taste,
        preparation_note: optional_text(
            &format!("{field}.preparation_note"),
            value.preparation_note,
            256,
        )?,
    })
}

fn validate_product_metadata_request(
    id: Uuid,
    req: UpsertProductRecipeMetadataRequest,
) -> ApiResult<SanitizedProductMetadata> {
    if req.product_id != id {
        return Err(ApiError::BadRequest(
            "product_id must match the path product id".into(),
        ));
    }
    let edible_yield_percent = match req.edible_yield_percent {
        Some(value) => {
            let value = validate_positive_decimal("edible_yield_percent", value)?;
            let parsed = Decimal::from_str(&value).map_err(|_| {
                ApiError::BadRequest("edible_yield_percent must be a decimal".into())
            })?;
            if parsed > Decimal::new(100, 0) {
                return Err(ApiError::BadRequest(
                    "edible_yield_percent must be <= 100".into(),
                ));
            }
            Some(value)
        }
        None => None,
    };
    let drained_quantity = match req.drained_quantity {
        Some(value) => Some(validate_positive_decimal("drained_quantity", value)?),
        None => None,
    };
    let drained_unit = optional_text("drained_unit", req.drained_unit, 64)?;
    if drained_quantity.is_some() != drained_unit.is_some() {
        return Err(ApiError::BadRequest(
            "drained_quantity and drained_unit must be provided together".into(),
        ));
    }
    if let Some(unit) = drained_unit.as_deref() {
        qm_core::units::lookup(unit).map_err(|_| ApiError::UnknownUnit(unit.to_owned()))?;
    }

    let density_recipe_quantity =
        optional_decimal("density_recipe_quantity", req.density_recipe_quantity)?;
    let density_recipe_unit = optional_text("density_recipe_unit", req.density_recipe_unit, 64)?;
    let density_inventory_quantity =
        optional_decimal("density_inventory_quantity", req.density_inventory_quantity)?;
    let density_inventory_unit =
        optional_text("density_inventory_unit", req.density_inventory_unit, 64)?;
    let density_fields_present = [
        density_recipe_quantity.is_some(),
        density_recipe_unit.is_some(),
        density_inventory_quantity.is_some(),
        density_inventory_unit.is_some(),
    ];
    if density_fields_present.iter().any(|present| *present)
        && !density_fields_present.iter().all(|present| *present)
    {
        return Err(ApiError::BadRequest(
            "density conversion requires recipe quantity/unit and inventory quantity/unit".into(),
        ));
    }
    if let Some(unit) = density_recipe_unit.as_deref() {
        qm_core::units::lookup(unit).map_err(|_| ApiError::UnknownUnit(unit.to_owned()))?;
    }
    if let Some(unit) = density_inventory_unit.as_deref() {
        qm_core::units::lookup(unit).map_err(|_| ApiError::UnknownUnit(unit.to_owned()))?;
    }

    Ok(SanitizedProductMetadata {
        edible_yield_percent,
        drained_quantity,
        drained_unit,
        density_recipe_quantity,
        density_recipe_unit,
        density_inventory_quantity,
        density_inventory_unit,
        density_provenance: req.density_provenance,
        preparation_state: optional_text("preparation_state", req.preparation_state, 128)?,
        counts_as_aliases_json: serde_json::to_string(&validate_text_list(
            "counts_as_aliases",
            req.counts_as_aliases,
            64,
            128,
        )?)
        .map_err(|err| ApiError::Internal(err.into()))?,
        notes: optional_text("notes", req.notes, 2048)?,
    })
}

struct SanitizedProductMetadata {
    edible_yield_percent: Option<String>,
    drained_quantity: Option<String>,
    drained_unit: Option<String>,
    density_recipe_quantity: Option<String>,
    density_recipe_unit: Option<String>,
    density_inventory_quantity: Option<String>,
    density_inventory_unit: Option<String>,
    density_provenance: Option<ConversionProvenance>,
    preparation_state: Option<String>,
    counts_as_aliases_json: String,
    notes: Option<String>,
}

impl<'a> From<&'a SanitizedProductMetadata> for NewProductRecipeMetadata<'a> {
    fn from(value: &'a SanitizedProductMetadata) -> Self {
        Self {
            edible_yield_percent: value.edible_yield_percent.as_deref(),
            drained_quantity: value.drained_quantity.as_deref(),
            drained_unit: value.drained_unit.as_deref(),
            density_recipe_quantity: value.density_recipe_quantity.as_deref(),
            density_recipe_unit: value.density_recipe_unit.as_deref(),
            density_inventory_quantity: value.density_inventory_quantity.as_deref(),
            density_inventory_unit: value.density_inventory_unit.as_deref(),
            density_provenance: value.density_provenance.map(ConversionProvenance::as_str),
            preparation_state: value.preparation_state.as_deref(),
            counts_as_aliases_json: &value.counts_as_aliases_json,
            notes: value.notes.as_deref(),
        }
    }
}

async fn ensure_ingredient_exists(state: &AppState, household_id: Uuid, id: Uuid) -> ApiResult<()> {
    qm_db::ingredients::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(())
}

async fn ensure_product_exists(state: &AppState, household_id: Uuid, id: Uuid) -> ApiResult<()> {
    qm_db::products::find_for_household(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(())
}

fn parse_optional_family(value: Option<&str>) -> ApiResult<Option<UnitFamily>> {
    value
        .map(|family| {
            UnitFamily::from_str_ci(family).ok_or_else(|| {
                ApiError::Internal(anyhow::anyhow!(
                    "unknown ingredient quantity family in DB row: {family}",
                ))
            })
        })
        .transpose()
}

fn optional_range(min: Option<String>, max: Option<String>) -> Option<QuantityRangeDto> {
    match (min, max) {
        (Some(min), Some(max)) => Some(QuantityRangeDto { min, max }),
        _ => None,
    }
}

fn json_string_vec(raw: &str) -> ApiResult<Vec<String>> {
    serde_json::from_str(raw).map_err(|err| {
        ApiError::Internal(anyhow::anyhow!(
            "invalid string-list JSON stored in ingredient data: {err}"
        ))
    })
}

fn required_text(field: &str, value: String, max_len: usize) -> ApiResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_len {
        return Err(ApiError::BadRequest(format!(
            "{field} must be 1..={max_len} chars"
        )));
    }
    Ok(value.to_owned())
}

fn optional_text(field: &str, value: Option<String>, max_len: usize) -> ApiResult<Option<String>> {
    value
        .map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.len() > max_len {
                return Err(ApiError::BadRequest(format!(
                    "{field} must be <= {max_len} chars"
                )));
            }
            Ok(Some(trimmed.to_owned()))
        })
        .transpose()
        .map(Option::flatten)
}

fn validate_text_list(
    field: &str,
    values: Vec<String>,
    max_items: usize,
    max_len: usize,
) -> ApiResult<Vec<String>> {
    if values.len() > max_items {
        return Err(ApiError::BadRequest(format!(
            "{field} must have at most {max_items} items"
        )));
    }
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        let value = required_text(field, value, max_len)?;
        if !out.iter().any(|existing| existing == &value) {
            out.push(value);
        }
    }
    Ok(out)
}

fn optional_decimal(field: &str, value: Option<String>) -> ApiResult<Option<String>> {
    value
        .map(|value| validate_positive_decimal(field, value))
        .transpose()
}

fn validate_positive_decimal(field: &str, value: String) -> ApiResult<String> {
    let value = value.trim();
    let parsed = Decimal::from_str(value)
        .map_err(|_| ApiError::BadRequest(format!("{field} must be a decimal")))?;
    if parsed <= Decimal::ZERO {
        return Err(ApiError::BadRequest(format!("{field} must be > 0")));
    }
    Ok(value.to_owned())
}
