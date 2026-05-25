use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

const INGREDIENT_COLS: &str = "id, household_id, display_name, category, default_family, \
                               aliases_json, dietary_tags_json, allergen_tags_json, notes, \
                               created_at, updated_at";

const MAPPING_COLS: &str = "id, household_id, ingredient_id, product_id, rank, match_kind, \
                            match_metadata_json, recipe_amount, recipe_unit, recipe_family, \
                            recipe_range_min, recipe_range_max, recipe_to_taste, \
                            recipe_preparation_note, inventory_amount, inventory_unit, \
                            inventory_family, inventory_range_min, inventory_range_max, \
                            inventory_to_taste, inventory_preparation_note, \
                            conversion_provenance, conversion_notes, created_at";

const PRODUCT_METADATA_COLS: &str = "household_id, product_id, edible_yield_percent, \
                                    drained_quantity, drained_unit, density_recipe_quantity, \
                                    density_recipe_unit, density_inventory_quantity, \
                                    density_inventory_unit, density_provenance, \
                                    preparation_state, counts_as_aliases_json, notes, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct IngredientRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub display_name: String,
    pub category: Option<String>,
    pub default_family: Option<String>,
    pub aliases_json: String,
    pub dietary_tags_json: String,
    pub allergen_tags_json: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngredientProductMappingRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub ingredient_id: Uuid,
    pub product_id: Uuid,
    pub rank: i64,
    pub match_kind: String,
    pub match_metadata_json: String,
    pub recipe_amount: Option<String>,
    pub recipe_unit: Option<String>,
    pub recipe_family: Option<String>,
    pub recipe_range_min: Option<String>,
    pub recipe_range_max: Option<String>,
    pub recipe_to_taste: bool,
    pub recipe_preparation_note: Option<String>,
    pub inventory_amount: Option<String>,
    pub inventory_unit: Option<String>,
    pub inventory_family: Option<String>,
    pub inventory_range_min: Option<String>,
    pub inventory_range_max: Option<String>,
    pub inventory_to_taste: bool,
    pub inventory_preparation_note: Option<String>,
    pub conversion_provenance: Option<String>,
    pub conversion_notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductRecipeMetadataRow {
    pub household_id: Uuid,
    pub product_id: Uuid,
    pub edible_yield_percent: Option<String>,
    pub drained_quantity: Option<String>,
    pub drained_unit: Option<String>,
    pub density_recipe_quantity: Option<String>,
    pub density_recipe_unit: Option<String>,
    pub density_inventory_quantity: Option<String>,
    pub density_inventory_unit: Option<String>,
    pub density_provenance: Option<String>,
    pub preparation_state: Option<String>,
    pub counts_as_aliases_json: String,
    pub notes: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngredientAvailabilityRow {
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

#[derive(Debug, Clone)]
pub struct NewIngredient<'a> {
    pub display_name: &'a str,
    pub category: Option<&'a str>,
    pub default_family: Option<&'a str>,
    pub aliases_json: &'a str,
    pub dietary_tags_json: &'a str,
    pub allergen_tags_json: &'a str,
    pub notes: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewIngredientProductMapping<'a> {
    pub product_id: Uuid,
    pub rank: i64,
    pub match_kind: &'a str,
    pub match_metadata_json: &'a str,
    pub recipe_amount: Option<&'a str>,
    pub recipe_unit: Option<&'a str>,
    pub recipe_family: Option<&'a str>,
    pub recipe_range_min: Option<&'a str>,
    pub recipe_range_max: Option<&'a str>,
    pub recipe_to_taste: bool,
    pub recipe_preparation_note: Option<&'a str>,
    pub inventory_amount: Option<&'a str>,
    pub inventory_unit: Option<&'a str>,
    pub inventory_family: Option<&'a str>,
    pub inventory_range_min: Option<&'a str>,
    pub inventory_range_max: Option<&'a str>,
    pub inventory_to_taste: bool,
    pub inventory_preparation_note: Option<&'a str>,
    pub conversion_provenance: Option<&'a str>,
    pub conversion_notes: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewProductRecipeMetadata<'a> {
    pub edible_yield_percent: Option<&'a str>,
    pub drained_quantity: Option<&'a str>,
    pub drained_unit: Option<&'a str>,
    pub density_recipe_quantity: Option<&'a str>,
    pub density_recipe_unit: Option<&'a str>,
    pub density_inventory_quantity: Option<&'a str>,
    pub density_inventory_unit: Option<&'a str>,
    pub density_provenance: Option<&'a str>,
    pub preparation_state: Option<&'a str>,
    pub counts_as_aliases_json: &'a str,
    pub notes: Option<&'a str>,
}

pub async fn list(
    db: &Database,
    household_id: Uuid,
    query: Option<&str>,
    limit: i64,
) -> Result<Vec<IngredientRow>, sqlx::Error> {
    let trimmed = query.map(str::trim).filter(|q| !q.is_empty());
    let search_clause = if trimmed.is_some() {
        "AND (LOWER(display_name) LIKE LOWER(?) OR LOWER(COALESCE(category, '')) LIKE LOWER(?))"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {INGREDIENT_COLS} \
         FROM ingredient \
         WHERE household_id = ? {search_clause} \
         ORDER BY display_name ASC \
         LIMIT ?"
    );
    let mut q = sqlx::query(&sql).bind(household_id.to_string());
    let pattern = trimmed.map(|value| format!("%{}%", value.replace('%', r"\%")));
    if let Some(pattern) = pattern.as_deref() {
        q = q.bind(pattern).bind(pattern);
    }
    let rows = q.bind(limit).fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_ingredient).collect()
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<IngredientRow>, sqlx::Error> {
    let sql = format!("SELECT {INGREDIENT_COLS} FROM ingredient WHERE household_id = ? AND id = ?");
    let row = sqlx::query(&sql)
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_ingredient).transpose()
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    new: &NewIngredient<'_>,
) -> Result<IngredientRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO ingredient \
         (id, household_id, display_name, category, default_family, aliases_json, \
          dietary_tags_json, allergen_tags_json, notes, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.display_name)
    .bind(new.category)
    .bind(new.default_family)
    .bind(new.aliases_json)
    .bind(new.dietary_tags_json)
    .bind(new.allergen_tags_json)
    .bind(new.notes)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    upd: &NewIngredient<'_>,
) -> Result<Option<IngredientRow>, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE ingredient \
         SET display_name = ?, category = ?, default_family = ?, aliases_json = ?, \
             dietary_tags_json = ?, allergen_tags_json = ?, notes = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(upd.display_name)
    .bind(upd.category)
    .bind(upd.default_family)
    .bind(upd.aliases_json)
    .bind(upd.dietary_tags_json)
    .bind(upd.allergen_tags_json)
    .bind(upd.notes)
    .bind(now_utc_rfc3339())
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find(db, household_id, id).await
}

pub async fn delete(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM ingredient WHERE household_id = ? AND id = ?")
        .bind(household_id.to_string())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn list_mappings(
    db: &Database,
    household_id: Uuid,
    ingredient_id: Uuid,
) -> Result<Vec<IngredientProductMappingRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {MAPPING_COLS} \
         FROM ingredient_product_mapping \
         WHERE household_id = ? AND ingredient_id = ? \
         ORDER BY rank ASC, created_at ASC"
    );
    let rows = sqlx::query(&sql)
        .bind(household_id.to_string())
        .bind(ingredient_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_mapping).collect()
}

pub async fn create_mapping(
    db: &Database,
    household_id: Uuid,
    ingredient_id: Uuid,
    new: &NewIngredientProductMapping<'_>,
) -> Result<IngredientProductMappingRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO ingredient_product_mapping \
         (id, household_id, ingredient_id, product_id, rank, match_kind, match_metadata_json, \
          recipe_amount, recipe_unit, recipe_family, recipe_range_min, recipe_range_max, \
          recipe_to_taste, recipe_preparation_note, inventory_amount, inventory_unit, \
          inventory_family, inventory_range_min, inventory_range_max, inventory_to_taste, \
          inventory_preparation_note, conversion_provenance, conversion_notes, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(ingredient_id.to_string())
    .bind(new.product_id.to_string())
    .bind(new.rank)
    .bind(new.match_kind)
    .bind(new.match_metadata_json)
    .bind(new.recipe_amount)
    .bind(new.recipe_unit)
    .bind(new.recipe_family)
    .bind(new.recipe_range_min)
    .bind(new.recipe_range_max)
    .bind(if new.recipe_to_taste { 1 } else { 0 })
    .bind(new.recipe_preparation_note)
    .bind(new.inventory_amount)
    .bind(new.inventory_unit)
    .bind(new.inventory_family)
    .bind(new.inventory_range_min)
    .bind(new.inventory_range_max)
    .bind(if new.inventory_to_taste { 1 } else { 0 })
    .bind(new.inventory_preparation_note)
    .bind(new.conversion_provenance)
    .bind(new.conversion_notes)
    .bind(&created_at)
    .execute(&db.pool)
    .await?;

    find_mapping(db, household_id, ingredient_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_mapping(
    db: &Database,
    household_id: Uuid,
    ingredient_id: Uuid,
    id: Uuid,
) -> Result<Option<IngredientProductMappingRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {MAPPING_COLS} \
         FROM ingredient_product_mapping \
         WHERE household_id = ? AND ingredient_id = ? AND id = ?"
    );
    let row = sqlx::query(&sql)
        .bind(household_id.to_string())
        .bind(ingredient_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_mapping).transpose()
}

pub async fn delete_mapping(
    db: &Database,
    household_id: Uuid,
    ingredient_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "DELETE FROM ingredient_product_mapping \
         WHERE household_id = ? AND ingredient_id = ? AND id = ?",
    )
    .bind(household_id.to_string())
    .bind(ingredient_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn upsert_product_metadata(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    new: &NewProductRecipeMetadata<'_>,
) -> Result<ProductRecipeMetadataRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO product_recipe_metadata \
         (household_id, product_id, edible_yield_percent, drained_quantity, drained_unit, \
          density_recipe_quantity, density_recipe_unit, density_inventory_quantity, \
          density_inventory_unit, density_provenance, preparation_state, \
          counts_as_aliases_json, notes, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT (household_id, product_id) DO UPDATE SET \
             edible_yield_percent = excluded.edible_yield_percent, \
             drained_quantity = excluded.drained_quantity, \
             drained_unit = excluded.drained_unit, \
             density_recipe_quantity = excluded.density_recipe_quantity, \
             density_recipe_unit = excluded.density_recipe_unit, \
             density_inventory_quantity = excluded.density_inventory_quantity, \
             density_inventory_unit = excluded.density_inventory_unit, \
             density_provenance = excluded.density_provenance, \
             preparation_state = excluded.preparation_state, \
             counts_as_aliases_json = excluded.counts_as_aliases_json, \
             notes = excluded.notes, \
             updated_at = excluded.updated_at",
    )
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(new.edible_yield_percent)
    .bind(new.drained_quantity)
    .bind(new.drained_unit)
    .bind(new.density_recipe_quantity)
    .bind(new.density_recipe_unit)
    .bind(new.density_inventory_quantity)
    .bind(new.density_inventory_unit)
    .bind(new.density_provenance)
    .bind(new.preparation_state)
    .bind(new.counts_as_aliases_json)
    .bind(new.notes)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_product_metadata(db, household_id, product_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_product_metadata(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
) -> Result<Option<ProductRecipeMetadataRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {PRODUCT_METADATA_COLS} \
         FROM product_recipe_metadata \
         WHERE household_id = ? AND product_id = ?"
    );
    let row = sqlx::query(&sql)
        .bind(household_id.to_string())
        .bind(product_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_product_metadata).transpose()
}

pub async fn list_availability(
    db: &Database,
    household_id: Uuid,
    ingredient_id: Uuid,
) -> Result<Vec<IngredientAvailabilityRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT \
             m.ingredient_id AS ingredient_id, m.id AS mapping_id, p.id AS product_id, \
             p.name AS product_name, l.id AS location_id, l.name AS location_name, \
             b.id AS batch_id, b.quantity AS quantity, b.unit AS unit, b.expires_on AS expires_on \
         FROM ingredient_product_mapping m \
         JOIN product p ON p.id = m.product_id \
         JOIN stock_batch b ON b.product_id = p.id AND b.household_id = m.household_id \
         JOIN location l ON l.id = b.location_id AND l.household_id = b.household_id \
         WHERE m.household_id = ? AND m.ingredient_id = ? AND b.depleted_at IS NULL \
         ORDER BY b.expires_on IS NULL ASC, b.expires_on ASC, p.name ASC",
    )
    .bind(household_id.to_string())
    .bind(ingredient_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_availability).collect()
}

fn row_uuid(row: &sqlx::any::AnyRow, name: &str) -> Result<Uuid, sqlx::Error> {
    let raw: String = row.try_get(name)?;
    Uuid::parse_str(&raw).map_err(|err| sqlx::Error::Decode(Box::new(err)))
}

fn row_bool(row: &sqlx::any::AnyRow, name: &str) -> Result<bool, sqlx::Error> {
    let value: i64 = row.try_get(name)?;
    Ok(value != 0)
}

fn row_to_ingredient(row: sqlx::any::AnyRow) -> Result<IngredientRow, sqlx::Error> {
    Ok(IngredientRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        display_name: row.try_get("display_name")?,
        category: row.try_get("category")?,
        default_family: row.try_get("default_family")?,
        aliases_json: row.try_get("aliases_json")?,
        dietary_tags_json: row.try_get("dietary_tags_json")?,
        allergen_tags_json: row.try_get("allergen_tags_json")?,
        notes: row.try_get("notes")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_mapping(row: sqlx::any::AnyRow) -> Result<IngredientProductMappingRow, sqlx::Error> {
    Ok(IngredientProductMappingRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        ingredient_id: row_uuid(&row, "ingredient_id")?,
        product_id: row_uuid(&row, "product_id")?,
        rank: row.try_get("rank")?,
        match_kind: row.try_get("match_kind")?,
        match_metadata_json: row.try_get("match_metadata_json")?,
        recipe_amount: row.try_get("recipe_amount")?,
        recipe_unit: row.try_get("recipe_unit")?,
        recipe_family: row.try_get("recipe_family")?,
        recipe_range_min: row.try_get("recipe_range_min")?,
        recipe_range_max: row.try_get("recipe_range_max")?,
        recipe_to_taste: row_bool(&row, "recipe_to_taste")?,
        recipe_preparation_note: row.try_get("recipe_preparation_note")?,
        inventory_amount: row.try_get("inventory_amount")?,
        inventory_unit: row.try_get("inventory_unit")?,
        inventory_family: row.try_get("inventory_family")?,
        inventory_range_min: row.try_get("inventory_range_min")?,
        inventory_range_max: row.try_get("inventory_range_max")?,
        inventory_to_taste: row_bool(&row, "inventory_to_taste")?,
        inventory_preparation_note: row.try_get("inventory_preparation_note")?,
        conversion_provenance: row.try_get("conversion_provenance")?,
        conversion_notes: row.try_get("conversion_notes")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_product_metadata(
    row: sqlx::any::AnyRow,
) -> Result<ProductRecipeMetadataRow, sqlx::Error> {
    Ok(ProductRecipeMetadataRow {
        household_id: row_uuid(&row, "household_id")?,
        product_id: row_uuid(&row, "product_id")?,
        edible_yield_percent: row.try_get("edible_yield_percent")?,
        drained_quantity: row.try_get("drained_quantity")?,
        drained_unit: row.try_get("drained_unit")?,
        density_recipe_quantity: row.try_get("density_recipe_quantity")?,
        density_recipe_unit: row.try_get("density_recipe_unit")?,
        density_inventory_quantity: row.try_get("density_inventory_quantity")?,
        density_inventory_unit: row.try_get("density_inventory_unit")?,
        density_provenance: row.try_get("density_provenance")?,
        preparation_state: row.try_get("preparation_state")?,
        counts_as_aliases_json: row.try_get("counts_as_aliases_json")?,
        notes: row.try_get("notes")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_availability(row: sqlx::any::AnyRow) -> Result<IngredientAvailabilityRow, sqlx::Error> {
    Ok(IngredientAvailabilityRow {
        ingredient_id: row_uuid(&row, "ingredient_id")?,
        mapping_id: row_uuid(&row, "mapping_id")?,
        product_id: row_uuid(&row, "product_id")?,
        product_name: row.try_get("product_name")?,
        location_id: row_uuid(&row, "location_id")?,
        location_name: row.try_get("location_name")?,
        batch_id: row_uuid(&row, "batch_id")?,
        quantity: row.try_get("quantity")?,
        unit: row.try_get("unit")?,
        expires_on: row.try_get("expires_on")?,
    })
}
