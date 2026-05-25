use std::str::FromStr;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use jiff::civil::Date;
use qm_core::{
    batch::BatchConsumption,
    units::{MeasurementSystem, UnitFamily},
};
use qm_db::recipes::{
    NewRecipeIngredient, NewRecipeOutput, NewRecipeProvenance, NewRecipeStep, RecipeFull,
    RecipeIngredientRow, RecipeOutputRow, RecipeProvenanceRow, RecipeRow, RecipeStepRow,
};
use qm_db::{
    products::ProductRow,
    stock::{NewRecipeExecution, RecipeStockOutput},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    auth::{self, CurrentUser},
    error::{ApiError, ApiResult},
    routes::ingredients::{QuantityRangeDto, StructuredQuantityDto},
    routes::products::ProductDto,
    routes::stock::StockBatchDto,
    types::{RecipeProvenanceSource, RecipeSource, RecipeVisibility},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/recipes", get(list).post(create))
        .route("/recipes/imports/text", post(import_text))
        .route("/recipes/{id}", get(get_one).put(update).delete(delete_one))
        .route("/recipes/{id}/scale", post(scale))
        .route("/recipes/{id}/validate", get(validate))
        .route("/recipes/executions/preflight", post(preflight))
        .route("/recipes/executions", post(execute))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeListResponse {
    pub items: Vec<RecipeSummaryDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeSummaryDto {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub serving_count: String,
    pub source: RecipeSource,
    pub visibility: RecipeVisibility,
    pub tags: Vec<String>,
    pub latest_version_id: Uuid,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeDto {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub serving_count: String,
    pub source: RecipeSource,
    pub visibility: RecipeVisibility,
    pub tags: Vec<String>,
    pub version: RecipeVersionDto,
    pub validation: RecipeValidationResponse,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeVersionDto {
    pub id: Uuid,
    pub recipe_id: Uuid,
    pub version_number: i64,
    pub serving_count: String,
    pub source_text: Option<String>,
    pub ingredients: Vec<RecipeIngredientDto>,
    pub steps: Vec<RecipeStepDto>,
    pub outputs: Vec<RecipeOutputDto>,
    pub provenance: Vec<RecipeProvenanceDto>,
    pub created_by: Option<Uuid>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RecipeIngredientDto {
    pub id: Option<Uuid>,
    pub ingredient_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub display_name: String,
    pub quantity: StructuredQuantityDto,
    pub preparation: Option<String>,
    #[serde(default)]
    pub optional: bool,
    pub group_label: Option<String>,
    #[serde(default)]
    pub substitution_hints: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RecipeStepDto {
    pub id: Option<Uuid>,
    pub instruction: String,
    #[serde(default)]
    pub timers: Vec<RecipeTimerDto>,
    #[serde(default)]
    pub equipment: Vec<String>,
    #[serde(default)]
    pub ingredient_refs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RecipeTimerDto {
    pub label: Option<String>,
    pub duration_seconds: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RecipeOutputDto {
    pub id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub name: String,
    pub quantity: StructuredQuantityDto,
    pub expires_after_days: Option<i64>,
    pub storage_notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RecipeProvenanceDto {
    pub id: Option<Uuid>,
    pub source_type: RecipeProvenanceSource,
    pub imported_url: Option<String>,
    pub imported_file_name: Option<String>,
    pub imported_text: Option<String>,
    pub prompt_version: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub user_edits: Vec<String>,
    pub parser_confidence: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRecipeRequest {
    pub name: String,
    pub description: Option<String>,
    pub serving_count: String,
    #[serde(default = "default_recipe_source")]
    pub source: RecipeSource,
    #[serde(default = "default_recipe_visibility")]
    pub visibility: RecipeVisibility,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source_text: Option<String>,
    #[serde(default)]
    pub ingredients: Vec<RecipeIngredientDto>,
    #[serde(default)]
    pub steps: Vec<RecipeStepDto>,
    #[serde(default)]
    pub outputs: Vec<RecipeOutputDto>,
    #[serde(default)]
    pub provenance: Vec<RecipeProvenanceDto>,
}

pub type UpdateRecipeRequest = CreateRecipeRequest;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportTextRecipeRequest {
    pub name: Option<String>,
    pub text: String,
    pub serving_count: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ScaleRecipeRequest {
    pub serving_count: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeScaleResponse {
    pub recipe_id: Uuid,
    pub from_serving_count: String,
    pub to_serving_count: String,
    pub ingredients: Vec<ScaledRecipeIngredientDto>,
    pub outputs: Vec<ScaledRecipeOutputDto>,
    pub validation: RecipeValidationResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ScaledRecipeIngredientDto {
    pub ingredient_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub display_name: String,
    pub quantity: StructuredQuantityDto,
    pub scaled_quantity: StructuredQuantityDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ScaledRecipeOutputDto {
    pub product_id: Option<Uuid>,
    pub name: String,
    pub quantity: StructuredQuantityDto,
    pub scaled_quantity: StructuredQuantityDto,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RecipeValidationResponse {
    pub valid: bool,
    pub errors: Vec<RecipeValidationIssueDto>,
    pub warnings: Vec<RecipeValidationIssueDto>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RecipeValidationIssueDto {
    pub code: String,
    pub message: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeExecutionIngredientRequest {
    pub line_id: Option<String>,
    pub display_name: Option<String>,
    pub ingredient_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub location_id: Option<Uuid>,
    pub quantity: String,
    pub unit: String,
    #[serde(default)]
    pub optional: bool,
    pub substitution_of: Option<String>,
    pub preparation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeExecutionOutputRequest {
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub produced_on: Option<String>,
    pub expires_on: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeExecutionRequest {
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub recipe_name: Option<String>,
    pub serving_scale: Option<String>,
    #[serde(default)]
    pub ingredients: Vec<RecipeExecutionIngredientRequest>,
    #[serde(default)]
    pub outputs: Vec<RecipeExecutionOutputRequest>,
    pub use_expiring_first: Option<bool>,
    pub allow_partial: Option<bool>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeExecutionPreflightResponse {
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub recipe_name: Option<String>,
    pub serving_scale: String,
    pub use_expiring_first: bool,
    pub ingredients: Vec<RecipeIngredientPlanDto>,
    pub missing_ingredients: Vec<RecipeMissingIngredientDto>,
    pub outputs: Vec<RecipeOutputPreviewDto>,
    pub warnings: Vec<String>,
    pub can_execute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeIngredientPlanDto {
    pub line_id: Option<String>,
    pub display_name: Option<String>,
    pub ingredient_id: Option<Uuid>,
    pub mapping_id: Option<Uuid>,
    pub product: ProductDto,
    pub requested_quantity: String,
    pub requested_unit: String,
    pub inventory_quantity: String,
    pub inventory_unit: String,
    pub optional: bool,
    pub substitution_of: Option<String>,
    pub conversion_assumption: Option<String>,
    pub matched_batches: Vec<RecipeMatchedBatchDto>,
    pub missing_quantity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeMatchedBatchDto {
    pub batch_id: Uuid,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub quantity_in_requested_unit: String,
    pub requested_unit: String,
    pub expires_on: Option<String>,
    pub depleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeMissingIngredientDto {
    pub line_id: Option<String>,
    pub display_name: Option<String>,
    pub ingredient_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub requested_quantity: String,
    pub requested_unit: String,
    pub missing_quantity: String,
    pub optional: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeOutputPreviewDto {
    pub product: ProductDto,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub produced_on: Option<String>,
    pub expires_on: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecipeExecutionResponse {
    pub execution_id: Uuid,
    pub consume_request_id: Uuid,
    pub idempotent_replay: bool,
    pub plan: RecipeExecutionPreflightResponse,
    pub output_batches: Vec<StockBatchDto>,
}

#[utoipa::path(
    get,
    path = "/recipes",
    operation_id = "recipe_list",
    tag = "recipes",
    responses((status = 200, body = RecipeListResponse)),
    security(("bearer" = [])),
)]
pub async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<RecipeListResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let rows = qm_db::recipes::list(&state.db, household_id).await?;
    let items = rows
        .into_iter()
        .map(summary_into_dto)
        .collect::<ApiResult<_>>()?;
    Ok(Json(RecipeListResponse { items }))
}

#[utoipa::path(
    post,
    path = "/recipes",
    operation_id = "recipe_create",
    tag = "recipes",
    request_body = CreateRecipeRequest,
    responses((status = 201, body = RecipeDto)),
    security(("bearer" = [])),
)]
pub async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<CreateRecipeRequest>,
) -> ApiResult<(StatusCode, Json<RecipeDto>)> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let actor = current.user_id;
    let sanitized = validate_recipe_request(&state, household_id, req).await?;
    let new = sanitized.as_new();
    let row = qm_db::recipes::create(&state.db, household_id, actor, &new).await?;
    Ok((StatusCode::CREATED, Json(full_into_dto(row)?)))
}

#[utoipa::path(
    post,
    path = "/recipes/imports/text",
    operation_id = "recipe_import_text",
    tag = "recipes",
    request_body = ImportTextRecipeRequest,
    responses((status = 201, body = RecipeDto)),
    security(("bearer" = [])),
)]
pub async fn import_text(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<ImportTextRecipeRequest>,
) -> ApiResult<(StatusCode, Json<RecipeDto>)> {
    let text = required_text("text", req.text, 65_536)?;
    let name = match req.name {
        Some(name) => required_text("name", name, 256)?,
        None => text
            .lines()
            .next()
            .unwrap_or("Imported recipe")
            .trim()
            .to_owned(),
    };
    let create_req = CreateRecipeRequest {
        name,
        description: None,
        serving_count: req.serving_count.unwrap_or_else(|| "1".to_owned()),
        source: RecipeSource::PlainTextImport,
        visibility: RecipeVisibility::Household,
        tags: req.tags,
        source_text: Some(text.clone()),
        ingredients: Vec::new(),
        steps: vec![RecipeStepDto {
            id: None,
            instruction: text.clone(),
            timers: Vec::new(),
            equipment: Vec::new(),
            ingredient_refs: Vec::new(),
        }],
        outputs: Vec::new(),
        provenance: vec![RecipeProvenanceDto {
            id: None,
            source_type: RecipeProvenanceSource::PlainTextPaste,
            imported_url: None,
            imported_file_name: None,
            imported_text: Some(text),
            prompt_version: None,
            model: None,
            user_edits: Vec::new(),
            parser_confidence: None,
        }],
    };
    create(State(state), current, Json(create_req)).await
}

#[utoipa::path(
    get,
    path = "/recipes/{id}",
    operation_id = "recipe_get",
    tag = "recipes",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = RecipeDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn get_one(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RecipeDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::recipes::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(full_into_dto(row)?))
}

#[utoipa::path(
    put,
    path = "/recipes/{id}",
    operation_id = "recipe_update",
    tag = "recipes",
    params(("id" = Uuid, Path)),
    request_body = UpdateRecipeRequest,
    responses((status = 200, body = RecipeDto), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRecipeRequest>,
) -> ApiResult<Json<RecipeDto>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;
    let actor = current.user_id;
    let sanitized = validate_recipe_request(&state, household_id, req).await?;
    let upd = sanitized.as_new();
    let row = qm_db::recipes::update(&state.db, household_id, actor, id, &upd)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(full_into_dto(row)?))
}

#[utoipa::path(
    delete,
    path = "/recipes/{id}",
    operation_id = "recipe_delete",
    tag = "recipes",
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
    if !qm_db::recipes::delete(&state.db, household_id, id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/recipes/{id}/validate",
    operation_id = "recipe_validate",
    tag = "recipes",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = RecipeValidationResponse), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn validate(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RecipeValidationResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let row = qm_db::recipes::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(validation_for_full(&row)?))
}

#[utoipa::path(
    post,
    path = "/recipes/{id}/scale",
    operation_id = "recipe_scale",
    tag = "recipes",
    params(("id" = Uuid, Path)),
    request_body = ScaleRecipeRequest,
    responses((status = 200, body = RecipeScaleResponse), (status = 404, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn scale(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ScaleRecipeRequest>,
) -> ApiResult<Json<RecipeScaleResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let target = validate_positive_decimal("serving_count", req.serving_count)?;
    let target_decimal = Decimal::from_str(&target)
        .map_err(|_| ApiError::BadRequest("serving_count must be a decimal".into()))?;
    let row = qm_db::recipes::find(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let from_decimal = Decimal::from_str(&row.recipe.serving_count)
        .map_err(|_| ApiError::BadRequest("recipe serving_count must be a decimal".into()))?;
    let factor = target_decimal / from_decimal;
    let validation = validation_for_full(&row)?;
    Ok(Json(RecipeScaleResponse {
        recipe_id: row.recipe.id,
        from_serving_count: row.recipe.serving_count.clone(),
        to_serving_count: target,
        ingredients: row
            .ingredients
            .iter()
            .map(|ingredient| ScaledRecipeIngredientDto {
                ingredient_id: ingredient.ingredient_id,
                product_id: ingredient.product_id,
                display_name: ingredient.display_name.clone(),
                quantity: quantity_from_ingredient(ingredient)
                    .expect("stored recipe quantity should be valid"),
                scaled_quantity: scale_quantity(
                    quantity_from_ingredient(ingredient)
                        .expect("stored recipe quantity should be valid"),
                    factor,
                ),
            })
            .collect(),
        outputs: row
            .outputs
            .iter()
            .map(|output| ScaledRecipeOutputDto {
                product_id: output.product_id,
                name: output.name.clone(),
                quantity: quantity_from_output(output)
                    .expect("stored output quantity should be valid"),
                scaled_quantity: scale_quantity(
                    quantity_from_output(output).expect("stored output quantity should be valid"),
                    factor,
                ),
            })
            .collect(),
        validation,
    }))
}

#[utoipa::path(
    post,
    path = "/recipes/executions/preflight",
    operation_id = "recipe_execution_preflight",
    tag = "recipes",
    request_body = RecipeExecutionRequest,
    responses((status = 200, body = RecipeExecutionPreflightResponse), (status = 400, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn preflight(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<RecipeExecutionRequest>,
) -> ApiResult<Json<RecipeExecutionPreflightResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    let plan = build_preflight(&state, household_id, &req).await?;
    Ok(Json(plan))
}

#[utoipa::path(
    post,
    path = "/recipes/executions",
    operation_id = "recipe_execution_execute",
    tag = "recipes",
    request_body = RecipeExecutionRequest,
    responses((status = 200, body = RecipeExecutionResponse), (status = 400, body = crate::error::ApiErrorBody)),
    security(("bearer" = [])),
)]
pub async fn execute(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(req): Json<RecipeExecutionRequest>,
) -> ApiResult<Json<RecipeExecutionResponse>> {
    let household_id = current.household_id.ok_or(ApiError::Forbidden)?;
    auth::require_read_write(&current)?;

    if let Some(key) = req
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        if let Some(row) =
            qm_db::recipes::find_execution_by_idempotency_key(&state.db, household_id, key).await?
        {
            let plan: RecipeExecutionPreflightResponse =
                serde_json::from_str(&row.preflight_json).map_err(anyhow::Error::from)?;
            let output_batches = output_batch_dtos(&state, household_id, row.id).await?;
            return Ok(Json(RecipeExecutionResponse {
                execution_id: row.id,
                consume_request_id: row.consume_request_id,
                idempotent_replay: true,
                plan,
                output_batches,
            }));
        }
    }

    let plan = build_preflight(&state, household_id, &req).await?;
    let blocking_missing = plan
        .missing_ingredients
        .iter()
        .any(|missing| !missing.optional);
    if blocking_missing && !req.allow_partial.unwrap_or(false) {
        return Err(ApiError::BadRequest(
            "recipe execution has missing required ingredients; re-submit with allow_partial=true to confirm partial execution".into(),
        ));
    }

    let execution_id = Uuid::now_v7();
    let adjusted_recipe_json = serde_json::to_string(&req).map_err(anyhow::Error::from)?;
    let preflight_json = serde_json::to_string(&plan).map_err(anyhow::Error::from)?;
    let consumption = consumption_from_plan(&plan)?;
    let outputs = recipe_stock_outputs(&req.outputs);
    let serving_scale = plan.serving_scale.clone();
    let idempotency_key = req
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let execution = NewRecipeExecution {
        id: execution_id,
        recipe_id: req.recipe_id,
        recipe_version_id: req.recipe_version_id,
        recipe_name: req.recipe_name.as_deref(),
        serving_scale: &serving_scale,
        idempotency_key,
        adjusted_recipe_json: &adjusted_recipe_json,
        preflight_json: &preflight_json,
    };
    let application = qm_db::stock::apply_recipe_execution(
        &state.db,
        household_id,
        &execution,
        &consumption,
        &outputs,
        current.user_id,
        Some(&state.config.expiry_reminder_policy),
    )
    .await?;

    let mut output_batches = Vec::with_capacity(application.output_batch_ids.len());
    for id in application.output_batch_ids {
        output_batches.push(stock_dto(&state, household_id, id).await?);
    }

    Ok(Json(RecipeExecutionResponse {
        execution_id,
        consume_request_id: application.consume_request_id,
        idempotent_replay: false,
        plan,
        output_batches,
    }))
}

fn summary_into_dto(row: RecipeRow) -> ApiResult<RecipeSummaryDto> {
    Ok(RecipeSummaryDto {
        id: row.id,
        name: row.name,
        description: row.description,
        serving_count: row.serving_count,
        source: RecipeSource::from_str(&row.source)?,
        visibility: RecipeVisibility::from_str(&row.visibility)?,
        tags: json_string_vec(&row.tags_json, "recipe.tags_json")?,
        latest_version_id: row.latest_version_id.ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!("recipe {} has no latest version", row.id))
        })?,
        created_by: row.created_by,
        updated_by: row.updated_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn full_into_dto(row: RecipeFull) -> ApiResult<RecipeDto> {
    let validation = validation_for_full(&row)?;
    Ok(RecipeDto {
        id: row.recipe.id,
        name: row.recipe.name,
        description: row.recipe.description,
        serving_count: row.recipe.serving_count,
        source: RecipeSource::from_str(&row.recipe.source)?,
        visibility: RecipeVisibility::from_str(&row.recipe.visibility)?,
        tags: json_string_vec(&row.recipe.tags_json, "recipe.tags_json")?,
        version: RecipeVersionDto {
            id: row.version.id,
            recipe_id: row.version.recipe_id,
            version_number: row.version.version_number,
            serving_count: row.version.serving_count,
            source_text: row.version.source_text,
            ingredients: row
                .ingredients
                .into_iter()
                .map(ingredient_into_dto)
                .collect::<ApiResult<_>>()?,
            steps: row
                .steps
                .into_iter()
                .map(step_into_dto)
                .collect::<ApiResult<_>>()?,
            outputs: row
                .outputs
                .into_iter()
                .map(output_into_dto)
                .collect::<ApiResult<_>>()?,
            provenance: row
                .provenance
                .into_iter()
                .map(provenance_into_dto)
                .collect::<ApiResult<_>>()?,
            created_by: row.version.created_by,
            created_at: row.version.created_at,
        },
        validation,
        created_by: row.recipe.created_by,
        updated_by: row.recipe.updated_by,
        created_at: row.recipe.created_at,
        updated_at: row.recipe.updated_at,
    })
}

fn ingredient_into_dto(row: RecipeIngredientRow) -> ApiResult<RecipeIngredientDto> {
    let quantity = quantity_from_ingredient(&row)?;
    Ok(RecipeIngredientDto {
        id: Some(row.id),
        ingredient_id: row.ingredient_id,
        product_id: row.product_id,
        display_name: row.display_name,
        quantity,
        preparation: row.preparation,
        optional: row.optional,
        group_label: row.group_label,
        substitution_hints: json_string_vec(&row.substitution_hints_json, "substitution_hints")?,
    })
}

fn step_into_dto(row: RecipeStepRow) -> ApiResult<RecipeStepDto> {
    Ok(RecipeStepDto {
        id: Some(row.id),
        instruction: row.instruction,
        timers: serde_json::from_str(&row.timers_json).map_err(|err| {
            ApiError::Internal(anyhow::anyhow!("invalid recipe timers JSON: {err}"))
        })?,
        equipment: json_string_vec(&row.equipment_json, "equipment")?,
        ingredient_refs: json_string_vec(&row.ingredient_refs_json, "ingredient_refs")?,
    })
}

fn output_into_dto(row: RecipeOutputRow) -> ApiResult<RecipeOutputDto> {
    let quantity = quantity_from_output(&row)?;
    Ok(RecipeOutputDto {
        id: Some(row.id),
        product_id: row.product_id,
        name: row.name,
        quantity,
        expires_after_days: row.expires_after_days,
        storage_notes: row.storage_notes,
    })
}

fn provenance_into_dto(row: RecipeProvenanceRow) -> ApiResult<RecipeProvenanceDto> {
    Ok(RecipeProvenanceDto {
        id: Some(row.id),
        source_type: RecipeProvenanceSource::from_str(&row.source_type)?,
        imported_url: row.imported_url,
        imported_file_name: row.imported_file_name,
        imported_text: row.imported_text,
        prompt_version: row.prompt_version,
        model: row.model,
        user_edits: json_string_vec(&row.user_edits_json, "user_edits")?,
        parser_confidence: row.parser_confidence,
    })
}

struct SanitizedRecipe {
    name: String,
    description: Option<String>,
    serving_count: String,
    source: RecipeSource,
    visibility: RecipeVisibility,
    tags_json: String,
    source_text: Option<String>,
    payload_json: String,
    ingredients: Vec<SanitizedRecipeIngredient>,
    steps: Vec<SanitizedRecipeStep>,
    outputs: Vec<SanitizedRecipeOutput>,
    provenance: Vec<SanitizedRecipeProvenance>,
}

impl SanitizedRecipe {
    fn as_new(&self) -> qm_db::recipes::NewRecipe<'_> {
        qm_db::recipes::NewRecipe {
            name: &self.name,
            description: self.description.as_deref(),
            serving_count: &self.serving_count,
            source: self.source.as_str(),
            visibility: self.visibility.as_str(),
            tags_json: &self.tags_json,
            source_text: self.source_text.as_deref(),
            payload_json: &self.payload_json,
            ingredients: self
                .ingredients
                .iter()
                .map(SanitizedRecipeIngredient::as_new)
                .collect(),
            steps: self.steps.iter().map(SanitizedRecipeStep::as_new).collect(),
            outputs: self
                .outputs
                .iter()
                .map(SanitizedRecipeOutput::as_new)
                .collect(),
            provenance: self
                .provenance
                .iter()
                .map(SanitizedRecipeProvenance::as_new)
                .collect(),
        }
    }
}

struct SanitizedRecipeIngredient {
    ingredient_id: Option<Uuid>,
    product_id: Option<Uuid>,
    display_name: String,
    quantity: SanitizedQuantity,
    preparation: Option<String>,
    optional: bool,
    group_label: Option<String>,
    substitution_hints_json: String,
}

impl SanitizedRecipeIngredient {
    fn as_new(&self) -> NewRecipeIngredient<'_> {
        NewRecipeIngredient {
            ingredient_id: self.ingredient_id,
            product_id: self.product_id,
            display_name: &self.display_name,
            amount: self.quantity.amount.as_deref(),
            unit: self.quantity.unit.as_deref(),
            family: self.quantity.family.map(UnitFamily::as_str),
            range_min: self.quantity.range_min.as_deref(),
            range_max: self.quantity.range_max.as_deref(),
            to_taste: self.quantity.to_taste,
            preparation: self.preparation.as_deref(),
            optional: self.optional,
            group_label: self.group_label.as_deref(),
            substitution_hints_json: &self.substitution_hints_json,
        }
    }
}

struct SanitizedRecipeStep {
    instruction: String,
    timers_json: String,
    equipment_json: String,
    ingredient_refs_json: String,
}

impl SanitizedRecipeStep {
    fn as_new(&self) -> NewRecipeStep<'_> {
        NewRecipeStep {
            instruction: &self.instruction,
            timers_json: &self.timers_json,
            equipment_json: &self.equipment_json,
            ingredient_refs_json: &self.ingredient_refs_json,
        }
    }
}

struct SanitizedRecipeOutput {
    product_id: Option<Uuid>,
    name: String,
    quantity: SanitizedQuantity,
    expires_after_days: Option<i64>,
    storage_notes: Option<String>,
}

impl SanitizedRecipeOutput {
    fn as_new(&self) -> NewRecipeOutput<'_> {
        NewRecipeOutput {
            product_id: self.product_id,
            name: &self.name,
            amount: self.quantity.amount.as_deref(),
            unit: self.quantity.unit.as_deref(),
            family: self.quantity.family.map(UnitFamily::as_str),
            range_min: self.quantity.range_min.as_deref(),
            range_max: self.quantity.range_max.as_deref(),
            to_taste: self.quantity.to_taste,
            preparation_note: self.quantity.preparation_note.as_deref(),
            expires_after_days: self.expires_after_days,
            storage_notes: self.storage_notes.as_deref(),
        }
    }
}

struct SanitizedRecipeProvenance {
    source_type: RecipeProvenanceSource,
    imported_url: Option<String>,
    imported_file_name: Option<String>,
    imported_text: Option<String>,
    prompt_version: Option<String>,
    model: Option<String>,
    user_edits_json: String,
    parser_confidence: Option<String>,
}

impl SanitizedRecipeProvenance {
    fn as_new(&self) -> NewRecipeProvenance<'_> {
        NewRecipeProvenance {
            source_type: self.source_type.as_str(),
            imported_url: self.imported_url.as_deref(),
            imported_file_name: self.imported_file_name.as_deref(),
            imported_text: self.imported_text.as_deref(),
            prompt_version: self.prompt_version.as_deref(),
            model: self.model.as_deref(),
            user_edits_json: &self.user_edits_json,
            parser_confidence: self.parser_confidence.as_deref(),
        }
    }
}

#[derive(Debug, Clone)]
struct SanitizedQuantity {
    amount: Option<String>,
    unit: Option<String>,
    family: Option<UnitFamily>,
    range_min: Option<String>,
    range_max: Option<String>,
    to_taste: bool,
    preparation_note: Option<String>,
}

async fn validate_recipe_request(
    state: &AppState,
    household_id: Uuid,
    req: CreateRecipeRequest,
) -> ApiResult<SanitizedRecipe> {
    let name = required_text("name", req.name, 256)?;
    let serving_count = validate_positive_decimal("serving_count", req.serving_count)?;
    if req.steps.is_empty() {
        return Err(ApiError::BadRequest(
            "recipe must include at least one step".into(),
        ));
    }
    let mut ingredients = Vec::with_capacity(req.ingredients.len());
    for (idx, ingredient) in req.ingredients.into_iter().enumerate() {
        ingredients.push(validate_recipe_ingredient(state, household_id, ingredient, idx).await?);
    }
    let mut steps = Vec::with_capacity(req.steps.len());
    for (idx, step) in req.steps.into_iter().enumerate() {
        steps.push(validate_recipe_step(step, idx)?);
    }
    let mut outputs = Vec::with_capacity(req.outputs.len());
    for (idx, output) in req.outputs.into_iter().enumerate() {
        outputs.push(validate_recipe_output(state, household_id, output, idx).await?);
    }
    let provenance = req
        .provenance
        .into_iter()
        .enumerate()
        .map(|(idx, provenance)| validate_recipe_provenance(provenance, idx))
        .collect::<ApiResult<Vec<_>>>()?;
    let tags = validate_text_list("tags", req.tags, 64, 64)?;
    let tags_json = serde_json::to_string(&tags).map_err(|err| ApiError::Internal(err.into()))?;
    let payload_json = serde_json::to_string(&json!({
        "schema_version": 1,
        "ingredients": ingredients.iter().map(|item| ingredient_payload(item)).collect::<Vec<_>>(),
        "steps": steps.iter().map(|item| step_payload(item)).collect::<Vec<_>>(),
        "outputs": outputs.iter().map(|item| output_payload(item)).collect::<Vec<_>>(),
    }))
    .map_err(|err| ApiError::Internal(err.into()))?;
    Ok(SanitizedRecipe {
        name,
        description: optional_text("description", req.description, 2048)?,
        serving_count,
        source: req.source,
        visibility: req.visibility,
        tags_json,
        source_text: optional_text("source_text", req.source_text, 65_536)?,
        payload_json,
        ingredients,
        steps,
        outputs,
        provenance,
    })
}

async fn validate_recipe_ingredient(
    state: &AppState,
    household_id: Uuid,
    ingredient: RecipeIngredientDto,
    idx: usize,
) -> ApiResult<SanitizedRecipeIngredient> {
    if let Some(id) = ingredient.ingredient_id {
        qm_db::ingredients::find(&state.db, household_id, id)
            .await?
            .ok_or(ApiError::NotFound)?;
    }
    if let Some(id) = ingredient.product_id {
        qm_db::products::find_for_household(&state.db, household_id, id)
            .await?
            .ok_or(ApiError::NotFound)?;
    }
    Ok(SanitizedRecipeIngredient {
        ingredient_id: ingredient.ingredient_id,
        product_id: ingredient.product_id,
        display_name: required_text(
            &format!("ingredients[{idx}].display_name"),
            ingredient.display_name,
            256,
        )?,
        quantity: validate_quantity(
            &format!("ingredients[{idx}].quantity"),
            ingredient.quantity,
            false,
        )?,
        preparation: optional_text(
            &format!("ingredients[{idx}].preparation"),
            ingredient.preparation,
            256,
        )?,
        optional: ingredient.optional,
        group_label: optional_text(
            &format!("ingredients[{idx}].group_label"),
            ingredient.group_label,
            128,
        )?,
        substitution_hints_json: serde_json::to_string(&validate_text_list(
            &format!("ingredients[{idx}].substitution_hints"),
            ingredient.substitution_hints,
            32,
            256,
        )?)
        .map_err(|err| ApiError::Internal(err.into()))?,
    })
}

fn validate_recipe_step(step: RecipeStepDto, idx: usize) -> ApiResult<SanitizedRecipeStep> {
    for timer in &step.timers {
        if timer.duration_seconds <= 0 || timer.duration_seconds > 172_800 {
            return Err(ApiError::BadRequest(format!(
                "steps[{idx}].timers duration_seconds must be 1..=172800"
            )));
        }
    }
    Ok(SanitizedRecipeStep {
        instruction: required_text(&format!("steps[{idx}].instruction"), step.instruction, 8192)?,
        timers_json: serde_json::to_string(&step.timers)
            .map_err(|err| ApiError::Internal(err.into()))?,
        equipment_json: serde_json::to_string(&validate_text_list(
            &format!("steps[{idx}].equipment"),
            step.equipment,
            64,
            128,
        )?)
        .map_err(|err| ApiError::Internal(err.into()))?,
        ingredient_refs_json: serde_json::to_string(&validate_text_list(
            &format!("steps[{idx}].ingredient_refs"),
            step.ingredient_refs,
            128,
            256,
        )?)
        .map_err(|err| ApiError::Internal(err.into()))?,
    })
}

async fn validate_recipe_output(
    state: &AppState,
    household_id: Uuid,
    output: RecipeOutputDto,
    idx: usize,
) -> ApiResult<SanitizedRecipeOutput> {
    if let Some(id) = output.product_id {
        qm_db::products::find_for_household(&state.db, household_id, id)
            .await?
            .ok_or(ApiError::NotFound)?;
    }
    let expires_after_days = match output.expires_after_days {
        Some(days) if !(0..=3650).contains(&days) => {
            return Err(ApiError::BadRequest(format!(
                "outputs[{idx}].expires_after_days must be 0..=3650"
            )));
        }
        other => other,
    };
    Ok(SanitizedRecipeOutput {
        product_id: output.product_id,
        name: required_text(&format!("outputs[{idx}].name"), output.name, 256)?,
        quantity: validate_quantity(&format!("outputs[{idx}].quantity"), output.quantity, false)?,
        expires_after_days,
        storage_notes: optional_text(
            &format!("outputs[{idx}].storage_notes"),
            output.storage_notes,
            512,
        )?,
    })
}

fn validate_recipe_provenance(
    provenance: RecipeProvenanceDto,
    idx: usize,
) -> ApiResult<SanitizedRecipeProvenance> {
    let parser_confidence = match provenance.parser_confidence {
        Some(value) => {
            let value =
                validate_positive_decimal(&format!("provenance[{idx}].parser_confidence"), value)?;
            let parsed = Decimal::from_str(&value)
                .map_err(|_| ApiError::BadRequest("parser_confidence must be a decimal".into()))?;
            if parsed > Decimal::ONE {
                return Err(ApiError::BadRequest(
                    "parser_confidence must be <= 1".into(),
                ));
            }
            Some(value)
        }
        None => None,
    };
    Ok(SanitizedRecipeProvenance {
        source_type: provenance.source_type,
        imported_url: optional_text(
            &format!("provenance[{idx}].imported_url"),
            provenance.imported_url,
            2048,
        )?,
        imported_file_name: optional_text(
            &format!("provenance[{idx}].imported_file_name"),
            provenance.imported_file_name,
            256,
        )?,
        imported_text: optional_text(
            &format!("provenance[{idx}].imported_text"),
            provenance.imported_text,
            65_536,
        )?,
        prompt_version: optional_text(
            &format!("provenance[{idx}].prompt_version"),
            provenance.prompt_version,
            128,
        )?,
        model: optional_text(&format!("provenance[{idx}].model"), provenance.model, 128)?,
        user_edits_json: serde_json::to_string(&validate_text_list(
            &format!("provenance[{idx}].user_edits"),
            provenance.user_edits,
            128,
            512,
        )?)
        .map_err(|err| ApiError::Internal(err.into()))?,
        parser_confidence,
    })
}

fn validation_for_full(row: &RecipeFull) -> ApiResult<RecipeValidationResponse> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for (idx, ingredient) in row.ingredients.iter().enumerate() {
        if ingredient.ingredient_id.is_none() && ingredient.product_id.is_none() {
            warnings.push(issue(
                "unresolved_ingredient",
                format!(
                    "{} is not linked to an ingredient or product",
                    ingredient.display_name
                ),
                format!("ingredients[{idx}]"),
            ));
        }
        if !ingredient.to_taste && ingredient.amount.is_none() {
            errors.push(issue(
                "missing_quantity",
                format!(
                    "{} needs an amount or to_taste=true",
                    ingredient.display_name
                ),
                format!("ingredients[{idx}].quantity.amount"),
            ));
        }
        if ingredient.amount.is_some() && ingredient.unit.is_none() {
            errors.push(issue(
                "missing_unit",
                format!("{} has an amount but no unit", ingredient.display_name),
                format!("ingredients[{idx}].quantity.unit"),
            ));
        }
    }
    if row.steps.is_empty() {
        errors.push(issue(
            "missing_steps",
            "Recipe needs at least one step",
            "steps",
        ));
    }
    Ok(RecipeValidationResponse {
        valid: errors.is_empty(),
        errors,
        warnings,
    })
}

fn issue(
    code: &str,
    message: impl Into<String>,
    path: impl Into<String>,
) -> RecipeValidationIssueDto {
    RecipeValidationIssueDto {
        code: code.to_owned(),
        message: message.into(),
        path: path.into(),
    }
}

fn validate_quantity(
    field: &str,
    value: StructuredQuantityDto,
    require_amount_unit: bool,
) -> ApiResult<SanitizedQuantity> {
    let amount = value
        .amount
        .map(|amount| validate_positive_decimal(&format!("{field}.amount"), amount))
        .transpose()?;
    let unit = optional_text(&format!("{field}.unit"), value.unit, 64)?;
    if require_amount_unit && (amount.is_none() || unit.is_none()) {
        return Err(ApiError::BadRequest(format!(
            "{field} requires amount and unit"
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

fn quantity_from_ingredient(row: &RecipeIngredientRow) -> ApiResult<StructuredQuantityDto> {
    Ok(StructuredQuantityDto {
        amount: row.amount.clone(),
        unit: row.unit.clone(),
        family: parse_optional_family(row.family.as_deref())?,
        range: optional_range(row.range_min.clone(), row.range_max.clone()),
        to_taste: row.to_taste,
        preparation_note: None,
    })
}

fn quantity_from_output(row: &RecipeOutputRow) -> ApiResult<StructuredQuantityDto> {
    Ok(StructuredQuantityDto {
        amount: row.amount.clone(),
        unit: row.unit.clone(),
        family: parse_optional_family(row.family.as_deref())?,
        range: optional_range(row.range_min.clone(), row.range_max.clone()),
        to_taste: row.to_taste,
        preparation_note: row.preparation_note.clone(),
    })
}

fn scale_quantity(quantity: StructuredQuantityDto, factor: Decimal) -> StructuredQuantityDto {
    StructuredQuantityDto {
        amount: quantity
            .amount
            .as_deref()
            .and_then(|value| scale_decimal(value, factor)),
        range: quantity.range.as_ref().map(|range| QuantityRangeDto {
            min: scale_decimal(&range.min, factor).unwrap_or_else(|| range.min.clone()),
            max: scale_decimal(&range.max, factor).unwrap_or_else(|| range.max.clone()),
        }),
        ..quantity
    }
}

fn scale_decimal(value: &str, factor: Decimal) -> Option<String> {
    let value = Decimal::from_str(value).ok()?;
    Some((value * factor).normalize().to_string())
}

fn parse_optional_family(value: Option<&str>) -> ApiResult<Option<UnitFamily>> {
    value
        .map(|family| {
            UnitFamily::from_str_ci(family).ok_or_else(|| {
                ApiError::Internal(anyhow::anyhow!("unknown recipe unit family: {family}"))
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

fn ingredient_payload(item: &SanitizedRecipeIngredient) -> Value {
    json!({
        "ingredient_id": item.ingredient_id,
        "product_id": item.product_id,
        "display_name": item.display_name,
        "quantity": quantity_payload(&item.quantity),
        "preparation": item.preparation,
        "optional": item.optional,
        "group_label": item.group_label,
    })
}

fn step_payload(item: &SanitizedRecipeStep) -> Value {
    json!({
        "instruction": item.instruction,
        "timers": item.timers_json,
        "equipment": item.equipment_json,
        "ingredient_refs": item.ingredient_refs_json,
    })
}

fn output_payload(item: &SanitizedRecipeOutput) -> Value {
    json!({
        "product_id": item.product_id,
        "name": item.name,
        "quantity": quantity_payload(&item.quantity),
        "expires_after_days": item.expires_after_days,
        "storage_notes": item.storage_notes,
    })
}

fn quantity_payload(item: &SanitizedQuantity) -> Value {
    json!({
        "amount": item.amount,
        "unit": item.unit,
        "family": item.family.map(UnitFamily::as_str),
        "range_min": item.range_min,
        "range_max": item.range_max,
        "to_taste": item.to_taste,
        "preparation_note": item.preparation_note,
    })
}

fn default_recipe_source() -> RecipeSource {
    RecipeSource::Manual
}

fn default_recipe_visibility() -> RecipeVisibility {
    RecipeVisibility::Household
}

fn json_string_vec(raw: &str, field: &str) -> ApiResult<Vec<String>> {
    serde_json::from_str(raw).map_err(|err| {
        ApiError::Internal(anyhow::anyhow!(
            "invalid string-list JSON stored in {field}: {err}"
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

fn validate_positive_decimal(field: &str, value: String) -> ApiResult<String> {
    let value = value.trim();
    let parsed = Decimal::from_str(value)
        .map_err(|_| ApiError::BadRequest(format!("{field} must be a decimal")))?;
    if parsed <= Decimal::ZERO {
        return Err(ApiError::BadRequest(format!("{field} must be > 0")));
    }
    Ok(parsed.normalize().to_string())
}

async fn build_preflight(
    state: &AppState,
    household_id: Uuid,
    req: &RecipeExecutionRequest,
) -> ApiResult<RecipeExecutionPreflightResponse> {
    let serving_scale = req.serving_scale.as_deref().unwrap_or("1");
    validate_positive_decimal_field("serving_scale", serving_scale)?;
    if req.ingredients.is_empty() && req.outputs.is_empty() {
        return Err(ApiError::BadRequest(
            "recipe execution needs at least one ingredient or output".into(),
        ));
    }

    let measurement_system = household_measurement_system(state, household_id).await?;
    let use_expiring_first = req.use_expiring_first.unwrap_or(true);
    let mut ingredients = Vec::new();
    let mut missing_ingredients = Vec::new();
    let mut warnings = Vec::new();

    for ingredient in &req.ingredients {
        validate_positive_decimal_field("quantity", &ingredient.quantity)?;
        let requested = Decimal::from_str(&ingredient.quantity)
            .map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?;
        let resolved =
            resolve_execution_ingredient(state, household_id, ingredient, measurement_system)
                .await?;
        let Some(resolved) = resolved else {
            missing_ingredients.push(RecipeMissingIngredientDto {
                line_id: ingredient.line_id.clone(),
                display_name: ingredient.display_name.clone(),
                ingredient_id: ingredient.ingredient_id,
                product_id: ingredient.product_id,
                requested_quantity: ingredient.quantity.clone(),
                requested_unit: ingredient.unit.clone(),
                missing_quantity: ingredient.quantity.clone(),
                optional: ingredient.optional,
                reason: "no mapped product was selected or available".into(),
            });
            continue;
        };

        let mut batches = qm_db::stock::list_active_batches(
            &state.db,
            household_id,
            resolved.product.id,
            ingredient.location_id,
        )
        .await?;
        if !use_expiring_first {
            batches.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        }

        let mut remaining = resolved.inventory_quantity;
        let mut matched_batches = Vec::new();
        for batch in batches {
            if remaining <= Decimal::ZERO {
                break;
            }
            let available = Decimal::from_str(&batch.quantity)
                .map_err(|err| ApiError::Internal(anyhow::Error::from(err)))?;
            if available <= Decimal::ZERO {
                continue;
            }
            let available_in_requested = convert_decimal(
                available,
                &batch.unit,
                &resolved.inventory_unit,
                measurement_system,
            )?;
            if available_in_requested <= Decimal::ZERO {
                continue;
            }
            let take_requested = remaining.min(available_in_requested);
            let take_batch = convert_decimal(
                take_requested,
                &resolved.inventory_unit,
                &batch.unit,
                measurement_system,
            )?;
            let depleted = (available - take_batch) <= Decimal::ZERO;
            matched_batches.push(RecipeMatchedBatchDto {
                batch_id: batch.id,
                location_id: batch.location_id,
                quantity: normalize_decimal(take_batch),
                unit: batch.unit,
                quantity_in_requested_unit: normalize_decimal(take_requested),
                requested_unit: resolved.inventory_unit.clone(),
                expires_on: batch.expires_on,
                depleted,
            });
            remaining -= take_requested;
        }

        let missing_quantity = if remaining > Decimal::ZERO {
            let missing = normalize_decimal(remaining);
            missing_ingredients.push(RecipeMissingIngredientDto {
                line_id: ingredient.line_id.clone(),
                display_name: ingredient.display_name.clone(),
                ingredient_id: ingredient.ingredient_id,
                product_id: Some(resolved.product.id),
                requested_quantity: ingredient.quantity.clone(),
                requested_unit: ingredient.unit.clone(),
                missing_quantity: missing.clone(),
                optional: ingredient.optional,
                reason: "insufficient stock".into(),
            });
            Some(missing)
        } else {
            None
        };

        if resolved.conversion_assumption.is_some() {
            warnings.push(format!(
                "{} uses recipe-layer conversion metadata",
                ingredient_label(ingredient)
            ));
        }

        ingredients.push(RecipeIngredientPlanDto {
            line_id: ingredient.line_id.clone(),
            display_name: ingredient.display_name.clone(),
            ingredient_id: ingredient.ingredient_id,
            mapping_id: resolved.mapping_id,
            product: resolved.product.try_into()?,
            requested_quantity: normalize_decimal(requested),
            requested_unit: ingredient.unit.clone(),
            inventory_quantity: normalize_decimal(resolved.inventory_quantity),
            inventory_unit: resolved.inventory_unit,
            optional: ingredient.optional,
            substitution_of: ingredient.substitution_of.clone(),
            conversion_assumption: resolved.conversion_assumption,
            matched_batches,
            missing_quantity,
        });
    }

    let mut outputs = Vec::with_capacity(req.outputs.len());
    for output in &req.outputs {
        validate_positive_decimal_field("output quantity", &output.quantity)?;
        validate_optional_date(output.produced_on.as_deref())?;
        validate_optional_date(output.expires_on.as_deref())?;
        let product = load_product_for_write(state, household_id, output.product_id).await?;
        validate_unit_family(&output.unit, &product.family)?;
        validate_location(state, household_id, output.location_id).await?;
        outputs.push(RecipeOutputPreviewDto {
            product: product.try_into()?,
            location_id: output.location_id,
            quantity: output.quantity.clone(),
            unit: output.unit.clone(),
            produced_on: output.produced_on.clone(),
            expires_on: output.expires_on.clone(),
            note: output.note.clone(),
        });
    }

    let can_execute = missing_ingredients.iter().all(|missing| missing.optional);
    Ok(RecipeExecutionPreflightResponse {
        recipe_id: req.recipe_id,
        recipe_version_id: req.recipe_version_id,
        recipe_name: req.recipe_name.clone(),
        serving_scale: serving_scale.to_owned(),
        use_expiring_first,
        ingredients,
        missing_ingredients,
        outputs,
        warnings,
        can_execute,
    })
}

struct ResolvedIngredient {
    product: ProductRow,
    mapping_id: Option<Uuid>,
    inventory_quantity: Decimal,
    inventory_unit: String,
    conversion_assumption: Option<String>,
}

async fn resolve_execution_ingredient(
    state: &AppState,
    household_id: Uuid,
    ingredient: &RecipeExecutionIngredientRequest,
    measurement_system: MeasurementSystem,
) -> ApiResult<Option<ResolvedIngredient>> {
    if let Some(product_id) = ingredient.product_id {
        let product = load_product_for_write(state, household_id, product_id).await?;
        validate_unit_family(&ingredient.unit, &product.family)?;
        return Ok(Some(ResolvedIngredient {
            product,
            mapping_id: None,
            inventory_quantity: Decimal::from_str(&ingredient.quantity)
                .map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?,
            inventory_unit: ingredient.unit.clone(),
            conversion_assumption: None,
        }));
    }

    let Some(ingredient_id) = ingredient.ingredient_id else {
        return Ok(None);
    };
    if qm_db::ingredients::find(&state.db, household_id, ingredient_id)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    let mappings =
        qm_db::ingredients::list_mappings(&state.db, household_id, ingredient_id).await?;
    let Some(mapping) = mappings.into_iter().next() else {
        return Ok(None);
    };
    let product = load_product_for_write(state, household_id, mapping.product_id).await?;

    let requested = Decimal::from_str(&ingredient.quantity)
        .map_err(|_| ApiError::BadRequest("quantity not a valid decimal".into()))?;
    if let (Some(recipe_amount), Some(recipe_unit), Some(inventory_amount), Some(inventory_unit)) = (
        mapping.recipe_amount.as_deref(),
        mapping.recipe_unit.as_deref(),
        mapping.inventory_amount.as_deref(),
        mapping.inventory_unit.as_deref(),
    ) {
        validate_unit_family(inventory_unit, &product.family)?;
        let recipe_amount = Decimal::from_str(recipe_amount).map_err(|_| {
            ApiError::BadRequest("ingredient mapping recipe quantity is not a valid decimal".into())
        })?;
        let inventory_amount = Decimal::from_str(inventory_amount).map_err(|_| {
            ApiError::BadRequest(
                "ingredient mapping inventory quantity is not a valid decimal".into(),
            )
        })?;
        if recipe_amount <= Decimal::ZERO || inventory_amount <= Decimal::ZERO {
            return Err(ApiError::BadRequest(
                "ingredient mapping conversion quantities must be positive".into(),
            ));
        }
        let requested_in_mapping_unit =
            convert_decimal(requested, &ingredient.unit, recipe_unit, measurement_system)?;
        let inventory_quantity = requested_in_mapping_unit / recipe_amount * inventory_amount;
        return Ok(Some(ResolvedIngredient {
            product,
            mapping_id: Some(mapping.id),
            inventory_quantity,
            inventory_unit: inventory_unit.to_owned(),
            conversion_assumption: Some(format!(
                "{} {recipe_unit} maps to {} {inventory_unit}",
                normalize_decimal(recipe_amount),
                normalize_decimal(inventory_amount)
            )),
        }));
    }

    validate_unit_family(&ingredient.unit, &product.family)?;
    Ok(Some(ResolvedIngredient {
        product,
        mapping_id: Some(mapping.id),
        inventory_quantity: requested,
        inventory_unit: ingredient.unit.clone(),
        conversion_assumption: None,
    }))
}

fn consumption_from_plan(
    plan: &RecipeExecutionPreflightResponse,
) -> ApiResult<Vec<BatchConsumption>> {
    let mut consumption = Vec::new();
    for ingredient in &plan.ingredients {
        for batch in &ingredient.matched_batches {
            let quantity = Decimal::from_str(&batch.quantity)
                .map_err(|_| ApiError::BadRequest("planned quantity not a valid decimal".into()))?;
            if quantity > Decimal::ZERO {
                consumption.push(BatchConsumption {
                    batch_id: batch.batch_id,
                    quantity,
                    depletes: batch.depleted,
                });
            }
        }
    }
    Ok(consumption)
}

fn recipe_stock_outputs(outputs: &[RecipeExecutionOutputRequest]) -> Vec<RecipeStockOutput<'_>> {
    outputs
        .iter()
        .map(|output| RecipeStockOutput {
            product_id: output.product_id,
            location_id: output.location_id,
            quantity: &output.quantity,
            unit: &output.unit,
            produced_on: output.produced_on.as_deref(),
            expires_on: output.expires_on.as_deref(),
            note: output.note.as_deref(),
        })
        .collect()
}

async fn output_batch_dtos(
    state: &AppState,
    household_id: Uuid,
    execution_id: Uuid,
) -> ApiResult<Vec<StockBatchDto>> {
    let ids = qm_db::recipes::output_batch_ids_for_execution(&state.db, household_id, execution_id)
        .await?;
    let mut output_batches = Vec::with_capacity(ids.len());
    for id in ids {
        output_batches.push(stock_dto(state, household_id, id).await?);
    }
    Ok(output_batches)
}

async fn stock_dto(state: &AppState, household_id: Uuid, id: Uuid) -> ApiResult<StockBatchDto> {
    let row = qm_db::stock::get_with_product(&state.db, household_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    row.try_into()
}

async fn load_product_for_write(
    state: &AppState,
    household_id: Uuid,
    product_id: Uuid,
) -> ApiResult<ProductRow> {
    qm_db::products::find_for_household(&state.db, household_id, product_id)
        .await?
        .ok_or(ApiError::NotFound)
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

async fn household_measurement_system(
    state: &AppState,
    household_id: Uuid,
) -> ApiResult<MeasurementSystem> {
    let household = qm_db::households::find_by_id(&state.db, household_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    crate::routes::households::measurement_system_from_db(&household.measurement_system)
}

fn validate_positive_decimal_field(field: &str, value: &str) -> ApiResult<()> {
    let parsed = Decimal::from_str(value)
        .map_err(|_| ApiError::BadRequest(format!("{field} not a valid decimal")))?;
    if parsed <= Decimal::ZERO {
        return Err(ApiError::BadRequest(format!("{field} must be > 0")));
    }
    Ok(())
}

fn validate_optional_date(value: Option<&str>) -> ApiResult<()> {
    if let Some(value) = value {
        Date::from_str(value)
            .map(|_| ())
            .map_err(|_| ApiError::BadRequest(format!("date must be YYYY-MM-DD (got {value})")))?;
    }
    Ok(())
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

fn ingredient_label(ingredient: &RecipeExecutionIngredientRequest) -> String {
    ingredient
        .display_name
        .clone()
        .or_else(|| ingredient.line_id.clone())
        .unwrap_or_else(|| "ingredient".into())
}
