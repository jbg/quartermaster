use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{audited_sql, now_utc_rfc3339, Database};

const RECIPE_COLS: &str = "id, household_id, name, description, serving_count, source, \
                           visibility, tags_json, latest_version_id, created_by, updated_by, \
                           created_at, updated_at";
const VERSION_COLS: &str = "id, household_id, recipe_id, version_number, serving_count, \
                            source_text, payload_json, created_by, created_at";
const INGREDIENT_COLS: &str = "id, household_id, recipe_id, recipe_version_id, sort_order, \
                               ingredient_id, product_id, display_name, amount, unit, family, \
                               range_min, range_max, to_taste, preparation, optional, \
                               group_label, substitution_hints_json, created_at";
const STEP_COLS: &str = "id, household_id, recipe_id, recipe_version_id, sort_order, instruction, \
                         timers_json, equipment_json, ingredient_refs_json, created_at";
const OUTPUT_COLS: &str =
    "id, household_id, recipe_id, recipe_version_id, sort_order, product_id, \
                           name, amount, unit, family, range_min, range_max, to_taste, \
                           preparation_note, expires_after_days, storage_notes, created_at";
const PROVENANCE_COLS: &str = "id, household_id, recipe_id, recipe_version_id, source_type, \
                               imported_url, imported_file_name, imported_text, prompt_version, \
                               model, user_edits_json, parser_confidence, created_at";

#[derive(Debug, Clone, Serialize)]
pub struct RecipeRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub serving_count: String,
    pub source: String,
    pub visibility: String,
    pub tags_json: String,
    pub latest_version_id: Option<Uuid>,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeVersionRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub recipe_id: Uuid,
    pub version_number: i64,
    pub serving_count: String,
    pub source_text: Option<String>,
    pub payload_json: String,
    pub created_by: Option<Uuid>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeIngredientRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub recipe_id: Uuid,
    pub recipe_version_id: Uuid,
    pub sort_order: i64,
    pub ingredient_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub display_name: String,
    pub amount: Option<String>,
    pub unit: Option<String>,
    pub family: Option<String>,
    pub range_min: Option<String>,
    pub range_max: Option<String>,
    pub to_taste: bool,
    pub preparation: Option<String>,
    pub optional: bool,
    pub group_label: Option<String>,
    pub substitution_hints_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeStepRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub recipe_id: Uuid,
    pub recipe_version_id: Uuid,
    pub sort_order: i64,
    pub instruction: String,
    pub timers_json: String,
    pub equipment_json: String,
    pub ingredient_refs_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeOutputRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub recipe_id: Uuid,
    pub recipe_version_id: Uuid,
    pub sort_order: i64,
    pub product_id: Option<Uuid>,
    pub name: String,
    pub amount: Option<String>,
    pub unit: Option<String>,
    pub family: Option<String>,
    pub range_min: Option<String>,
    pub range_max: Option<String>,
    pub to_taste: bool,
    pub preparation_note: Option<String>,
    pub expires_after_days: Option<i64>,
    pub storage_notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeProvenanceRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub recipe_id: Uuid,
    pub recipe_version_id: Uuid,
    pub source_type: String,
    pub imported_url: Option<String>,
    pub imported_file_name: Option<String>,
    pub imported_text: Option<String>,
    pub prompt_version: Option<String>,
    pub model: Option<String>,
    pub user_edits_json: String,
    pub parser_confidence: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct RecipeFull {
    pub recipe: RecipeRow,
    pub version: RecipeVersionRow,
    pub ingredients: Vec<RecipeIngredientRow>,
    pub steps: Vec<RecipeStepRow>,
    pub outputs: Vec<RecipeOutputRow>,
    pub provenance: Vec<RecipeProvenanceRow>,
}

#[derive(Debug, Clone)]
pub struct NewRecipe<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub serving_count: &'a str,
    pub source: &'a str,
    pub visibility: &'a str,
    pub tags_json: &'a str,
    pub source_text: Option<&'a str>,
    pub payload_json: &'a str,
    pub ingredients: Vec<NewRecipeIngredient<'a>>,
    pub steps: Vec<NewRecipeStep<'a>>,
    pub outputs: Vec<NewRecipeOutput<'a>>,
    pub provenance: Vec<NewRecipeProvenance<'a>>,
}

#[derive(Debug, Clone)]
pub struct NewRecipeIngredient<'a> {
    pub ingredient_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub display_name: &'a str,
    pub amount: Option<&'a str>,
    pub unit: Option<&'a str>,
    pub family: Option<&'a str>,
    pub range_min: Option<&'a str>,
    pub range_max: Option<&'a str>,
    pub to_taste: bool,
    pub preparation: Option<&'a str>,
    pub optional: bool,
    pub group_label: Option<&'a str>,
    pub substitution_hints_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewRecipeStep<'a> {
    pub instruction: &'a str,
    pub timers_json: &'a str,
    pub equipment_json: &'a str,
    pub ingredient_refs_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewRecipeOutput<'a> {
    pub product_id: Option<Uuid>,
    pub name: &'a str,
    pub amount: Option<&'a str>,
    pub unit: Option<&'a str>,
    pub family: Option<&'a str>,
    pub range_min: Option<&'a str>,
    pub range_max: Option<&'a str>,
    pub to_taste: bool,
    pub preparation_note: Option<&'a str>,
    pub expires_after_days: Option<i64>,
    pub storage_notes: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewRecipeProvenance<'a> {
    pub source_type: &'a str,
    pub imported_url: Option<&'a str>,
    pub imported_file_name: Option<&'a str>,
    pub imported_text: Option<&'a str>,
    pub prompt_version: Option<&'a str>,
    pub model: Option<&'a str>,
    pub user_edits_json: &'a str,
    pub parser_confidence: Option<&'a str>,
}

pub async fn list(db: &Database, household_id: Uuid) -> Result<Vec<RecipeRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {RECIPE_COLS} FROM recipe \
         WHERE household_id = ? ORDER BY updated_at DESC, name ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_recipe).collect()
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<RecipeFull>, sqlx::Error> {
    let sql = format!("SELECT {RECIPE_COLS} FROM recipe WHERE household_id = ? AND id = ?");
    let Some(recipe) = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?
        .map(row_to_recipe)
        .transpose()?
    else {
        return Ok(None);
    };
    let Some(version_id) = recipe.latest_version_id else {
        return Err(sqlx::Error::RowNotFound);
    };
    let version = find_version(db, household_id, version_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Some(RecipeFull {
        ingredients: list_ingredients(db, household_id, version.id).await?,
        steps: list_steps(db, household_id, version.id).await?,
        outputs: list_outputs(db, household_id, version.id).await?,
        provenance: list_provenance(db, household_id, version.id).await?,
        recipe,
        version,
    }))
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    actor_user_id: Uuid,
    new: &NewRecipe<'_>,
) -> Result<RecipeFull, sqlx::Error> {
    let recipe_id = Uuid::now_v7();
    let version_id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;
    sqlx::query(
        "INSERT INTO recipe \
         (id, household_id, name, description, serving_count, source, visibility, tags_json, \
          latest_version_id, created_by, updated_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(recipe_id.to_string())
    .bind(household_id.to_string())
    .bind(new.name)
    .bind(new.description)
    .bind(new.serving_count)
    .bind(new.source)
    .bind(new.visibility)
    .bind(new.tags_json)
    .bind(version_id.to_string())
    .bind(actor_user_id.to_string())
    .bind(actor_user_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    insert_version_graph_tx(
        &mut tx,
        household_id,
        recipe_id,
        version_id,
        1,
        actor_user_id,
        new,
        &now,
    )
    .await?;
    tx.commit().await?;
    find(db, household_id, recipe_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update(
    db: &Database,
    household_id: Uuid,
    actor_user_id: Uuid,
    id: Uuid,
    upd: &NewRecipe<'_>,
) -> Result<Option<RecipeFull>, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT COALESCE(MAX(version_number), 0) FROM recipe_version \
         WHERE household_id = ? AND recipe_id = ?",
    )
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&mut *tx)
    .await?;
    let Some((latest_number,)) = existing else {
        return Ok(None);
    };
    let version_id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let res = sqlx::query(
        "UPDATE recipe \
         SET name = ?, description = ?, serving_count = ?, source = ?, visibility = ?, \
             tags_json = ?, latest_version_id = ?, updated_by = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(upd.name)
    .bind(upd.description)
    .bind(upd.serving_count)
    .bind(upd.source)
    .bind(upd.visibility)
    .bind(upd.tags_json)
    .bind(version_id.to_string())
    .bind(actor_user_id.to_string())
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    insert_version_graph_tx(
        &mut tx,
        household_id,
        id,
        version_id,
        latest_number + 1,
        actor_user_id,
        upd,
        &now,
    )
    .await?;
    tx.commit().await?;
    find(db, household_id, id).await
}

pub async fn delete(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM recipe WHERE household_id = ? AND id = ?")
        .bind(household_id.to_string())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

async fn insert_version_graph_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    recipe_id: Uuid,
    version_id: Uuid,
    version_number: i64,
    actor_user_id: Uuid,
    new: &NewRecipe<'_>,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO recipe_version \
         (id, household_id, recipe_id, version_number, serving_count, source_text, payload_json, \
          created_by, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(version_id.to_string())
    .bind(household_id.to_string())
    .bind(recipe_id.to_string())
    .bind(version_number)
    .bind(new.serving_count)
    .bind(new.source_text)
    .bind(new.payload_json)
    .bind(actor_user_id.to_string())
    .bind(now)
    .execute(&mut **tx)
    .await?;

    for (idx, ingredient) in new.ingredients.iter().enumerate() {
        sqlx::query(
            "INSERT INTO recipe_ingredient \
             (id, household_id, recipe_id, recipe_version_id, sort_order, ingredient_id, \
              product_id, display_name, amount, unit, family, range_min, range_max, to_taste, \
              preparation, optional, group_label, substitution_hints_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(household_id.to_string())
        .bind(recipe_id.to_string())
        .bind(version_id.to_string())
        .bind(idx as i64)
        .bind(ingredient.ingredient_id.map(|id| id.to_string()))
        .bind(ingredient.product_id.map(|id| id.to_string()))
        .bind(ingredient.display_name)
        .bind(ingredient.amount)
        .bind(ingredient.unit)
        .bind(ingredient.family)
        .bind(ingredient.range_min)
        .bind(ingredient.range_max)
        .bind(bool_int(ingredient.to_taste))
        .bind(ingredient.preparation)
        .bind(bool_int(ingredient.optional))
        .bind(ingredient.group_label)
        .bind(ingredient.substitution_hints_json)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    for (idx, step) in new.steps.iter().enumerate() {
        sqlx::query(
            "INSERT INTO recipe_step \
             (id, household_id, recipe_id, recipe_version_id, sort_order, instruction, \
              timers_json, equipment_json, ingredient_refs_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(household_id.to_string())
        .bind(recipe_id.to_string())
        .bind(version_id.to_string())
        .bind(idx as i64)
        .bind(step.instruction)
        .bind(step.timers_json)
        .bind(step.equipment_json)
        .bind(step.ingredient_refs_json)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    for (idx, output) in new.outputs.iter().enumerate() {
        sqlx::query(
            "INSERT INTO recipe_output \
             (id, household_id, recipe_id, recipe_version_id, sort_order, product_id, name, \
              amount, unit, family, range_min, range_max, to_taste, preparation_note, \
              expires_after_days, storage_notes, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(household_id.to_string())
        .bind(recipe_id.to_string())
        .bind(version_id.to_string())
        .bind(idx as i64)
        .bind(output.product_id.map(|id| id.to_string()))
        .bind(output.name)
        .bind(output.amount)
        .bind(output.unit)
        .bind(output.family)
        .bind(output.range_min)
        .bind(output.range_max)
        .bind(bool_int(output.to_taste))
        .bind(output.preparation_note)
        .bind(output.expires_after_days)
        .bind(output.storage_notes)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    for provenance in &new.provenance {
        sqlx::query(
            "INSERT INTO recipe_provenance \
             (id, household_id, recipe_id, recipe_version_id, source_type, imported_url, \
              imported_file_name, imported_text, prompt_version, model, user_edits_json, \
              parser_confidence, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(household_id.to_string())
        .bind(recipe_id.to_string())
        .bind(version_id.to_string())
        .bind(provenance.source_type)
        .bind(provenance.imported_url)
        .bind(provenance.imported_file_name)
        .bind(provenance.imported_text)
        .bind(provenance.prompt_version)
        .bind(provenance.model)
        .bind(provenance.user_edits_json)
        .bind(provenance.parser_confidence)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn find_version(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<RecipeVersionRow>, sqlx::Error> {
    let sql =
        format!("SELECT {VERSION_COLS} FROM recipe_version WHERE household_id = ? AND id = ?");
    let row = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_version).transpose()
}

async fn list_ingredients(
    db: &Database,
    household_id: Uuid,
    version_id: Uuid,
) -> Result<Vec<RecipeIngredientRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {INGREDIENT_COLS} FROM recipe_ingredient \
         WHERE household_id = ? AND recipe_version_id = ? ORDER BY sort_order ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(version_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_ingredient).collect()
}

async fn list_steps(
    db: &Database,
    household_id: Uuid,
    version_id: Uuid,
) -> Result<Vec<RecipeStepRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {STEP_COLS} FROM recipe_step \
         WHERE household_id = ? AND recipe_version_id = ? ORDER BY sort_order ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(version_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_step).collect()
}

async fn list_outputs(
    db: &Database,
    household_id: Uuid,
    version_id: Uuid,
) -> Result<Vec<RecipeOutputRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {OUTPUT_COLS} FROM recipe_output \
         WHERE household_id = ? AND recipe_version_id = ? ORDER BY sort_order ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(version_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_output).collect()
}

async fn list_provenance(
    db: &Database,
    household_id: Uuid,
    version_id: Uuid,
) -> Result<Vec<RecipeProvenanceRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {PROVENANCE_COLS} FROM recipe_provenance \
         WHERE household_id = ? AND recipe_version_id = ? ORDER BY created_at ASC, id ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(version_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_provenance).collect()
}

fn row_uuid(row: &sqlx::any::AnyRow, name: &str) -> Result<Uuid, sqlx::Error> {
    let raw: String = row.try_get(name)?;
    Uuid::parse_str(&raw).map_err(|err| sqlx::Error::Decode(Box::new(err)))
}

fn optional_uuid(row: &sqlx::any::AnyRow, name: &str) -> Result<Option<Uuid>, sqlx::Error> {
    let raw: Option<String> = row.try_get(name)?;
    raw.map(|value| Uuid::parse_str(&value).map_err(|err| sqlx::Error::Decode(Box::new(err))))
        .transpose()
}

fn bool_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn row_bool(row: &sqlx::any::AnyRow, name: &str) -> Result<bool, sqlx::Error> {
    Ok(row.try_get::<i64, _>(name)? != 0)
}

fn row_to_recipe(row: sqlx::any::AnyRow) -> Result<RecipeRow, sqlx::Error> {
    Ok(RecipeRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        serving_count: row.try_get("serving_count")?,
        source: row.try_get("source")?,
        visibility: row.try_get("visibility")?,
        tags_json: row.try_get("tags_json")?,
        latest_version_id: optional_uuid(&row, "latest_version_id")?,
        created_by: optional_uuid(&row, "created_by")?,
        updated_by: optional_uuid(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_version(row: sqlx::any::AnyRow) -> Result<RecipeVersionRow, sqlx::Error> {
    Ok(RecipeVersionRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        recipe_id: row_uuid(&row, "recipe_id")?,
        version_number: row.try_get("version_number")?,
        serving_count: row.try_get("serving_count")?,
        source_text: row.try_get("source_text")?,
        payload_json: row.try_get("payload_json")?,
        created_by: optional_uuid(&row, "created_by")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_ingredient(row: sqlx::any::AnyRow) -> Result<RecipeIngredientRow, sqlx::Error> {
    Ok(RecipeIngredientRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        recipe_id: row_uuid(&row, "recipe_id")?,
        recipe_version_id: row_uuid(&row, "recipe_version_id")?,
        sort_order: row.try_get("sort_order")?,
        ingredient_id: optional_uuid(&row, "ingredient_id")?,
        product_id: optional_uuid(&row, "product_id")?,
        display_name: row.try_get("display_name")?,
        amount: row.try_get("amount")?,
        unit: row.try_get("unit")?,
        family: row.try_get("family")?,
        range_min: row.try_get("range_min")?,
        range_max: row.try_get("range_max")?,
        to_taste: row_bool(&row, "to_taste")?,
        preparation: row.try_get("preparation")?,
        optional: row_bool(&row, "optional")?,
        group_label: row.try_get("group_label")?,
        substitution_hints_json: row.try_get("substitution_hints_json")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_step(row: sqlx::any::AnyRow) -> Result<RecipeStepRow, sqlx::Error> {
    Ok(RecipeStepRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        recipe_id: row_uuid(&row, "recipe_id")?,
        recipe_version_id: row_uuid(&row, "recipe_version_id")?,
        sort_order: row.try_get("sort_order")?,
        instruction: row.try_get("instruction")?,
        timers_json: row.try_get("timers_json")?,
        equipment_json: row.try_get("equipment_json")?,
        ingredient_refs_json: row.try_get("ingredient_refs_json")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_output(row: sqlx::any::AnyRow) -> Result<RecipeOutputRow, sqlx::Error> {
    Ok(RecipeOutputRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        recipe_id: row_uuid(&row, "recipe_id")?,
        recipe_version_id: row_uuid(&row, "recipe_version_id")?,
        sort_order: row.try_get("sort_order")?,
        product_id: optional_uuid(&row, "product_id")?,
        name: row.try_get("name")?,
        amount: row.try_get("amount")?,
        unit: row.try_get("unit")?,
        family: row.try_get("family")?,
        range_min: row.try_get("range_min")?,
        range_max: row.try_get("range_max")?,
        to_taste: row_bool(&row, "to_taste")?,
        preparation_note: row.try_get("preparation_note")?,
        expires_after_days: row.try_get("expires_after_days")?,
        storage_notes: row.try_get("storage_notes")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_provenance(row: sqlx::any::AnyRow) -> Result<RecipeProvenanceRow, sqlx::Error> {
    Ok(RecipeProvenanceRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        recipe_id: row_uuid(&row, "recipe_id")?,
        recipe_version_id: row_uuid(&row, "recipe_version_id")?,
        source_type: row.try_get("source_type")?,
        imported_url: row.try_get("imported_url")?,
        imported_file_name: row.try_get("imported_file_name")?,
        imported_text: row.try_get("imported_text")?,
        prompt_version: row.try_get("prompt_version")?,
        model: row.try_get("model")?,
        user_edits_json: row.try_get("user_edits_json")?,
        parser_confidence: row.try_get("parser_confidence")?,
        created_at: row.try_get("created_at")?,
    })
}
