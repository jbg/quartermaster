use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone)]
pub struct AuthSessionRow {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub active_household_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn find(
    db: &Database,
    session_id: Uuid,
) -> Result<Option<AuthSessionRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT session_id, user_id, active_household_id, created_at, updated_at \
         FROM auth_session WHERE session_id = ?",
    )
    .bind(session_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_auth_session).transpose()
}

pub async fn upsert(
    db: &Database,
    session_id: Uuid,
    user_id: Uuid,
    active_household_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    let now = now_utc_rfc3339();
    let updated = sqlx::query(
        "UPDATE auth_session \
         SET user_id = ?, active_household_id = ?, updated_at = ? \
         WHERE session_id = ?",
    )
    .bind(user_id.to_string())
    .bind(active_household_id.map(|id| id.to_string()))
    .bind(&now)
    .bind(session_id.to_string())
    .execute(&db.pool)
    .await?;

    if updated.rows_affected() > 0 {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO auth_session (session_id, user_id, active_household_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(session_id.to_string())
    .bind(user_id.to_string())
    .bind(active_household_id.map(|id| id.to_string()))
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn delete(db: &Database, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM auth_session WHERE session_id = ?")
        .bind(session_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

fn row_to_auth_session(row: sqlx::any::AnyRow) -> Result<AuthSessionRow, sqlx::Error> {
    let session_id: String = row.try_get("session_id")?;
    let user_id: String = row.try_get("user_id")?;
    let active_household_id: Option<String> = row.try_get("active_household_id")?;
    Ok(AuthSessionRow {
        session_id: Uuid::parse_str(&session_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        active_household_id: active_household_id
            .map(|id| Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e))))
            .transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
