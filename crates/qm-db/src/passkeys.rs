use jiff::Timestamp;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, time, Database};

pub const CEREMONY_REGISTRATION: &str = "registration";
pub const CEREMONY_AUTHENTICATION: &str = "authentication";

#[derive(Debug, Clone)]
pub struct PasskeyCredentialRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub credential_id: String,
    pub label: Option<String>,
    pub passkey_json: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PasskeyCeremonyRow {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub kind: String,
    pub state_json: String,
    pub created_at: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
}

pub async fn list_credentials(
    db: &Database,
    user_id: Uuid,
) -> Result<Vec<PasskeyCredentialRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, user_id, credential_id, label, passkey_json, created_at, last_used_at \
         FROM passkey_credential WHERE user_id = ? ORDER BY created_at DESC, id DESC",
    )
    .bind(user_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_credential).collect()
}

pub async fn get_credential(
    db: &Database,
    id: Uuid,
    user_id: Uuid,
) -> Result<Option<PasskeyCredentialRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, credential_id, label, passkey_json, created_at, last_used_at \
         FROM passkey_credential WHERE id = ? AND user_id = ?",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_credential).transpose()
}

pub async fn find_credential_by_credential_id(
    db: &Database,
    credential_id: &str,
) -> Result<Option<PasskeyCredentialRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, credential_id, label, passkey_json, created_at, last_used_at \
         FROM passkey_credential WHERE credential_id = ?",
    )
    .bind(credential_id)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_credential).transpose()
}

pub async fn insert_credential(
    db: &Database,
    user_id: Uuid,
    credential_id: &str,
    label: Option<&str>,
    passkey_json: &str,
) -> Result<PasskeyCredentialRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO passkey_credential \
         (id, user_id, credential_id, label, passkey_json, created_at, last_used_at) \
         VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(credential_id)
    .bind(label)
    .bind(passkey_json)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    Ok(PasskeyCredentialRow {
        id,
        user_id,
        credential_id: credential_id.to_owned(),
        label: label.map(str::to_owned),
        passkey_json: passkey_json.to_owned(),
        created_at: now,
        last_used_at: None,
    })
}

pub async fn update_credential_after_auth(
    db: &Database,
    id: Uuid,
    passkey_json: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE passkey_credential SET passkey_json = ?, last_used_at = ? WHERE id = ?")
        .bind(passkey_json)
        .bind(now_utc_rfc3339())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn delete_credential(
    db: &Database,
    id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM passkey_credential WHERE id = ? AND user_id = ?")
        .bind(id.to_string())
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn create_ceremony(
    db: &Database,
    user_id: Option<Uuid>,
    kind: &str,
    state_json: &str,
    expires_at: Timestamp,
) -> Result<PasskeyCeremonyRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let expires_at = time::format_timestamp(expires_at);
    sqlx::query(
        "INSERT INTO passkey_ceremony \
         (id, user_id, kind, state_json, created_at, expires_at, consumed_at) \
         VALUES (?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(id.to_string())
    .bind(user_id.map(|id| id.to_string()))
    .bind(kind)
    .bind(state_json)
    .bind(&now)
    .bind(&expires_at)
    .execute(&db.pool)
    .await?;
    Ok(PasskeyCeremonyRow {
        id,
        user_id,
        kind: kind.to_owned(),
        state_json: state_json.to_owned(),
        created_at: now,
        expires_at,
        consumed_at: None,
    })
}

pub async fn consume_ceremony(
    db: &Database,
    id: Uuid,
    kind: &str,
    user_id: Option<Uuid>,
    now: Timestamp,
) -> Result<Option<PasskeyCeremonyRow>, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, user_id, kind, state_json, created_at, expires_at, consumed_at \
         FROM passkey_ceremony WHERE id = ? AND kind = ?",
    )
    .bind(id.to_string())
    .bind(kind)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let ceremony = row_to_ceremony(row)?;
    if ceremony.consumed_at.is_some() {
        tx.commit().await?;
        return Ok(None);
    }
    if let Some(expected_user_id) = user_id {
        if ceremony.user_id != Some(expected_user_id) {
            tx.commit().await?;
            return Ok(None);
        }
    } else if kind == CEREMONY_REGISTRATION && ceremony.user_id.is_some() {
        tx.commit().await?;
        return Ok(None);
    }
    let expires_at = time::parse_timestamp(&ceremony.expires_at)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    if expires_at <= now {
        tx.commit().await?;
        return Ok(None);
    }
    sqlx::query("UPDATE passkey_ceremony SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL")
        .bind(time::format_timestamp(now))
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Some(ceremony))
}

fn row_to_credential(row: sqlx::any::AnyRow) -> Result<PasskeyCredentialRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let user_id: String = row.try_get("user_id")?;
    Ok(PasskeyCredentialRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        credential_id: row.try_get("credential_id")?,
        label: row.try_get("label")?,
        passkey_json: row.try_get("passkey_json")?,
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
    })
}

fn row_to_ceremony(row: sqlx::any::AnyRow) -> Result<PasskeyCeremonyRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let user_id: Option<String> = row.try_get("user_id")?;
    Ok(PasskeyCeremonyRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: user_id
            .map(|id| Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e))))
            .transpose()?,
        kind: row.try_get("kind")?,
        state_json: row.try_get("state_json")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        consumed_at: row.try_get("consumed_at")?,
    })
}
