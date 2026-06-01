use std::{collections::HashMap, str::FromStr};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use jiff::civil::Date;
use qm_db::{
    meal_plans::{
        self, MealPlanFull, NewMealPlan, NewMealPlanDay, NewMealPlanMeal, NewStockReservation,
        StockReservationRow,
    },
    stock::{NewRecipeExecution, StockFilter},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    routes::products::ProductDto,
    routes::recipes::{
        build_preflight_with_reserved_quantities, consumption_from_plan,
        create_recipe_from_request, execution_request_from_recipe, CreateRecipeRequest,
        RecipeExecutionPreflightResponse, RecipeExecutionResponse, RecipeIngredientDto,
        RecipeProvenanceDto, RecipeStepDto,
    },
    routes::stock::StockBatchDto,
    types::{AiTaskUserState, RecipeProvenanceSource, RecipeSource, RecipeVisibility},
    AppState,
};

const AI_PROMPT_VERSION: &str = "meal-plan-recipe.v1";
const MAX_AI_INGREDIENTS: i64 = 8;
const MAX_AI_STEPS: i64 = 6;
const MAX_AI_TIMERS: i64 = 3;
const MAX_AI_EQUIPMENT: i64 = 5;
const MAX_AI_INGREDIENT_REFS: i64 = 8;
const MAX_AI_LIST_ITEMS: i64 = 5;
const MAX_AI_SUBSTITUTION_HINTS: i64 = 3;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/meal-plans", get(list).post(create))
        .route("/meal-plans/generate", post(generate))
        .route(
            "/meal-plans/{id}",
            get(get_one).put(update).delete(delete_one),
        )
        .route("/meal-plans/{id}/refresh", post(refresh))
        .route(
            "/meal-plans/{id}/meals/{meal_id}/execute",
            post(execute_meal),
        )
        .route("/meal-plans/{id}/meals/{meal_id}/skip", post(skip_meal))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MealSlotDto {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateMealPlanRequest {
    pub title: String,
    pub dates: Vec<String>,
    #[serde(default)]
    pub slots: Vec<MealSlotDto>,
    #[serde(default)]
    pub constraints: Value,
}

pub type UpdateMealPlanRequest = CreateMealPlanRequest;

#[derive(Debug, Deserialize, ToSchema)]
pub struct GenerateMealPlanRequest {
    pub title: Option<String>,
    pub dates: Vec<String>,
    #[serde(default)]
    pub slots: Vec<MealSlotDto>,
    #[serde(default)]
    pub constraints: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MealPlanListResponse {
    pub items: Vec<MealPlanSummaryDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MealPlanSummaryDto {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub dates: Vec<String>,
    pub meal_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MealPlanDto {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub constraints: Value,
    pub ai_task_id: Option<Uuid>,
    pub days: Vec<MealPlanDayDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MealPlanDayDto {
    pub id: Uuid,
    pub date: String,
    pub meals: Vec<MealPlanMealDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MealPlanMealDto {
    pub id: Uuid,
    pub date: String,
    pub slot_key: String,
    pub slot_label: String,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub recipe_name: Option<String>,
    pub serving_scale: String,
    pub status: String,
    pub preflight: Option<RecipeExecutionPreflightResponse>,
    pub warnings: Vec<String>,
    pub conflicts: Vec<String>,
    pub reservations: Vec<MealPlanReservationDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MealPlanReservationDto {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RefreshMealPlanResponse {
    pub plan: MealPlanDto,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GeneratedMealPlanRecipeResponse {
    recipe: GeneratedMealPlanRecipeDto,
}

#[derive(Debug, Deserialize, Serialize)]
struct GeneratedMealPlanRecipeDto {
    name: String,
    description: Option<String>,
    serving_count: String,
    #[serde(default)]
    ingredients: Vec<RecipeIngredientDto>,
    #[serde(default)]
    steps: Vec<RecipeStepDto>,
    #[serde(default)]
    explanation: Option<String>,
    #[serde(default)]
    unresolved_conversions: Vec<String>,
    #[serde(default)]
    substitutions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AiMealPlanInputSummary<'a> {
    plan_date: &'a str,
    slot_key: &'a str,
    slot_label: &'a str,
    constraints: &'a Value,
    inventory: Vec<AiMealPlanInventoryItem>,
    policy: &'static str,
    output_guidance: &'static str,
}

#[derive(Debug, Serialize)]
struct AiMealPlanInventoryItem {
    product_id: Uuid,
    name: String,
    brand: Option<String>,
    family: String,
    available_quantity: String,
    unit: String,
}

#[utoipa::path(
    get,
    path = "/meal-plans",
    operation_id = "meal_plan_list",
    tag = "meal-plans",
    responses((status = 200, body = MealPlanListResponse)),
    security(("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<MealPlanListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let rows = meal_plans::list(&state.db, household_id).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let full = meal_plans::find(&state.db, household_id, row.id)
            .await?
            .ok_or(ApiError::NotFound)?;
        items.push(summary_dto(&full));
    }
    Ok(Json(MealPlanListResponse { items }))
}

#[utoipa::path(
    post,
    path = "/meal-plans",
    operation_id = "meal_plan_create",
    tag = "meal-plans",
    request_body = CreateMealPlanRequest,
    responses((status = 201, body = MealPlanDto)),
    security(("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateMealPlanRequest>,
) -> ApiResult<(StatusCode, Json<MealPlanDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let sanitized = SanitizedPlanInput::from_create(req)?;
    let new = sanitized.as_new(meal_plans::PLAN_STATUS_DRAFT);
    let row = meal_plans::create(&state.db, household_id, current.user_id, &new).await?;
    Ok((StatusCode::CREATED, Json(plan_dto(row)?)))
}

#[utoipa::path(
    post,
    path = "/meal-plans/generate",
    operation_id = "meal_plan_generate",
    tag = "meal-plans",
    request_body = GenerateMealPlanRequest,
    responses((status = 201, body = MealPlanDto)),
    security(("bearer" = [])),
)]
pub async fn generate(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<GenerateMealPlanRequest>,
) -> ApiResult<(StatusCode, Json<MealPlanDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let sanitized = SanitizedPlanInput::from_generate(req)?;
    let new = sanitized.as_new(meal_plans::PLAN_STATUS_ACTIVE);
    let row = meal_plans::create(&state.db, household_id, current.user_id, &new).await?;
    let row = populate_plan(&state, household_id, current.user_id, row).await?;
    Ok((StatusCode::CREATED, Json(plan_dto(row)?)))
}

#[utoipa::path(
    get,
    path = "/meal-plans/{id}",
    operation_id = "meal_plan_get",
    tag = "meal-plans",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = MealPlanDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MealPlanDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = meal_plans::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(plan_dto(row)?))
}

#[utoipa::path(
    put,
    path = "/meal-plans/{id}",
    operation_id = "meal_plan_update",
    tag = "meal-plans",
    params(("id" = Uuid, Path)),
    request_body = UpdateMealPlanRequest,
    responses((status = 200, body = MealPlanDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateMealPlanRequest>,
) -> ApiResult<Json<MealPlanDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let sanitized = SanitizedPlanInput::from_create(req)?;
    let new = sanitized.as_new(meal_plans::PLAN_STATUS_DRAFT);
    let row = meal_plans::replace(&state.db, household_id, current.user_id, id, &new)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(plan_dto(row)?))
}

#[utoipa::path(
    delete,
    path = "/meal-plans/{id}",
    operation_id = "meal_plan_delete",
    tag = "meal-plans",
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
    if !meal_plans::delete(&state.db, household_id, id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/meal-plans/{id}/refresh",
    operation_id = "meal_plan_refresh",
    tag = "meal-plans",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = RefreshMealPlanResponse), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn refresh(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RefreshMealPlanResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let full = meal_plans::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let refreshed = refresh_existing_plan(&state, household_id, current.user_id, full).await?;
    Ok(Json(RefreshMealPlanResponse {
        plan: plan_dto(refreshed)?,
        warnings: Vec::new(),
    }))
}

#[utoipa::path(
    post,
    path = "/meal-plans/{id}/meals/{meal_id}/execute",
    operation_id = "meal_plan_meal_execute",
    tag = "meal-plans",
    params(("id" = Uuid, Path), ("meal_id" = Uuid, Path)),
    responses((status = 200, body = RecipeExecutionResponse), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn execute_meal(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((id, meal_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<RecipeExecutionResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let meal = meal_plans::find_meal(&state.db, household_id, id, meal_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if meal.status == meal_plans::MEAL_STATUS_COOKED {
        return Err(ApiError::BadRequest("meal has already been cooked".into()));
    }
    let preflight_json = meal
        .preflight_json
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("meal has no executable recipe plan".into()))?;
    let plan: RecipeExecutionPreflightResponse =
        serde_json::from_str(preflight_json).map_err(anyhow::Error::from)?;
    if !plan.can_execute {
        return Err(ApiError::BadRequest(
            "planned meal has unresolved required ingredients; refresh the plan first".into(),
        ));
    }

    let execution_id = Uuid::now_v7();
    let adjusted_recipe_json = serde_json::to_string(&json!({
        "meal_plan_id": id,
        "meal_plan_meal_id": meal_id,
        "recipe_id": meal.recipe_id,
        "recipe_version_id": meal.recipe_version_id,
        "recipe_name": meal.recipe_name,
        "serving_scale": meal.serving_scale,
    }))
    .map_err(anyhow::Error::from)?;
    let execution = NewRecipeExecution {
        id: execution_id,
        recipe_id: meal.recipe_id,
        recipe_version_id: meal.recipe_version_id,
        meal_plan_id: Some(id),
        meal_plan_meal_id: Some(meal_id),
        recipe_name: meal.recipe_name.as_deref(),
        serving_scale: &meal.serving_scale,
        idempotency_key: None,
        adjusted_recipe_json: &adjusted_recipe_json,
        preflight_json,
    };
    let consumption = consumption_from_plan(&plan)?;
    let application = qm_db::stock::apply_recipe_execution(
        &state.db,
        household_id,
        &execution,
        &consumption,
        &[],
        current.user_id,
        Some(&state.config.expiry_reminder_policy),
    )
    .await?;
    meal_plans::mark_reservations_consumed_for_meal(&state.db, household_id, meal_id).await?;
    meal_plans::set_meal_status(
        &state.db,
        household_id,
        meal_id,
        meal_plans::MEAL_STATUS_COOKED,
    )
    .await?;

    Ok(Json(RecipeExecutionResponse {
        execution_id,
        consume_request_id: application.consume_request_id,
        idempotent_replay: false,
        plan,
        output_batches: Vec::<StockBatchDto>::new(),
    }))
}

#[utoipa::path(
    post,
    path = "/meal-plans/{id}/meals/{meal_id}/skip",
    operation_id = "meal_plan_meal_skip",
    tag = "meal-plans",
    params(("id" = Uuid, Path), ("meal_id" = Uuid, Path)),
    responses((status = 200, body = MealPlanDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn skip_meal(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((id, meal_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<MealPlanDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    meal_plans::find_meal(&state.db, household_id, id, meal_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    meal_plans::release_reservations_for_meal(&state.db, household_id, meal_id).await?;
    meal_plans::set_meal_status(
        &state.db,
        household_id,
        meal_id,
        meal_plans::MEAL_STATUS_SKIPPED,
    )
    .await?;
    let row = meal_plans::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(plan_dto(row)?))
}

async fn populate_plan(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    full: MealPlanFull,
) -> ApiResult<MealPlanFull> {
    let recipes = load_recipe_candidates(state, household_id).await?;
    let constraints: Value =
        serde_json::from_str(&full.plan.constraints_json).map_err(anyhow::Error::from)?;
    let mut reserved = reservation_map(
        meal_plans::active_reservations_excluding_plan(&state.db, household_id, Some(full.plan.id))
            .await?,
    )?;

    for day in &full.days {
        for meal in &day.meals {
            let choice = choose_recipe_for_meal(state, household_id, &recipes, &reserved).await?;
            let (recipe, plan) = if let Some(executable) = choice.executable {
                executable
            } else if let Some(generated) = generate_ai_recipe_for_meal(
                state,
                household_id,
                actor,
                full.plan.id,
                &day.day.plan_date,
                &meal.meal.slot_key,
                &meal.meal.slot_label,
                &constraints,
                &reserved,
            )
            .await?
            {
                if let Some(ai_task_id) = generated.ai_task_id {
                    meal_plans::set_plan_ai_task_id(
                        &state.db,
                        household_id,
                        full.plan.id,
                        ai_task_id,
                    )
                    .await?;
                }
                (generated.recipe, generated.plan)
            } else if let Some(fallback) = choice.fallback {
                fallback
            } else {
                continue;
            };
            write_meal_plan(
                state,
                household_id,
                actor,
                full.plan.id,
                meal.meal.id,
                &day.day.plan_date,
                &recipe,
                &plan,
                &mut reserved,
            )
            .await?;
        }
    }
    meal_plans::find(&state.db, household_id, full.plan.id)
        .await?
        .ok_or(ApiError::NotFound)
}

async fn refresh_existing_plan(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    full: MealPlanFull,
) -> ApiResult<MealPlanFull> {
    let mut reserved = reservation_map(
        meal_plans::active_reservations_excluding_plan(&state.db, household_id, Some(full.plan.id))
            .await?,
    )?;
    for day in &full.days {
        for meal in &day.meals {
            meal_plans::release_reservations_for_meal(&state.db, household_id, meal.meal.id)
                .await?;
            let Some(recipe_id) = meal.meal.recipe_id else {
                continue;
            };
            let Some(recipe) = qm_db::recipes::find(&state.db, household_id, recipe_id).await?
            else {
                continue;
            };
            let mut request = execution_request_from_recipe(&recipe)?;
            request.serving_scale = Some(meal.meal.serving_scale.clone());
            let plan =
                build_preflight_with_reserved_quantities(state, household_id, &request, &reserved)
                    .await?;
            write_meal_plan(
                state,
                household_id,
                actor,
                full.plan.id,
                meal.meal.id,
                &day.day.plan_date,
                &recipe,
                &plan,
                &mut reserved,
            )
            .await?;
        }
    }
    meal_plans::find(&state.db, household_id, full.plan.id)
        .await?
        .ok_or(ApiError::NotFound)
}

async fn choose_recipe_for_meal(
    state: &AppState,
    household_id: Uuid,
    recipes: &[qm_db::recipes::RecipeFull],
    reserved: &HashMap<Uuid, Decimal>,
) -> ApiResult<SavedRecipeChoice> {
    let mut fallback = None;
    for recipe in recipes {
        let request = execution_request_from_recipe(recipe)?;
        if request.ingredients.is_empty() {
            continue;
        }
        let plan =
            build_preflight_with_reserved_quantities(state, household_id, &request, reserved)
                .await?;
        if plan.can_execute {
            return Ok(SavedRecipeChoice {
                executable: Some((recipe.clone(), plan)),
                fallback,
            });
        }
        if fallback.is_none() {
            fallback = Some((recipe.clone(), plan));
        }
    }
    Ok(SavedRecipeChoice {
        executable: None,
        fallback,
    })
}

struct SavedRecipeChoice {
    executable: Option<(qm_db::recipes::RecipeFull, RecipeExecutionPreflightResponse)>,
    fallback: Option<(qm_db::recipes::RecipeFull, RecipeExecutionPreflightResponse)>,
}

struct GeneratedRecipeChoice {
    recipe: qm_db::recipes::RecipeFull,
    plan: RecipeExecutionPreflightResponse,
    ai_task_id: Option<Uuid>,
}

#[allow(clippy::too_many_arguments)]
async fn generate_ai_recipe_for_meal(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    plan_id: Uuid,
    plan_date: &str,
    slot_key: &str,
    slot_label: &str,
    constraints: &Value,
    reserved: &HashMap<Uuid, Decimal>,
) -> ApiResult<Option<GeneratedRecipeChoice>> {
    let status = state.ai_provider.status();
    if !status.enabled || !status.configured || !status.structured_outputs {
        return Ok(None);
    }

    let inventory = build_ai_inventory(state, household_id, reserved).await?;
    let product_ids = inventory
        .iter()
        .map(|item| item.product_id)
        .collect::<Vec<_>>();
    let input_summary = AiMealPlanInputSummary {
        plan_date,
        slot_key,
        slot_label,
        constraints,
        inventory,
        policy: "Generate one practical recipe for this exact meal slot. Use only product_id values from the provided inventory when an ingredient should reserve stock. Quartermaster will validate, save, preflight, and reserve; do not claim execution is guaranteed.",
        output_guidance: "Keep the recipe compact: no more than 8 ingredients, 6 steps, 3 timers, and 5 note entries. Use positive decimal strings for serving_count and ingredient amounts.",
    };
    let input_summary_json = serde_json::to_string(&input_summary).map_err(anyhow::Error::from)?;
    let input_digest = format!(
        "sha256:{}",
        Sha256::digest(input_summary_json.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let schema = meal_plan_recipe_schema(&product_ids);
    let response = match state
        .ai_provider
        .complete_structured(qm_ai::StructuredOutputRequest {
            task_type: "recipe_generation".into(),
            prompt_version: AI_PROMPT_VERSION.into(),
            model: None,
            max_output_tokens: Some(state.config.ai_pantry_suggestion_max_output_tokens),
            system_prompt: "You generate durable meal-plan recipes from household pantry inventory. Return strict JSON only. Every stocked ingredient must use a product_id from the user's inventory list; use null only for pantry staples or unresolved items. Keep quantities in the product's listed unit/family when possible.".into(),
            user_prompt: input_summary_json.clone(),
            json_schema_name: "meal_plan_recipe".into(),
            json_schema: schema,
        })
        .await
    {
        Ok(response) => response,
        Err(err) => {
            tracing::warn!(
                provider = %status.provider,
                model = status.model.as_deref().unwrap_or("unknown"),
                error = %err,
                "AI meal-plan recipe request failed"
            );
            return Ok(None);
        }
    };

    let parsed =
        serde_json::from_value::<GeneratedMealPlanRecipeResponse>(response.output_json.clone());
    let generated = match parsed {
        Ok(parsed) => parsed.recipe,
        Err(err) => {
            let errors = vec![format!(
                "AI output did not match meal-plan recipe schema: {err}"
            )];
            let task_id = record_ai_generation_task(
                state,
                household_id,
                actor,
                &input_digest,
                &input_summary_json,
                &response,
                &errors,
            )
            .await?;
            meal_plans::set_plan_ai_task_id(&state.db, household_id, plan_id, task_id).await?;
            return Ok(None);
        }
    };

    let validation_errors = validate_generated_meal_plan_recipe(&generated);
    if !validation_errors.is_empty() {
        let task_id = record_ai_generation_task(
            state,
            household_id,
            actor,
            &input_digest,
            &input_summary_json,
            &response,
            &validation_errors,
        )
        .await?;
        meal_plans::set_plan_ai_task_id(&state.db, household_id, plan_id, task_id).await?;
        return Ok(None);
    }

    let recipe_request = generated_recipe_request(generated, &response.model)?;
    let recipe = match create_recipe_from_request(state, household_id, actor, recipe_request).await
    {
        Ok(recipe) => recipe,
        Err(err) => {
            let errors = vec![format!("generated recipe failed recipe validation: {err}")];
            let task_id = record_ai_generation_task(
                state,
                household_id,
                actor,
                &input_digest,
                &input_summary_json,
                &response,
                &errors,
            )
            .await?;
            meal_plans::set_plan_ai_task_id(&state.db, household_id, plan_id, task_id).await?;
            return Ok(None);
        }
    };
    let request = execution_request_from_recipe(&recipe)?;
    let plan =
        build_preflight_with_reserved_quantities(state, household_id, &request, reserved).await?;
    let task_id = record_ai_generation_task(
        state,
        household_id,
        actor,
        &input_digest,
        &input_summary_json,
        &response,
        &[],
    )
    .await?;
    Ok(Some(GeneratedRecipeChoice {
        recipe,
        plan,
        ai_task_id: Some(task_id),
    }))
}

async fn write_meal_plan(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    plan_id: Uuid,
    meal_id: Uuid,
    plan_date: &str,
    recipe: &qm_db::recipes::RecipeFull,
    plan: &RecipeExecutionPreflightResponse,
    reserved: &mut HashMap<Uuid, Decimal>,
) -> ApiResult<()> {
    let warnings = if plan.can_execute {
        plan.warnings.clone()
    } else {
        let mut warnings = plan.warnings.clone();
        warnings.push("Required ingredients are missing for this planned meal".into());
        warnings
    };
    let warnings_json = serde_json::to_string(&warnings).map_err(anyhow::Error::from)?;
    let conflicts_json = if plan.can_execute {
        "[]".to_owned()
    } else {
        serde_json::to_string(&plan.missing_ingredients).map_err(anyhow::Error::from)?
    };
    let preflight_json = serde_json::to_string(plan).map_err(anyhow::Error::from)?;
    let status = if plan.can_execute {
        meal_plans::MEAL_STATUS_PLANNED
    } else {
        meal_plans::MEAL_STATUS_CONFLICTED
    };
    meal_plans::update_meal_plan(
        &state.db,
        household_id,
        meal_id,
        Some(recipe.recipe.id),
        Some(recipe.version.id),
        Some(&recipe.recipe.name),
        Some(&preflight_json),
        &warnings_json,
        &conflicts_json,
        status,
    )
    .await?;
    meal_plans::release_reservations_for_meal(&state.db, household_id, meal_id).await?;

    let mut new_reservations = Vec::new();
    for ingredient in &plan.ingredients {
        for batch in &ingredient.matched_batches {
            new_reservations.push(NewStockReservation {
                meal_plan_id: plan_id,
                meal_plan_meal_id: meal_id,
                batch_id: batch.batch_id,
                product_id: ingredient.product.id,
                quantity: &batch.quantity,
                unit: &batch.unit,
                status: meal_plans::RESERVATION_ACTIVE,
            });
        }
    }
    let created =
        meal_plans::create_reservations(&state.db, household_id, &new_reservations).await?;
    for row in created {
        add_reserved_row(reserved, &row)?;
    }
    create_demand_signals(state, household_id, actor, plan_date, recipe, plan).await?;
    Ok(())
}

async fn create_demand_signals(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    plan_date: &str,
    recipe: &qm_db::recipes::RecipeFull,
    plan: &RecipeExecutionPreflightResponse,
) -> ApiResult<()> {
    for missing in &plan.missing_ingredients {
        if missing.optional {
            continue;
        }
        let Some(product_id) = missing.product_id else {
            continue;
        };
        let metadata = json!({
            "meal_plan_recipe": recipe.recipe.name,
            "reason": missing.reason,
        });
        let metadata_json = serde_json::to_string(&metadata).map_err(anyhow::Error::from)?;
        qm_db::replenishment::create_demand_signal(
            &state.db,
            household_id,
            actor,
            &qm_db::replenishment::NewDemandSignal {
                product_id,
                location_id: None,
                signal_type: qm_db::replenishment::DEMAND_SIGNAL_UPCOMING_RECIPE,
                quantity: &missing.missing_quantity,
                unit: &missing.requested_unit,
                recipe_id: Some(recipe.recipe.id),
                recipe_version_id: Some(recipe.version.id),
                desired_on: Some(plan_date),
                supplier_id: None,
                supplier_item_id: None,
                note: Some("Generated from meal plan missing ingredients"),
                metadata_json: &metadata_json,
            },
        )
        .await?;
    }
    Ok(())
}

async fn build_ai_inventory(
    state: &AppState,
    household_id: Uuid,
    reserved: &HashMap<Uuid, Decimal>,
) -> ApiResult<Vec<AiMealPlanInventoryItem>> {
    let household = qm_db::households::find_by_id(&state.db, household_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let measurement_system =
        crate::routes::households::measurement_system_from_db(&household.measurement_system)?;
    let stock = qm_db::stock::list(
        &state.db,
        household_id,
        &StockFilter {
            include_depleted: false,
            ..StockFilter::default()
        },
    )
    .await?;

    let mut by_product: HashMap<Uuid, (ProductDto, Decimal)> = HashMap::new();
    for item in stock {
        let batch_quantity =
            Decimal::from_str(&item.batch.quantity).map_err(anyhow::Error::from)?;
        let batch_reserved = reserved
            .get(&item.batch.id)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let available = batch_quantity - batch_reserved;
        if available <= Decimal::ZERO {
            continue;
        }
        let product = ProductDto::try_from(item.product)?;
        let normalized = convert_decimal(
            available,
            &item.batch.unit,
            &product.preferred_unit,
            measurement_system,
        )?;
        let entry = by_product
            .entry(product.id)
            .or_insert_with(|| (product, Decimal::ZERO));
        entry.1 += normalized;
    }

    let mut inventory = by_product
        .into_values()
        .map(|(product, quantity)| AiMealPlanInventoryItem {
            product_id: product.id,
            name: product.name,
            brand: product.brand,
            family: product.family.as_str().to_owned(),
            available_quantity: normalize_decimal(quantity),
            unit: product.preferred_unit,
        })
        .collect::<Vec<_>>();
    inventory.sort_by(|a, b| a.name.cmp(&b.name).then(a.product_id.cmp(&b.product_id)));
    Ok(inventory)
}

fn generated_recipe_request(
    generated: GeneratedMealPlanRecipeDto,
    model: &str,
) -> ApiResult<CreateRecipeRequest> {
    let raw_generated_json = serde_json::to_string(&generated).map_err(anyhow::Error::from)?;
    let mut user_edits = Vec::new();
    user_edits.extend(
        generated
            .unresolved_conversions
            .iter()
            .map(|item| format!("unresolved conversion: {item}")),
    );
    user_edits.extend(
        generated
            .substitutions
            .iter()
            .map(|item| format!("substitution: {item}")),
    );
    Ok(CreateRecipeRequest {
        name: generated.name.trim().to_owned(),
        description: generated.description,
        serving_count: generated.serving_count.trim().to_owned(),
        source: RecipeSource::LlmGenerated,
        visibility: RecipeVisibility::Household,
        tags: vec!["meal-plan".into(), "generated".into()],
        source_text: Some(raw_generated_json.clone()),
        ingredients: generated.ingredients,
        steps: generated.steps,
        outputs: Vec::new(),
        provenance: vec![RecipeProvenanceDto {
            id: None,
            source_type: RecipeProvenanceSource::Llm,
            imported_url: None,
            imported_file_name: None,
            imported_text: Some(raw_generated_json),
            prompt_version: Some(AI_PROMPT_VERSION.into()),
            model: Some(model.to_owned()),
            user_edits,
            parser_confidence: None,
        }],
    })
}

async fn record_ai_generation_task(
    state: &AppState,
    household_id: Uuid,
    actor: Uuid,
    input_digest: &str,
    input_summary_json: &str,
    response: &qm_ai::StructuredOutputResponse,
    validation_errors: &[String],
) -> ApiResult<Uuid> {
    let output_json = serde_json::to_string(&response.output_json).map_err(anyhow::Error::from)?;
    let raw_response_json = response
        .raw_response_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(anyhow::Error::from)?;
    let validation_errors_json =
        serde_json::to_string(validation_errors).map_err(anyhow::Error::from)?;
    let task = qm_db::ai_tasks::create(
        &state.db,
        household_id,
        &qm_db::ai_tasks::NewAiTask {
            created_by: Some(actor),
            task_type: "recipe_generation",
            provider: response.provider.as_str(),
            model: Some(&response.model),
            prompt_version: AI_PROMPT_VERSION,
            input_digest,
            input_summary_json,
            output_json: Some(&output_json),
            validation_status: if validation_errors.is_empty() {
                "valid"
            } else {
                "rejected"
            },
            validation_errors_json: &validation_errors_json,
            user_state: AiTaskUserState::Proposed.as_str(),
            credentials_assertion: true,
            raw_response_json: raw_response_json.as_deref(),
        },
    )
    .await?;
    Ok(task.id)
}

fn validate_generated_meal_plan_recipe(generated: &GeneratedMealPlanRecipeDto) -> Vec<String> {
    let mut errors = Vec::new();
    if generated.name.trim().is_empty() {
        errors.push("recipe.name is required".into());
    }
    if Decimal::from_str(generated.serving_count.trim())
        .map_or(true, |value| value <= Decimal::ZERO)
    {
        errors.push("recipe.serving_count must be a positive decimal".into());
    }
    if generated.steps.is_empty() {
        errors.push("recipe.steps must include at least one step".into());
    }
    if generated.ingredients.is_empty() {
        errors.push("recipe.ingredients must include at least one ingredient".into());
    }
    for (idx, ingredient) in generated.ingredients.iter().enumerate() {
        if ingredient.product_id.is_some()
            && (ingredient.quantity.amount.is_none() || ingredient.quantity.unit.is_none())
        {
            errors.push(format!(
                "recipe.ingredients[{idx}] uses a product_id but is missing amount or unit"
            ));
        }
    }
    errors
}

fn meal_plan_recipe_schema(product_ids: &[Uuid]) -> Value {
    let mut product_enum = product_ids
        .iter()
        .map(|id| Value::String(id.to_string()))
        .collect::<Vec<_>>();
    product_enum.push(Value::Null);
    let product_id_schema = json!({
        "type": ["string", "null"],
        "enum": product_enum,
    });
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
            "product_id": product_id_schema,
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
        "required": ["recipe"],
        "properties": {
            "recipe": {
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
                        "minItems": 1,
                        "maxItems": MAX_AI_INGREDIENTS,
                        "items": ingredient_schema
                    },
                    "steps": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": MAX_AI_STEPS,
                        "items": step_schema
                    },
                    "explanation": {"type": ["string", "null"], "maxLength": 280},
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
    })
}

async fn load_recipe_candidates(
    state: &AppState,
    household_id: Uuid,
) -> ApiResult<Vec<qm_db::recipes::RecipeFull>> {
    let summaries = qm_db::recipes::list(&state.db, household_id).await?;
    let mut recipes = Vec::with_capacity(summaries.len());
    for summary in summaries {
        if let Some(full) = qm_db::recipes::find(&state.db, household_id, summary.id).await? {
            recipes.push(full);
        }
    }
    Ok(recipes)
}

fn reservation_map(rows: Vec<StockReservationRow>) -> ApiResult<HashMap<Uuid, Decimal>> {
    let mut out = HashMap::new();
    for row in rows {
        add_reserved_row(&mut out, &row)?;
    }
    Ok(out)
}

fn add_reserved_row(out: &mut HashMap<Uuid, Decimal>, row: &StockReservationRow) -> ApiResult<()> {
    let quantity = Decimal::from_str(&row.quantity)
        .map_err(|_| ApiError::BadRequest("reservation quantity must be a decimal".into()))?;
    *out.entry(row.batch_id).or_insert(Decimal::ZERO) += quantity;
    Ok(())
}

fn convert_decimal(
    quantity: Decimal,
    from: &str,
    to: &str,
    measurement_system: qm_core::units::MeasurementSystem,
) -> ApiResult<Decimal> {
    qm_core::units::convert_with_measurement_system(quantity, from, to, measurement_system)
        .map_err(ApiError::Domain)
}

fn normalize_decimal(value: Decimal) -> String {
    value.normalize().to_string()
}

struct SanitizedPlanInput {
    title: String,
    dates: Vec<String>,
    slots: Vec<MealSlotDto>,
    constraints_json: String,
}

impl SanitizedPlanInput {
    fn from_create(req: CreateMealPlanRequest) -> ApiResult<Self> {
        Self::new(Some(req.title), req.dates, req.slots, req.constraints)
    }

    fn from_generate(req: GenerateMealPlanRequest) -> ApiResult<Self> {
        Self::new(req.title, req.dates, req.slots, req.constraints)
    }

    fn new(
        title: Option<String>,
        dates: Vec<String>,
        slots: Vec<MealSlotDto>,
        constraints: Value,
    ) -> ApiResult<Self> {
        let dates = sanitize_dates(dates)?;
        let title = title
            .map(|title| title.trim().to_owned())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| format!("Meal plan for {}", dates[0]));
        if title.len() > 256 {
            return Err(ApiError::BadRequest("title must be <= 256 chars".into()));
        }
        let slots = sanitize_slots(slots)?;
        let constraints_json = serde_json::to_string(&constraints).map_err(anyhow::Error::from)?;
        Ok(Self {
            title,
            dates,
            slots,
            constraints_json,
        })
    }

    fn as_new<'a>(&'a self, status: &'a str) -> NewMealPlan<'a> {
        NewMealPlan {
            title: &self.title,
            status,
            constraints_json: &self.constraints_json,
            days: self
                .dates
                .iter()
                .map(|date| NewMealPlanDay {
                    plan_date: date,
                    meals: self
                        .slots
                        .iter()
                        .map(|slot| NewMealPlanMeal {
                            slot_key: &slot.key,
                            slot_label: &slot.label,
                            recipe_id: None,
                            recipe_version_id: None,
                            recipe_name: None,
                            serving_scale: "1",
                            status: meal_plans::MEAL_STATUS_PLANNED,
                            preflight_json: None,
                            warnings_json: "[]",
                            conflicts_json: "[]",
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

fn sanitize_dates(dates: Vec<String>) -> ApiResult<Vec<String>> {
    if dates.is_empty() || dates.len() > 90 {
        return Err(ApiError::BadRequest(
            "dates must contain 1..=90 dates".into(),
        ));
    }
    let mut parsed = Vec::with_capacity(dates.len());
    for date in dates {
        let date = Date::from_str(date.trim())
            .map_err(|_| ApiError::BadRequest(format!("date must be YYYY-MM-DD (got {date})")))?;
        if !parsed.contains(&date) {
            parsed.push(date);
        }
    }
    parsed.sort();
    Ok(parsed.into_iter().map(|date| date.to_string()).collect())
}

fn sanitize_slots(slots: Vec<MealSlotDto>) -> ApiResult<Vec<MealSlotDto>> {
    let slots = if slots.is_empty() {
        vec![
            MealSlotDto {
                key: "breakfast".into(),
                label: "Breakfast".into(),
            },
            MealSlotDto {
                key: "lunch".into(),
                label: "Lunch".into(),
            },
            MealSlotDto {
                key: "dinner".into(),
                label: "Dinner".into(),
            },
        ]
    } else {
        slots
    };
    if slots.len() > 8 {
        return Err(ApiError::BadRequest(
            "slots must have at most 8 items".into(),
        ));
    }
    let mut out = Vec::with_capacity(slots.len());
    for slot in slots {
        let key = slot.key.trim().to_ascii_lowercase().replace(' ', "_");
        let label = slot.label.trim().to_owned();
        if key.is_empty() || key.len() > 48 || label.is_empty() || label.len() > 80 {
            return Err(ApiError::BadRequest(
                "slot key and label must be non-empty and reasonably short".into(),
            ));
        }
        if out.iter().any(|existing: &MealSlotDto| existing.key == key) {
            continue;
        }
        out.push(MealSlotDto { key, label });
    }
    Ok(out)
}

fn summary_dto(full: &MealPlanFull) -> MealPlanSummaryDto {
    MealPlanSummaryDto {
        id: full.plan.id,
        title: full.plan.title.clone(),
        status: full.plan.status.clone(),
        dates: full
            .days
            .iter()
            .map(|day| day.day.plan_date.clone())
            .collect(),
        meal_count: full
            .days
            .iter()
            .map(|day| day.meals.len() as i64)
            .sum::<i64>(),
        created_at: full.plan.created_at.clone(),
        updated_at: full.plan.updated_at.clone(),
    }
}

fn plan_dto(full: MealPlanFull) -> ApiResult<MealPlanDto> {
    Ok(MealPlanDto {
        id: full.plan.id,
        title: full.plan.title,
        status: full.plan.status,
        constraints: parse_json(&full.plan.constraints_json)?,
        ai_task_id: full.plan.ai_task_id,
        days: full
            .days
            .into_iter()
            .map(|day| {
                Ok(MealPlanDayDto {
                    id: day.day.id,
                    date: day.day.plan_date,
                    meals: day
                        .meals
                        .into_iter()
                        .map(meal_dto)
                        .collect::<ApiResult<Vec<_>>>()?,
                })
            })
            .collect::<ApiResult<Vec<_>>>()?,
        created_at: full.plan.created_at,
        updated_at: full.plan.updated_at,
    })
}

fn meal_dto(full: meal_plans::MealPlanMealFull) -> ApiResult<MealPlanMealDto> {
    Ok(MealPlanMealDto {
        id: full.meal.id,
        date: full.meal.plan_date,
        slot_key: full.meal.slot_key,
        slot_label: full.meal.slot_label,
        recipe_id: full.meal.recipe_id,
        recipe_version_id: full.meal.recipe_version_id,
        recipe_name: full.meal.recipe_name,
        serving_scale: full.meal.serving_scale,
        status: full.meal.status,
        preflight: full
            .meal
            .preflight_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(anyhow::Error::from)?,
        warnings: serde_json::from_str(&full.meal.warnings_json).map_err(anyhow::Error::from)?,
        conflicts: parse_conflicts(&full.meal.conflicts_json)?,
        reservations: full
            .reservations
            .into_iter()
            .map(|row| MealPlanReservationDto {
                id: row.id,
                batch_id: row.batch_id,
                product_id: row.product_id,
                quantity: row.quantity,
                unit: row.unit,
                status: row.status,
            })
            .collect(),
    })
}

fn parse_json(value: &str) -> ApiResult<Value> {
    serde_json::from_str(value).map_err(|err| ApiError::Internal(err.into()))
}

fn parse_conflicts(value: &str) -> ApiResult<Vec<String>> {
    let parsed: Value = parse_json(value)?;
    if let Some(items) = parsed.as_array() {
        Ok(items
            .iter()
            .map(|item| {
                item.get("display_name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| item.to_string())
            })
            .collect())
    } else {
        Ok(Vec::new())
    }
}
