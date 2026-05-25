use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{audited_sql, now_utc_rfc3339, Database};

const COLS: &str = "id, household_id, created_by, source, status, recipe_id, recipe_version_id, \
                    ai_task_id, title, summary, score, score_breakdown_json, missing_json, \
                    pantry_items_json, generated_recipe_json, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct PantrySuggestionRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub created_by: Option<Uuid>,
    pub source: String,
    pub status: String,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub ai_task_id: Option<Uuid>,
    pub title: String,
    pub summary: Option<String>,
    pub score: i64,
    pub score_breakdown_json: String,
    pub missing_json: String,
    pub pantry_items_json: String,
    pub generated_recipe_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewPantrySuggestion<'a> {
    pub created_by: Option<Uuid>,
    pub source: &'a str,
    pub status: &'a str,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub ai_task_id: Option<Uuid>,
    pub title: &'a str,
    pub summary: Option<&'a str>,
    pub score: i64,
    pub score_breakdown_json: &'a str,
    pub missing_json: &'a str,
    pub pantry_items_json: &'a str,
    pub generated_recipe_json: Option<&'a str>,
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    new: &NewPantrySuggestion<'_>,
) -> Result<PantrySuggestionRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO pantry_suggestion \
         (id, household_id, created_by, source, status, recipe_id, recipe_version_id, \
          ai_task_id, title, summary, score, score_breakdown_json, missing_json, \
          pantry_items_json, generated_recipe_json, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.created_by.map(|id| id.to_string()))
    .bind(new.source)
    .bind(new.status)
    .bind(new.recipe_id.map(|id| id.to_string()))
    .bind(new.recipe_version_id.map(|id| id.to_string()))
    .bind(new.ai_task_id.map(|id| id.to_string()))
    .bind(new.title)
    .bind(new.summary)
    .bind(new.score)
    .bind(new.score_breakdown_json)
    .bind(new.missing_json)
    .bind(new.pantry_items_json)
    .bind(new.generated_recipe_json)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list(
    db: &Database,
    household_id: Uuid,
    limit: i64,
) -> Result<Vec<PantrySuggestionRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {COLS} FROM pantry_suggestion \
         WHERE household_id = ? \
         ORDER BY created_at DESC, id DESC \
         LIMIT ?"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(limit)
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_suggestion).collect()
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<PantrySuggestionRow>, sqlx::Error> {
    let sql = format!("SELECT {COLS} FROM pantry_suggestion WHERE household_id = ? AND id = ?");
    let row = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_suggestion).transpose()
}

pub async fn update_status(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    status: &str,
) -> Result<Option<PantrySuggestionRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let res = sqlx::query(
        "UPDATE pantry_suggestion SET status = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(status)
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find(db, household_id, id).await
}

fn row_to_suggestion(row: sqlx::any::AnyRow) -> Result<PantrySuggestionRow, sqlx::Error> {
    Ok(PantrySuggestionRow {
        id: row_uuid(&row, "id")?,
        household_id: row_uuid(&row, "household_id")?,
        created_by: optional_row_uuid(&row, "created_by")?,
        source: row.get("source"),
        status: row.get("status"),
        recipe_id: optional_row_uuid(&row, "recipe_id")?,
        recipe_version_id: optional_row_uuid(&row, "recipe_version_id")?,
        ai_task_id: optional_row_uuid(&row, "ai_task_id")?,
        title: row.get("title"),
        summary: row.try_get("summary")?,
        score: row.get("score"),
        score_breakdown_json: row.get("score_breakdown_json"),
        missing_json: row.get("missing_json"),
        pantry_items_json: row.get("pantry_items_json"),
        generated_recipe_json: row.try_get("generated_recipe_json")?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_uuid(row: &sqlx::any::AnyRow, column: &str) -> Result<Uuid, sqlx::Error> {
    Uuid::parse_str(row.get::<&str, _>(column)).map_err(|err| sqlx::Error::Decode(Box::new(err)))
}

fn optional_row_uuid(row: &sqlx::any::AnyRow, column: &str) -> Result<Option<Uuid>, sqlx::Error> {
    row.try_get::<Option<&str>, _>(column)?
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|err| sqlx::Error::Decode(Box::new(err)))
}
