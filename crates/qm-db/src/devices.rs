use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone)]
pub struct DeviceUpsert {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub device_id: String,
    pub platform: String,
    pub push_token: Option<String>,
    pub push_authorization: String,
    pub app_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeviceRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub device_id: String,
    pub platform: String,
    pub push_token: Option<String>,
    pub push_authorization: String,
    pub app_version: Option<String>,
    pub last_seen_at: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn upsert(db: &Database, input: &DeviceUpsert) -> Result<DeviceRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    let updated = sqlx::query(
        "UPDATE notification_device \
         SET user_id = ?, platform = ?, push_token = ?, push_authorization = ?, \
             app_version = ?, last_seen_at = ?, updated_at = ? \
         WHERE session_id = ? AND device_id = ?",
    )
    .bind(input.user_id.to_string())
    .bind(&input.platform)
    .bind(&input.push_token)
    .bind(&input.push_authorization)
    .bind(&input.app_version)
    .bind(&now)
    .bind(&now)
    .bind(input.session_id.to_string())
    .bind(&input.device_id)
    .execute(&db.pool)
    .await?;

    if updated.rows_affected() == 0 {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO notification_device \
             (id, user_id, session_id, device_id, platform, push_token, push_authorization, \
              app_version, last_seen_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(input.user_id.to_string())
        .bind(input.session_id.to_string())
        .bind(&input.device_id)
        .bind(&input.platform)
        .bind(&input.push_token)
        .bind(&input.push_authorization)
        .bind(&input.app_version)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&db.pool)
        .await?;
    }

    find_by_session_device(db, input.session_id, &input.device_id)
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("device upsert did not persist row".into()))
}

pub async fn find_by_session_device(
    db: &Database,
    session_id: Uuid,
    device_id: &str,
) -> Result<Option<DeviceRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, session_id, device_id, platform, push_token, push_authorization, \
                app_version, last_seen_at, created_at, updated_at \
         FROM notification_device \
         WHERE session_id = ? AND device_id = ?",
    )
    .bind(session_id.to_string())
    .bind(device_id)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_device).transpose()
}

pub async fn find_latest_for_session(
    db: &Database,
    session_id: Uuid,
) -> Result<Option<DeviceRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, session_id, device_id, platform, push_token, push_authorization, \
                app_version, last_seen_at, created_at, updated_at \
         FROM notification_device \
         WHERE session_id = ? \
         ORDER BY updated_at DESC, id DESC",
    )
    .bind(session_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_device).transpose()
}

fn row_to_device(row: sqlx::any::AnyRow) -> Result<DeviceRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let user_id: String = row.try_get("user_id")?;
    let session_id: String = row.try_get("session_id")?;
    Ok(DeviceRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        session_id: Uuid::parse_str(&session_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        device_id: row.try_get("device_id")?,
        platform: row.try_get("platform")?,
        push_token: row.try_get("push_token")?,
        push_authorization: row.try_get("push_authorization")?,
        app_version: row.try_get("app_version")?,
        last_seen_at: row.try_get("last_seen_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
