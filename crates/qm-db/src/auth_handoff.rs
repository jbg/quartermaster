use jiff::Timestamp;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, time, Database};

#[derive(Debug, Clone)]
pub struct AuthHandoffRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source_session_id: Uuid,
    pub active_household_id: Option<Uuid>,
    pub target_device_label: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub accepted_session_id: Option<Uuid>,
}

pub async fn create(
    db: &Database,
    user_id: Uuid,
    source_session_id: Uuid,
    active_household_id: Option<Uuid>,
    target_device_label: Option<&str>,
    token_hash: &str,
    expires_at: Timestamp,
) -> Result<AuthHandoffRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let expires_at = time::format_timestamp(expires_at);
    sqlx::query(
        "INSERT INTO auth_handoff_request \
         (id, user_id, source_session_id, active_household_id, target_device_label, token_hash, \
          created_at, expires_at, consumed_at, cancelled_at, accepted_session_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL)",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(source_session_id.to_string())
    .bind(active_household_id.map(|id| id.to_string()))
    .bind(target_device_label)
    .bind(token_hash)
    .bind(&now)
    .bind(&expires_at)
    .execute(&db.pool)
    .await?;
    Ok(AuthHandoffRow {
        id,
        user_id,
        source_session_id,
        active_household_id,
        target_device_label: target_device_label.map(str::to_owned),
        created_at: now,
        expires_at,
        consumed_at: None,
        cancelled_at: None,
        accepted_session_id: None,
    })
}

pub async fn get_for_user(
    db: &Database,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<AuthHandoffRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, source_session_id, active_household_id, target_device_label, \
                created_at, expires_at, consumed_at, cancelled_at, accepted_session_id \
         FROM auth_handoff_request WHERE id = ? AND user_id = ?",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_handoff).transpose()
}

pub async fn find_by_token_hash(
    db: &Database,
    id: Uuid,
    token_hash: &str,
) -> Result<Option<AuthHandoffRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, source_session_id, active_household_id, target_device_label, \
                created_at, expires_at, consumed_at, cancelled_at, accepted_session_id \
         FROM auth_handoff_request WHERE id = ? AND token_hash = ?",
    )
    .bind(id.to_string())
    .bind(token_hash)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_handoff).transpose()
}

pub async fn cancel(
    db: &Database,
    id: Uuid,
    user_id: Uuid,
    source_session_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE auth_handoff_request SET cancelled_at = ? \
         WHERE id = ? AND user_id = ? AND source_session_id = ? \
           AND consumed_at IS NULL AND cancelled_at IS NULL",
    )
    .bind(now_utc_rfc3339())
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(source_session_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn consume(
    db: &Database,
    id: Uuid,
    token_hash: &str,
    accepted_session_id: Uuid,
    now: Timestamp,
) -> Result<Option<AuthHandoffRow>, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, user_id, source_session_id, active_household_id, target_device_label, \
                created_at, expires_at, consumed_at, cancelled_at, accepted_session_id \
         FROM auth_handoff_request WHERE id = ? AND token_hash = ?",
    )
    .bind(id.to_string())
    .bind(token_hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let handoff = row_to_handoff(row)?;
    if handoff.consumed_at.is_some() || handoff.cancelled_at.is_some() {
        tx.commit().await?;
        return Ok(None);
    }
    let expires_at =
        time::parse_timestamp(&handoff.expires_at).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    if expires_at <= now {
        tx.commit().await?;
        return Ok(None);
    }
    let consumed_at = time::format_timestamp(now);
    let updated = sqlx::query(
        "UPDATE auth_handoff_request \
         SET consumed_at = ?, accepted_session_id = ? \
         WHERE id = ? AND token_hash = ? AND consumed_at IS NULL AND cancelled_at IS NULL",
    )
    .bind(&consumed_at)
    .bind(accepted_session_id.to_string())
    .bind(id.to_string())
    .bind(token_hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if updated.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(AuthHandoffRow {
        consumed_at: Some(consumed_at),
        accepted_session_id: Some(accepted_session_id),
        ..handoff
    }))
}

fn row_to_handoff(row: sqlx::any::AnyRow) -> Result<AuthHandoffRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let user_id: String = row.try_get("user_id")?;
    let source_session_id: String = row.try_get("source_session_id")?;
    let active_household_id: Option<String> = row.try_get("active_household_id")?;
    let accepted_session_id: Option<String> = row.try_get("accepted_session_id")?;
    Ok(AuthHandoffRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        source_session_id: Uuid::parse_str(&source_session_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        active_household_id: active_household_id
            .map(|id| Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e))))
            .transpose()?,
        target_device_label: row.try_get("target_device_label")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        consumed_at: row.try_get("consumed_at")?,
        cancelled_at: row.try_get("cancelled_at")?,
        accepted_session_id: accepted_session_id
            .map(|id| Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e))))
            .transpose()?,
    })
}
