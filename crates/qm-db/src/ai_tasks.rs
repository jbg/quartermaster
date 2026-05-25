use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{audited_sql, now_utc_rfc3339, Database};

const AI_TASK_COLS: &str = "id, household_id, created_by, task_type, provider, model, \
                            prompt_version, input_digest, input_summary_json, output_json, \
                            validation_status, validation_errors_json, user_state, \
                            credentials_assertion, raw_response_json, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct AiTaskRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub created_by: Option<Uuid>,
    pub task_type: String,
    pub provider: String,
    pub model: Option<String>,
    pub prompt_version: String,
    pub input_digest: String,
    pub input_summary_json: String,
    pub output_json: Option<String>,
    pub validation_status: String,
    pub validation_errors_json: String,
    pub user_state: String,
    pub credentials_assertion: bool,
    pub raw_response_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewAiTask<'a> {
    pub created_by: Option<Uuid>,
    pub task_type: &'a str,
    pub provider: &'a str,
    pub model: Option<&'a str>,
    pub prompt_version: &'a str,
    pub input_digest: &'a str,
    pub input_summary_json: &'a str,
    pub output_json: Option<&'a str>,
    pub validation_status: &'a str,
    pub validation_errors_json: &'a str,
    pub user_state: &'a str,
    pub credentials_assertion: bool,
    pub raw_response_json: Option<&'a str>,
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    new: &NewAiTask<'_>,
) -> Result<AiTaskRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO ai_task ( \
            id, household_id, created_by, task_type, provider, model, prompt_version, \
            input_digest, input_summary_json, output_json, validation_status, \
            validation_errors_json, user_state, credentials_assertion, raw_response_json, \
            created_at, updated_at \
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.created_by.map(|id| id.to_string()))
    .bind(new.task_type)
    .bind(new.provider)
    .bind(new.model)
    .bind(new.prompt_version)
    .bind(new.input_digest)
    .bind(new.input_summary_json)
    .bind(new.output_json)
    .bind(new.validation_status)
    .bind(new.validation_errors_json)
    .bind(new.user_state)
    .bind(if new.credentials_assertion {
        1_i64
    } else {
        0_i64
    })
    .bind(new.raw_response_json)
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
) -> Result<Vec<AiTaskRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {AI_TASK_COLS} FROM ai_task \
         WHERE household_id = ? \
         ORDER BY created_at DESC, id DESC \
         LIMIT ?"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(limit)
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_ai_task).collect()
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<AiTaskRow>, sqlx::Error> {
    let sql = format!("SELECT {AI_TASK_COLS} FROM ai_task WHERE household_id = ? AND id = ?");
    let row = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_ai_task).transpose()
}

pub async fn update_user_state(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    user_state: &str,
) -> Result<Option<AiTaskRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let result = sqlx::query(
        "UPDATE ai_task SET user_state = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(user_state)
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    find(db, household_id, id).await
}

fn row_to_ai_task(row: sqlx::any::AnyRow) -> Result<AiTaskRow, sqlx::Error> {
    Ok(AiTaskRow {
        id: Uuid::parse_str(row.get::<&str, _>("id"))
            .map_err(|err| sqlx::Error::Decode(Box::new(err)))?,
        household_id: Uuid::parse_str(row.get::<&str, _>("household_id"))
            .map_err(|err| sqlx::Error::Decode(Box::new(err)))?,
        created_by: row
            .try_get::<Option<&str>, _>("created_by")?
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|err| sqlx::Error::Decode(Box::new(err)))?,
        task_type: row.get("task_type"),
        provider: row.get("provider"),
        model: row.try_get("model")?,
        prompt_version: row.get("prompt_version"),
        input_digest: row.get("input_digest"),
        input_summary_json: row.get("input_summary_json"),
        output_json: row.try_get("output_json")?,
        validation_status: row.get("validation_status"),
        validation_errors_json: row.get("validation_errors_json"),
        user_state: row.get("user_state"),
        credentials_assertion: row.get::<i64, _>("credentials_assertion") != 0,
        raw_response_json: row.try_get("raw_response_json")?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
