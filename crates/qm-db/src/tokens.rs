use jiff::Timestamp;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

pub const KIND_ACCESS: &str = "access";
pub const KIND_REFRESH: &str = "refresh";

#[derive(Debug, Clone)]
pub struct TokenRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub kind: String,
    pub device_label: Option<String>,
    pub last_used_at: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

pub async fn create(
    db: &Database,
    user_id: Uuid,
    session_id: Uuid,
    token_hash: &str,
    kind: &str,
    device_label: Option<&str>,
    expires_at: Timestamp,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO auth_token \
         (id, user_id, session_id, token_hash, kind, device_label, last_used_at, expires_at, revoked_at, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(session_id.to_string())
    .bind(token_hash)
    .bind(kind)
    .bind(device_label)
    .bind(&now)
    .bind(crate::time::format_timestamp(expires_at))
    .bind(&now)
    .execute(&db.pool)
    .await?;
    Ok(id)
}

pub async fn find_active_by_hash(
    db: &Database,
    token_hash: &str,
) -> Result<Option<TokenRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, session_id, kind, device_label, last_used_at, expires_at, revoked_at \
         FROM auth_token \
         WHERE token_hash = ? AND revoked_at IS NULL",
    )
    .bind(token_hash)
    .fetch_optional(&db.pool)
    .await?;

    row.map(|r| {
        let id_str: String = r.try_get("id")?;
        let user_id_str: String = r.try_get("user_id")?;
        let session_id_str: String = r.try_get("session_id")?;
        Ok::<_, sqlx::Error>(TokenRow {
            id: Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            user_id: Uuid::parse_str(&user_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            session_id: Uuid::parse_str(&session_id_str)
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            kind: r.try_get("kind")?,
            device_label: r.try_get("device_label")?,
            last_used_at: r.try_get("last_used_at")?,
            expires_at: r.try_get("expires_at")?,
            revoked_at: r.try_get("revoked_at")?,
        })
    })
    .transpose()
}

pub async fn touch_last_used(db: &Database, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE auth_token SET last_used_at = ? WHERE id = ?")
        .bind(now_utc_rfc3339())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn revoke(db: &Database, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE auth_token SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
        .bind(now_utc_rfc3339())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn revoke_by_hash(db: &Database, token_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE auth_token SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
        .bind(now_utc_rfc3339())
        .bind(token_hash)
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn revoke_session(db: &Database, session_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE auth_token SET revoked_at = ? WHERE session_id = ? AND revoked_at IS NULL")
        .bind(now_utc_rfc3339())
        .bind(session_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}
