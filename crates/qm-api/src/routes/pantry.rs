use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    time::Instant,
};

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch},
    Json, Router,
};
use jiff::{civil::Date, tz, Timestamp};
use qm_core::units::MeasurementSystem;
use qm_db::{
    pantry_suggestions::{NewPantrySuggestion, PantrySuggestionRow},
    products::ProductRow,
    recipes::{RecipeFull, RecipeIngredientRow},
    stock::StockFilter,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use utoipa::{
    openapi::{
        schema::{
            AllOfBuilder, ArrayBuilder, KnownFormat, ObjectBuilder, Schema, SchemaFormat,
            SchemaType, Type,
        },
        Ref, RefOr,
    },
    IntoParams, PartialSchema, ToSchema,
};
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    routes::{
        products::ProductDto,
        recipes::{RecipeIngredientDto, RecipeStepDto},
    },
    types::{AiTaskUserState, PantrySuggestionSource, PantrySuggestionStatus},
    AppState,
};

const PROMPT_VERSION: &str = "pantry-suggestion.v2";
const MAX_AI_INGREDIENTS: i64 = 8;
const MAX_AI_STEPS: i64 = 6;
const MAX_AI_TIMERS: i64 = 3;
const MAX_AI_EQUIPMENT: i64 = 5;
const MAX_AI_INGREDIENT_REFS: i64 = 8;
const MAX_AI_LIST_ITEMS: i64 = 5;
const MAX_AI_SUBSTITUTION_HINTS: i64 = 3;
const MAX_PROMPT_EXPIRING_BATCHES: usize = 3;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/pantry/suggestions",
            get(list_suggestions).post(create_suggestions),
        )
        .route("/pantry/suggestions/{id}", get(get_suggestion))
        .route(
            "/pantry/suggestions/{id}/state",
            patch(update_suggestion_state),
        )
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PantrySuggestionListQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePantrySuggestionsRequest {
    #[serde(default)]
    pub excluded_product_ids: Vec<Uuid>,
    #[serde(default)]
    pub excluded_location_ids: Vec<Uuid>,
    #[serde(default)]
    pub dietary_constraints: Vec<String>,
    #[serde(default)]
    pub equipment: Vec<String>,
    pub max_missing_required: Option<i64>,
    #[serde(default)]
    pub generate_recipe_ideas: bool,
    pub max_ai_suggestions: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PantrySuggestionsResponse {
    pub context: PantryContextDto,
    pub suggestions: Vec<PantrySuggestionDto>,
    pub generation_task: Option<Uuid>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PantrySuggestionListResponse {
    pub items: Vec<PantrySuggestionDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PantryContextDto {
    pub inventory: Vec<PantryInventoryItemDto>,
    pub excluded_product_ids: Vec<Uuid>,
    pub excluded_location_ids: Vec<Uuid>,
    pub dietary_constraints: Vec<String>,
    pub equipment: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PantryInventoryItemDto {
    pub product: ProductDto,
    pub total_quantity: String,
    pub unit: String,
    pub expiry_urgency: PantryExpiryUrgency,
    pub batches: Vec<PantryInventoryBatchDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PantryInventoryBatchDto {
    pub batch_id: Uuid,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
    pub expiry_urgency: PantryExpiryUrgency,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PantryExpiryUrgency {
    None,
    Future,
    Soon,
    Today,
    Expired,
}

#[derive(Debug, Serialize)]
pub struct PantrySuggestionDto {
    pub id: Uuid,
    pub source: PantrySuggestionSource,
    pub status: PantrySuggestionStatus,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub ai_task_id: Option<Uuid>,
    pub title: String,
    pub summary: Option<String>,
    pub score: i64,
    pub score_breakdown: PantrySuggestionScoreDto,
    pub missing: Vec<PantrySuggestionMissingDto>,
    pub pantry_items: Vec<Uuid>,
    pub generated_recipe: Option<GeneratedRecipeIdeaDto>,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

impl PartialSchema for PantrySuggestionDto {
    fn schema() -> RefOr<Schema> {
        let nullable_generated_recipe = Schema::AllOf(
            AllOfBuilder::new()
                .item(Ref::from_schema_name(GeneratedRecipeIdeaDto::name()))
                .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
                .build(),
        );

        ObjectBuilder::new()
            .property("id", uuid_schema())
            .required("id")
            .property(
                "source",
                Ref::from_schema_name(PantrySuggestionSource::name()),
            )
            .required("source")
            .property(
                "status",
                Ref::from_schema_name(PantrySuggestionStatus::name()),
            )
            .required("status")
            .property("recipe_id", nullable_uuid_schema())
            .property("recipe_version_id", nullable_uuid_schema())
            .property("ai_task_id", nullable_uuid_schema())
            .property("title", String::schema())
            .required("title")
            .property("summary", nullable_string_schema())
            .property("score", i64::schema())
            .required("score")
            .property(
                "score_breakdown",
                Ref::from_schema_name(PantrySuggestionScoreDto::name()),
            )
            .required("score_breakdown")
            .property(
                "missing",
                ArrayBuilder::new()
                    .items(Ref::from_schema_name(PantrySuggestionMissingDto::name()))
                    .build(),
            )
            .required("missing")
            .property(
                "pantry_items",
                ArrayBuilder::new().items(uuid_schema()).build(),
            )
            .required("pantry_items")
            .property("generated_recipe", nullable_generated_recipe)
            .property("created_by", nullable_uuid_schema())
            .property("created_at", String::schema())
            .required("created_at")
            .property("updated_at", String::schema())
            .required("updated_at")
            .into()
    }
}

fn uuid_schema() -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .format(Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)))
}

fn nullable_uuid_schema() -> ObjectBuilder {
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::String, Type::Null]))
        .format(Some(SchemaFormat::KnownFormat(KnownFormat::Uuid)))
}

fn nullable_string_schema() -> ObjectBuilder {
    ObjectBuilder::new().schema_type(SchemaType::from_iter([Type::String, Type::Null]))
}

impl ToSchema for PantrySuggestionDto {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((
            GeneratedRecipeIdeaDto::name().into_owned(),
            GeneratedRecipeIdeaDto::schema(),
        ));
        <GeneratedRecipeIdeaDto as ToSchema>::schemas(schemas);
        schemas.push((
            PantrySuggestionScoreDto::name().into_owned(),
            PantrySuggestionScoreDto::schema(),
        ));
        <PantrySuggestionScoreDto as ToSchema>::schemas(schemas);
        schemas.push((
            PantrySuggestionMissingDto::name().into_owned(),
            PantrySuggestionMissingDto::schema(),
        ));
        <PantrySuggestionMissingDto as ToSchema>::schemas(schemas);
        schemas.push((
            PantrySuggestionSource::name().into_owned(),
            PantrySuggestionSource::schema(),
        ));
        <PantrySuggestionSource as ToSchema>::schemas(schemas);
        schemas.push((
            PantrySuggestionStatus::name().into_owned(),
            PantrySuggestionStatus::schema(),
        ));
        <PantrySuggestionStatus as ToSchema>::schemas(schemas);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PantrySuggestionScoreDto {
    pub cookable: bool,
    pub required_missing_count: i64,
    pub optional_missing_count: i64,
    pub unresolved_count: i64,
    pub expiring_match_count: i64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PantrySuggestionMissingDto {
    pub display_name: String,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub optional: bool,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GeneratedRecipeIdeaDto {
    pub name: String,
    pub description: Option<String>,
    pub serving_count: String,
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredientDto>,
    #[serde(default)]
    pub steps: Vec<RecipeStepDto>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub unresolved_conversions: Vec<String>,
    #[serde(default)]
    pub substitutions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AiPantryInputSummary<'a> {
    inventory: Vec<AiPantryInventoryItem<'a>>,
    dietary_constraints: &'a [String],
    equipment: &'a [String],
    max_suggestions: i64,
    policy: &'static str,
    output_guidance: &'static str,
}

#[derive(Debug, Serialize)]
struct AiPantryInventoryItem<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    brand: Option<&'a str>,
    family: &'static str,
    quantity: &'a str,
    unit: &'a str,
    expiry_urgency: PantryExpiryUrgency,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    expiring_batches: Vec<AiPantryInventoryBatch<'a>>,
}

#[derive(Debug, Serialize)]
struct AiPantryInventoryBatch<'a> {
    quantity: &'a str,
    unit: &'a str,
    expires_on: Option<&'a str>,
    expiry_urgency: PantryExpiryUrgency,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePantrySuggestionStateRequest {
    pub status: PantrySuggestionStatus,
}

#[utoipa::path(
    post,
    path = "/pantry/suggestions",
    operation_id = "pantry_suggestions_create",
    tag = "pantry",
    request_body = CreatePantrySuggestionsRequest,
    responses((status = 200, body = PantrySuggestionsResponse)),
    security(("bearer" = [])),
)]
pub async fn create_suggestions(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreatePantrySuggestionsRequest>,
) -> ApiResult<Json<PantrySuggestionsResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let constraints = SanitizedSuggestionConstraints::from_request(req)?;
    let ctx = build_pantry_context(&state, household_id, &constraints).await?;
    let recipe_scores = score_saved_recipes(&state, household_id, &ctx, &constraints).await?;
    let inventory_item_count = ctx.dto.inventory.len();
    let inventory_batch_count = ctx
        .dto
        .inventory
        .iter()
        .map(|item| item.batches.len())
        .sum::<usize>();
    tracing::info!(
        inventory_item_count,
        inventory_batch_count,
        saved_recipe_suggestion_count = recipe_scores.len(),
        generate_recipe_ideas = constraints.generate_recipe_ideas,
        max_missing_required = constraints.max_missing_required,
        max_ai_suggestions = constraints.max_ai_suggestions,
        "built pantry suggestion context"
    );
    let mut suggestions = Vec::with_capacity(recipe_scores.len());
    for score in recipe_scores {
        let row = insert_suggestion(&state, household_id, current.user_id, &score, None).await?;
        suggestions.push(suggestion_into_dto(row)?);
    }

    let mut warnings = Vec::new();
    let mut generation_task = None;
    if constraints.generate_recipe_ideas {
        match generate_recipe_ideas(&state, household_id, current.user_id, &ctx, &constraints).await
        {
            Ok(generated) => {
                generation_task = Some(generated.task_id);
                let generated_suggestion_count = generated.suggestions.len();
                if !generated.validation_errors.is_empty() {
                    tracing::warn!(
                        ai_task_id = %generated.task_id,
                        validation_error_count = generated.validation_errors.len(),
                        "AI pantry generation returned invalid candidates"
                    );
                    warnings.push(format!(
                        "AI recipe generation returned invalid candidates: {}",
                        generated.validation_errors.join("; ")
                    ));
                }
                for score in generated.suggestions {
                    let row = insert_suggestion(
                        &state,
                        household_id,
                        current.user_id,
                        &score,
                        Some(generated.task_id),
                    )
                    .await?;
                    suggestions.push(suggestion_into_dto(row)?);
                }
                tracing::info!(
                    ai_task_id = %generated.task_id,
                    generated_suggestion_count,
                    "stored AI pantry suggestions"
                );
            }
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    inventory_item_count,
                    inventory_batch_count,
                    "AI pantry generation failed"
                );
                warnings.push(err);
            }
        }
    }

    suggestions.sort_by(|a, b| b.score.cmp(&a.score).then(a.title.cmp(&b.title)));
    Ok(Json(PantrySuggestionsResponse {
        context: ctx.dto,
        suggestions,
        generation_task,
        warnings,
    }))
}

#[utoipa::path(
    get,
    path = "/pantry/suggestions",
    operation_id = "pantry_suggestions_list",
    tag = "pantry",
    params(PantrySuggestionListQuery),
    responses((status = 200, body = PantrySuggestionListResponse)),
    security(("bearer" = [])),
)]
pub async fn list_suggestions(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<PantrySuggestionListQuery>,
) -> ApiResult<Json<PantrySuggestionListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let rows = qm_db::pantry_suggestions::list(&state.db, household_id, limit).await?;
    Ok(Json(PantrySuggestionListResponse {
        items: rows
            .into_iter()
            .map(suggestion_into_dto)
            .collect::<ApiResult<_>>()?,
    }))
}

#[utoipa::path(
    get,
    path = "/pantry/suggestions/{id}",
    operation_id = "pantry_suggestion_get",
    tag = "pantry",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = PantrySuggestionDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_suggestion(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PantrySuggestionDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::pantry_suggestions::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(suggestion_into_dto(row)?))
}

#[utoipa::path(
    patch,
    path = "/pantry/suggestions/{id}/state",
    operation_id = "pantry_suggestion_state_update",
    tag = "pantry",
    params(("id" = Uuid, Path)),
    request_body = UpdatePantrySuggestionStateRequest,
    responses((status = 200, body = PantrySuggestionDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn update_suggestion_state(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePantrySuggestionStateRequest>,
) -> ApiResult<Json<PantrySuggestionDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let row =
        qm_db::pantry_suggestions::update_status(&state.db, household_id, id, req.status.as_str())
            .await?
            .ok_or(ApiError::NotFound)?;
    Ok(Json(suggestion_into_dto(row)?))
}

struct SanitizedSuggestionConstraints {
    excluded_product_ids: HashSet<Uuid>,
    excluded_location_ids: HashSet<Uuid>,
    dietary_constraints: Vec<String>,
    equipment: Vec<String>,
    max_missing_required: i64,
    generate_recipe_ideas: bool,
    max_ai_suggestions: i64,
}

impl SanitizedSuggestionConstraints {
    fn from_request(req: CreatePantrySuggestionsRequest) -> ApiResult<Self> {
        Ok(Self {
            excluded_product_ids: req.excluded_product_ids.into_iter().collect(),
            excluded_location_ids: req.excluded_location_ids.into_iter().collect(),
            dietary_constraints: validate_text_list(
                "dietary_constraints",
                req.dietary_constraints,
                32,
                64,
            )?,
            equipment: validate_text_list("equipment", req.equipment, 64, 64)?,
            max_missing_required: req.max_missing_required.unwrap_or(2).clamp(0, 25),
            generate_recipe_ideas: req.generate_recipe_ideas,
            max_ai_suggestions: req.max_ai_suggestions.unwrap_or(2).clamp(1, 5),
        })
    }
}

struct PantryContext {
    dto: PantryContextDto,
    by_product: HashMap<Uuid, ProductInventory>,
    measurement_system: MeasurementSystem,
}

struct ProductInventory {
    product: ProductDto,
    total_quantity: Decimal,
    unit: String,
    batches: Vec<PantryInventoryBatchDto>,
}

async fn build_pantry_context(
    state: &AppState,
    household_id: Uuid,
    constraints: &SanitizedSuggestionConstraints,
) -> ApiResult<PantryContext> {
    let household = qm_db::households::find_by_id(&state.db, household_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let measurement_system =
        crate::routes::households::measurement_system_from_db(&household.measurement_system)?;
    let today = household_today(&household.timezone)?;
    let stock = qm_db::stock::list(
        &state.db,
        household_id,
        &StockFilter {
            include_depleted: false,
            ..StockFilter::default()
        },
    )
    .await?;
    let mut by_product: HashMap<Uuid, ProductInventory> = HashMap::new();
    for item in stock {
        if constraints.excluded_product_ids.contains(&item.product.id)
            || constraints
                .excluded_location_ids
                .contains(&item.batch.location_id)
        {
            continue;
        }
        let product_dto = ProductDto::try_from(item.product.clone())?;
        let unit = product_dto.preferred_unit.clone();
        let quantity = Decimal::from_str(&item.batch.quantity)
            .map_err(|err| ApiError::Internal(anyhow::Error::from(err)))?;
        let normalized_quantity =
            convert_decimal(quantity, &item.batch.unit, &unit, measurement_system)?;
        let expiry_urgency = expiry_urgency(item.batch.expires_on.as_deref(), today)?;
        let entry = by_product
            .entry(item.product.id)
            .or_insert_with(|| ProductInventory {
                product: product_dto,
                total_quantity: Decimal::ZERO,
                unit,
                batches: Vec::new(),
            });
        entry.total_quantity += normalized_quantity;
        entry.batches.push(PantryInventoryBatchDto {
            batch_id: item.batch.id,
            location_id: item.batch.location_id,
            quantity: item.batch.quantity,
            unit: item.batch.unit,
            expires_on: item.batch.expires_on,
            expiry_urgency,
        });
    }
    let mut inventory = by_product
        .values()
        .map(|item| PantryInventoryItemDto {
            product: item.product.clone(),
            total_quantity: normalize_decimal(item.total_quantity),
            unit: item.unit.clone(),
            expiry_urgency: item
                .batches
                .iter()
                .map(|batch| batch.expiry_urgency)
                .max()
                .unwrap_or(PantryExpiryUrgency::None),
            batches: item.batches.clone(),
        })
        .collect::<Vec<_>>();
    inventory.sort_by(|a, b| a.product.name.cmp(&b.product.name));
    Ok(PantryContext {
        dto: PantryContextDto {
            inventory,
            excluded_product_ids: sorted_ids(&constraints.excluded_product_ids),
            excluded_location_ids: sorted_ids(&constraints.excluded_location_ids),
            dietary_constraints: constraints.dietary_constraints.clone(),
            equipment: constraints.equipment.clone(),
        },
        by_product,
        measurement_system,
    })
}

struct ScoredSuggestion {
    source: PantrySuggestionSource,
    recipe_id: Option<Uuid>,
    recipe_version_id: Option<Uuid>,
    title: String,
    summary: Option<String>,
    score: i64,
    score_breakdown: PantrySuggestionScoreDto,
    missing: Vec<PantrySuggestionMissingDto>,
    pantry_items: Vec<Uuid>,
    generated_recipe: Option<GeneratedRecipeIdeaDto>,
}

async fn score_saved_recipes(
    state: &AppState,
    household_id: Uuid,
    ctx: &PantryContext,
    constraints: &SanitizedSuggestionConstraints,
) -> ApiResult<Vec<ScoredSuggestion>> {
    let recipes = qm_db::recipes::list(&state.db, household_id).await?;
    let mut suggestions = Vec::new();
    for recipe in recipes {
        let Some(full) = qm_db::recipes::find(&state.db, household_id, recipe.id).await? else {
            continue;
        };
        let scored = score_recipe(state, household_id, ctx, &full).await?;
        if scored.score_breakdown.required_missing_count <= constraints.max_missing_required {
            suggestions.push(scored);
        }
    }
    suggestions.sort_by(|a, b| b.score.cmp(&a.score).then(a.title.cmp(&b.title)));
    Ok(suggestions.into_iter().take(20).collect())
}

async fn score_recipe(
    state: &AppState,
    household_id: Uuid,
    ctx: &PantryContext,
    recipe: &RecipeFull,
) -> ApiResult<ScoredSuggestion> {
    let mut missing = Vec::new();
    let mut pantry_items = HashSet::new();
    let mut required_missing = 0;
    let mut optional_missing = 0;
    let mut unresolved = 0;
    let mut expiring_match_count = 0;
    let mut notes = Vec::new();

    for ingredient in &recipe.ingredients {
        let Some((product, source_note)) =
            resolve_recipe_product(state, household_id, ingredient).await?
        else {
            unresolved += 1;
            missing.push(missing_from_ingredient(
                ingredient,
                "no product mapping selected",
            ));
            continue;
        };
        if let Some(note) = source_note {
            notes.push(note);
        }
        let Some(amount) = ingredient.amount.as_deref() else {
            if !ingredient.to_taste {
                unresolved += 1;
            }
            continue;
        };
        let Some(unit) = ingredient.unit.as_deref() else {
            unresolved += 1;
            missing.push(missing_from_ingredient(ingredient, "missing recipe unit"));
            continue;
        };
        let requested = Decimal::from_str(amount)
            .map_err(|err| ApiError::Internal(anyhow::Error::from(err)))?;
        let Some(inventory) = ctx.by_product.get(&product.id) else {
            if ingredient.optional {
                optional_missing += 1;
            } else {
                required_missing += 1;
            }
            missing.push(missing_from_ingredient(
                ingredient,
                "not in active inventory",
            ));
            continue;
        };
        let available = convert_decimal(
            inventory.total_quantity,
            &inventory.unit,
            unit,
            ctx.measurement_system,
        )?;
        if available < requested {
            if ingredient.optional {
                optional_missing += 1;
            } else {
                required_missing += 1;
            }
            missing.push(missing_from_ingredient(ingredient, "insufficient stock"));
        } else {
            pantry_items.insert(product.id);
            if inventory
                .batches
                .iter()
                .any(|batch| batch.expiry_urgency >= PantryExpiryUrgency::Soon)
            {
                expiring_match_count += 1;
            }
        }
    }

    let cookable = required_missing == 0 && unresolved == 0;
    let mut score = 100 - (required_missing * 25) - (optional_missing * 5) - (unresolved * 15)
        + (expiring_match_count * 8);
    score = score.clamp(0, 150);
    let score_breakdown = PantrySuggestionScoreDto {
        cookable,
        required_missing_count: required_missing,
        optional_missing_count: optional_missing,
        unresolved_count: unresolved,
        expiring_match_count,
        notes,
    };
    let mut pantry_items = pantry_items.into_iter().collect::<Vec<_>>();
    pantry_items.sort();
    Ok(ScoredSuggestion {
        source: PantrySuggestionSource::SavedRecipe,
        recipe_id: Some(recipe.recipe.id),
        recipe_version_id: Some(recipe.version.id),
        title: recipe.recipe.name.clone(),
        summary: recipe.recipe.description.clone(),
        score,
        score_breakdown,
        missing,
        pantry_items,
        generated_recipe: None,
    })
}

async fn resolve_recipe_product(
    state: &AppState,
    household_id: Uuid,
    ingredient: &RecipeIngredientRow,
) -> ApiResult<Option<(ProductRow, Option<String>)>> {
    if let Some(product_id) = ingredient.product_id {
        let product = qm_db::products::find_for_household(&state.db, household_id, product_id)
            .await?
            .ok_or(ApiError::NotFound)?;
        return Ok(Some((product, None)));
    }
    let Some(ingredient_id) = ingredient.ingredient_id else {
        return Ok(None);
    };
    let Some(mapping) = qm_db::ingredients::list_mappings(&state.db, household_id, ingredient_id)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let product = qm_db::products::find_for_household(&state.db, household_id, mapping.product_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Some((
        product,
        mapping.conversion_provenance.map(|provenance| {
            format!(
                "{} uses {provenance} conversion metadata",
                ingredient.display_name
            )
        }),
    )))
}

struct GeneratedSuggestions {
    task_id: Uuid,
    suggestions: Vec<ScoredSuggestion>,
    validation_errors: Vec<String>,
}

async fn generate_recipe_ideas(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    ctx: &PantryContext,
    constraints: &SanitizedSuggestionConstraints,
) -> Result<GeneratedSuggestions, String> {
    let status = state.ai_provider.status();
    if !status.enabled || !status.configured {
        return Err("AI recipe generation is not enabled or configured".into());
    }
    let input_summary = ai_pantry_input_summary(ctx, constraints);
    let input_summary_json =
        serde_json::to_string(&input_summary).map_err(|err| err.to_string())?;
    let input_digest = format!(
        "sha256:{}",
        Sha256::digest(input_summary_json.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let schema = pantry_recipe_ideas_schema(constraints.max_ai_suggestions);
    let schema_bytes = serde_json::to_vec(&schema)
        .map(|bytes| bytes.len())
        .unwrap_or_default();
    tracing::info!(
        provider = %status.provider,
        model = status.model.as_deref().unwrap_or("unknown"),
        inventory_item_count = ctx.dto.inventory.len(),
        inventory_batch_count = ctx
            .dto
            .inventory
            .iter()
            .map(|item| item.batches.len())
            .sum::<usize>(),
        input_summary_bytes = input_summary_json.len(),
        schema_bytes,
        max_ai_suggestions = constraints.max_ai_suggestions,
        "requesting AI pantry recipe ideas"
    );
    let provider_started = Instant::now();
    let max_output_tokens = state.config.ai_pantry_suggestion_max_output_tokens;
    let response = match state
        .ai_provider
        .complete_structured(qm_ai::StructuredOutputRequest {
            task_type: "recipe_generation".into(),
            prompt_version: PROMPT_VERSION.into(),
            model: None,
            max_output_tokens: Some(max_output_tokens),
            system_prompt: "You suggest practical, concise recipes from pantry inventory. Return strict JSON only. Use positive decimal strings for serving_count, never ranges. Keep the JSON compact: short names, short steps, and no prose beyond schema fields. Prefer short candidate recipes with no more than 8 ingredients and 6 steps. Mark unresolved conversions and substitutions explicitly. Never claim the recipe is executable; Quartermaster will validate it.".into(),
            user_prompt: input_summary_json.clone(),
            json_schema_name: "pantry_recipe_ideas".into(),
            json_schema: schema,
        })
        .await
    {
        Ok(response) => {
            tracing::info!(
                provider = %response.provider,
                model = %response.model,
                max_output_tokens,
                elapsed_ms = provider_started.elapsed().as_millis() as u64,
                "received AI pantry recipe ideas"
            );
            response
        }
        Err(err) => {
            tracing::warn!(
                provider = %status.provider,
                model = status.model.as_deref().unwrap_or("unknown"),
                max_output_tokens,
                elapsed_ms = provider_started.elapsed().as_millis() as u64,
                error = %err,
                "AI pantry recipe idea request failed"
            );
            return Err(err.to_string());
        }
    };
    let ideas: Vec<GeneratedRecipeIdeaDto> = serde_json::from_value(
        response
            .output_json
            .get("ideas")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    )
    .map_err(|err| err.to_string())?;
    let generated_idea_count = ideas.len();
    let validation_errors = validate_generated_ideas(&ideas);
    let valid_ideas = ideas
        .into_iter()
        .enumerate()
        .filter_map(|(idx, idea)| {
            validate_generated_idea(idx, &idea)
                .is_empty()
                .then_some(idea)
        })
        .collect::<Vec<_>>();
    tracing::info!(
        generated_idea_count,
        valid_idea_count = valid_ideas.len(),
        validation_error_count = validation_errors.len(),
        "validated AI pantry recipe ideas"
    );
    let output_json =
        serde_json::to_string(&response.output_json).map_err(|err| err.to_string())?;
    let raw_response_json = response
        .raw_response_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| err.to_string())?;
    let validation_errors_json =
        serde_json::to_string(&validation_errors).map_err(|err| err.to_string())?;
    let task = qm_db::ai_tasks::create(
        &state.db,
        household_id,
        &qm_db::ai_tasks::NewAiTask {
            created_by: Some(actor),
            task_type: "recipe_generation",
            provider: response.provider.as_str(),
            model: Some(&response.model),
            prompt_version: PROMPT_VERSION,
            input_digest: &input_digest,
            input_summary_json: &input_summary_json,
            output_json: Some(&output_json),
            validation_status: if valid_ideas.is_empty() && !validation_errors.is_empty() {
                "rejected"
            } else {
                "valid"
            },
            validation_errors_json: &validation_errors_json,
            user_state: AiTaskUserState::Proposed.as_str(),
            credentials_assertion: true,
            raw_response_json: raw_response_json.as_deref(),
        },
    )
    .await
    .map_err(|err| err.to_string())?;
    if valid_ideas.is_empty() {
        return Ok(GeneratedSuggestions {
            task_id: task.id,
            suggestions: Vec::new(),
            validation_errors,
        });
    }
    let suggestions = valid_ideas
        .into_iter()
        .map(|idea| {
            let title = idea.name.clone();
            let summary = idea.explanation.clone().or(idea.description.clone());
            ScoredSuggestion {
                source: PantrySuggestionSource::AiRecipe,
                recipe_id: None,
                recipe_version_id: None,
                title,
                summary,
                score: 60,
                score_breakdown: PantrySuggestionScoreDto {
                    cookable: false,
                    required_missing_count: 0,
                    optional_missing_count: 0,
                    unresolved_count: idea.unresolved_conversions.len() as i64,
                    expiring_match_count: 0,
                    notes: vec![
                        "AI-generated candidate requires review before saving or cooking".into(),
                    ],
                },
                missing: Vec::new(),
                pantry_items: Vec::new(),
                generated_recipe: Some(idea),
            }
        })
        .collect();
    Ok(GeneratedSuggestions {
        task_id: task.id,
        suggestions,
        validation_errors,
    })
}

fn ai_pantry_input_summary<'a>(
    ctx: &'a PantryContext,
    constraints: &'a SanitizedSuggestionConstraints,
) -> Value {
    let inventory = ctx
        .dto
        .inventory
        .iter()
        .map(|item| {
            let expiring_batches = item
                .batches
                .iter()
                .filter(|batch| batch.expiry_urgency >= PantryExpiryUrgency::Soon)
                .take(MAX_PROMPT_EXPIRING_BATCHES)
                .map(|batch| AiPantryInventoryBatch {
                    quantity: &batch.quantity,
                    unit: &batch.unit,
                    expires_on: batch.expires_on.as_deref(),
                    expiry_urgency: batch.expiry_urgency,
                })
                .collect();
            AiPantryInventoryItem {
                name: &item.product.name,
                brand: item.product.brand.as_deref(),
                family: item.product.family.as_str(),
                quantity: &item.total_quantity,
                unit: &item.unit,
                expiry_urgency: item.expiry_urgency,
                expiring_batches,
            }
        })
        .collect();
    json!(AiPantryInputSummary {
        inventory,
        dietary_constraints: &constraints.dietary_constraints,
        equipment: &constraints.equipment,
        max_suggestions: constraints.max_ai_suggestions,
        policy: "Generate candidates only; Quartermaster validates before saving or executing. No credentials are included.",
        output_guidance: "Keep every idea compact: no more than 8 ingredients, 6 steps, 3 timers, and 5 equipment or note entries.",
    })
}

fn pantry_recipe_ideas_schema(max_suggestions: i64) -> Value {
    let quantity_range_schema = json!({
        "type": ["object", "null"],
        "additionalProperties": false,
        "required": ["min", "max"],
        "properties": {
            "min": {"type": "string", "maxLength": 32},
            "max": {"type": "string", "maxLength": 32}
        }
    });
    let quantity_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["amount", "unit", "family", "range", "to_taste", "preparation_note"],
        "properties": {
            "amount": {"type": ["string", "null"], "maxLength": 32},
            "unit": {"type": ["string", "null"], "maxLength": 32},
            "family": {"type": ["string", "null"], "enum": ["mass", "volume", "count", null]},
            "range": quantity_range_schema,
            "to_taste": {"type": "boolean"},
            "preparation_note": {"type": ["string", "null"], "maxLength": 96}
        }
    });
    let ingredient_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "id",
            "ingredient_id",
            "product_id",
            "display_name",
            "quantity",
            "preparation",
            "optional",
            "group_label",
            "substitution_hints"
        ],
        "properties": {
            "id": {"type": "null"},
            "ingredient_id": {"type": "null"},
            "product_id": {"type": "null"},
            "display_name": {"type": "string", "maxLength": 96},
            "quantity": quantity_schema,
            "preparation": {"type": ["string", "null"], "maxLength": 96},
            "optional": {"type": "boolean"},
            "group_label": {"type": ["string", "null"], "maxLength": 64},
            "substitution_hints": {
                "type": "array",
                "maxItems": MAX_AI_SUBSTITUTION_HINTS,
                "items": {"type": "string", "maxLength": 120}
            }
        }
    });
    let timer_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["label", "duration_seconds"],
        "properties": {
            "label": {"type": ["string", "null"], "maxLength": 96},
            "duration_seconds": {"type": "integer"}
        }
    });
    let step_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "instruction", "timers", "equipment", "ingredient_refs"],
        "properties": {
            "id": {"type": "null"},
            "instruction": {"type": "string", "maxLength": 360},
            "timers": {"type": "array", "maxItems": MAX_AI_TIMERS, "items": timer_schema},
            "equipment": {
                "type": "array",
                "maxItems": MAX_AI_EQUIPMENT,
                "items": {"type": "string", "maxLength": 80}
            },
            "ingredient_refs": {
                "type": "array",
                "maxItems": MAX_AI_INGREDIENT_REFS,
                "items": {"type": "string", "maxLength": 96}
            }
        }
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ideas"],
        "properties": {
            "ideas": {
                "type": "array",
                "minItems": 1,
                "maxItems": max_suggestions,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "name",
                        "description",
                        "serving_count",
                        "ingredients",
                        "steps",
                        "explanation",
                        "unresolved_conversions",
                        "substitutions"
                    ],
                    "properties": {
                        "name": {"type": "string", "maxLength": 96},
                        "description": {"type": ["string", "null"], "maxLength": 240},
                        "serving_count": {"type": "string", "maxLength": 32},
                        "ingredients": {
                            "type": "array",
                            "maxItems": MAX_AI_INGREDIENTS,
                            "items": ingredient_schema
                        },
                        "steps": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": MAX_AI_STEPS,
                            "items": step_schema
                        },
                        "explanation": {"type": "string", "maxLength": 280},
                        "unresolved_conversions": {
                            "type": "array",
                            "maxItems": MAX_AI_LIST_ITEMS,
                            "items": {"type": "string", "maxLength": 160}
                        },
                        "substitutions": {
                            "type": "array",
                            "maxItems": MAX_AI_LIST_ITEMS,
                            "items": {"type": "string", "maxLength": 160}
                        }
                    }
                }
            }
        }
    })
}

fn validate_generated_ideas(ideas: &[GeneratedRecipeIdeaDto]) -> Vec<String> {
    let mut errors = Vec::new();
    if ideas.is_empty() {
        errors.push("ideas must include at least one recipe candidate".into());
    }
    for (idx, idea) in ideas.iter().enumerate() {
        errors.extend(validate_generated_idea(idx, idea));
    }
    errors
}

fn validate_generated_idea(idx: usize, idea: &GeneratedRecipeIdeaDto) -> Vec<String> {
    let mut errors = Vec::new();
    if idea.name.trim().is_empty() {
        errors.push(format!("ideas[{idx}].name is required"));
    }
    if Decimal::from_str(idea.serving_count.trim()).map_or(true, |value| value <= Decimal::ZERO) {
        errors.push(format!(
            "ideas[{idx}].serving_count must be a positive decimal"
        ));
    }
    if idea.steps.is_empty() {
        errors.push(format!("ideas[{idx}].steps must not be empty"));
    }
    errors
}

async fn insert_suggestion(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    suggestion: &ScoredSuggestion,
    ai_task_id: Option<Uuid>,
) -> ApiResult<PantrySuggestionRow> {
    let score_breakdown_json = serde_json::to_string(&suggestion.score_breakdown)
        .map_err(|err| ApiError::Internal(err.into()))?;
    let missing_json =
        serde_json::to_string(&suggestion.missing).map_err(|err| ApiError::Internal(err.into()))?;
    let pantry_items_json = serde_json::to_string(&suggestion.pantry_items)
        .map_err(|err| ApiError::Internal(err.into()))?;
    let generated_recipe_json = suggestion
        .generated_recipe
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| ApiError::Internal(err.into()))?;
    qm_db::pantry_suggestions::create(
        &state.db,
        household_id,
        &NewPantrySuggestion {
            created_by: Some(actor),
            source: suggestion.source.as_str(),
            status: PantrySuggestionStatus::Suggested.as_str(),
            recipe_id: suggestion.recipe_id,
            recipe_version_id: suggestion.recipe_version_id,
            ai_task_id,
            title: &suggestion.title,
            summary: suggestion.summary.as_deref(),
            score: suggestion.score,
            score_breakdown_json: &score_breakdown_json,
            missing_json: &missing_json,
            pantry_items_json: &pantry_items_json,
            generated_recipe_json: generated_recipe_json.as_deref(),
        },
    )
    .await
    .map_err(ApiError::from)
}

fn suggestion_into_dto(row: PantrySuggestionRow) -> ApiResult<PantrySuggestionDto> {
    Ok(PantrySuggestionDto {
        id: row.id,
        source: PantrySuggestionSource::from_str(&row.source)?,
        status: PantrySuggestionStatus::from_str(&row.status)?,
        recipe_id: row.recipe_id,
        recipe_version_id: row.recipe_version_id,
        ai_task_id: row.ai_task_id,
        title: row.title,
        summary: row.summary,
        score: row.score,
        score_breakdown: serde_json::from_str(&row.score_breakdown_json)
            .map_err(|err| ApiError::Internal(err.into()))?,
        missing: serde_json::from_str(&row.missing_json)
            .map_err(|err| ApiError::Internal(err.into()))?,
        pantry_items: serde_json::from_str(&row.pantry_items_json)
            .map_err(|err| ApiError::Internal(err.into()))?,
        generated_recipe: row
            .generated_recipe_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|err| ApiError::Internal(err.into()))?,
        created_by: row.created_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn missing_from_ingredient(
    ingredient: &RecipeIngredientRow,
    reason: &str,
) -> PantrySuggestionMissingDto {
    PantrySuggestionMissingDto {
        display_name: ingredient.display_name.clone(),
        quantity: ingredient.amount.clone(),
        unit: ingredient.unit.clone(),
        optional: ingredient.optional,
        reason: reason.into(),
    }
}

fn household_today(timezone: &str) -> ApiResult<Date> {
    let time_zone = tz::db()
        .get(timezone)
        .map_err(|_| ApiError::BadRequest("household timezone must be a valid IANA zone".into()))?;
    Ok(Timestamp::now().to_zoned(time_zone).date())
}

fn expiry_urgency(value: Option<&str>, today: Date) -> ApiResult<PantryExpiryUrgency> {
    let Some(value) = value else {
        return Ok(PantryExpiryUrgency::None);
    };
    let date = Date::from_str(value)
        .map_err(|_| ApiError::BadRequest(format!("date must be YYYY-MM-DD (got {value})")))?;
    if date < today {
        return Ok(PantryExpiryUrgency::Expired);
    }
    if date == today {
        return Ok(PantryExpiryUrgency::Today);
    }
    let days = date
        .since(today)
        .map_err(|err| ApiError::Internal(anyhow::Error::from(err)))?
        .get_days();
    if days <= 7 {
        Ok(PantryExpiryUrgency::Soon)
    } else {
        Ok(PantryExpiryUrgency::Future)
    }
}

fn convert_decimal(
    quantity: Decimal,
    from: &str,
    to: &str,
    measurement_system: MeasurementSystem,
) -> ApiResult<Decimal> {
    qm_core::units::convert_with_measurement_system(quantity, from, to, measurement_system)
        .map_err(ApiError::Domain)
}

fn normalize_decimal(value: Decimal) -> String {
    value.normalize().to_string()
}

fn sorted_ids(ids: &HashSet<Uuid>) -> Vec<Uuid> {
    let mut ids = ids.iter().copied().collect::<Vec<_>>();
    ids.sort();
    ids
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
        let value = value.trim();
        if value.is_empty() || value.len() > max_len {
            return Err(ApiError::BadRequest(format!(
                "{field} entries must be 1..={max_len} chars"
            )));
        }
        if !out.iter().any(|existing| existing == value) {
            out.push(value.to_owned());
        }
    }
    Ok(out)
}
