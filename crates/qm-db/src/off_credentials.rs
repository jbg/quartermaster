use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct OffCredentialsRow {
    pub user_id: Uuid,
    pub off_username: String,
    pub encrypted_password: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get(db: &Database, user_id: Uuid) -> Result<Option<OffCredentialsRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT user_id, off_username, encrypted_password, created_at, updated_at \
         FROM off_credentials WHERE user_id = ?",
    )
    .bind(user_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_credentials).transpose()
}

pub async fn upsert(
    db: &Database,
    user_id: Uuid,
    off_username: &str,
    encrypted_password: &str,
) -> Result<OffCredentialsRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    if get(db, user_id).await?.is_some() {
        sqlx::query(
            "UPDATE off_credentials \
             SET off_username = ?, encrypted_password = ?, updated_at = ? \
             WHERE user_id = ?",
        )
        .bind(off_username)
        .bind(encrypted_password)
        .bind(&now)
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO off_credentials \
             (user_id, off_username, encrypted_password, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(user_id.to_string())
        .bind(off_username)
        .bind(encrypted_password)
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await?;
    }

    get(db, user_id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn delete(db: &Database, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM off_credentials WHERE user_id = ?")
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

fn row_to_credentials(row: sqlx::any::AnyRow) -> Result<OffCredentialsRow, sqlx::Error> {
    let user_id_str: String = row.try_get("user_id")?;
    Ok(OffCredentialsRow {
        user_id: Uuid::parse_str(&user_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        off_username: row.try_get("off_username")?,
        encrypted_password: row.try_get("encrypted_password")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
