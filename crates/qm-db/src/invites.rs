use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct InviteRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub code: String,
    pub created_by: Uuid,
    pub expires_at: String,
    pub max_uses: i64,
    pub use_count: i64,
    pub role_granted: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteStatus {
    Active,
    Exhausted,
    Expired,
    Revoked,
    NotFound,
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    code: &str,
    created_by: Uuid,
    expires_at: &str,
    max_uses: i64,
    role_granted: &str,
) -> Result<InviteRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO invite \
         (id, household_id, code, created_by, expires_at, max_uses, use_count, role_granted, created_at, revoked_at) \
         VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, NULL)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(code)
    .bind(created_by.to_string())
    .bind(expires_at)
    .bind(max_uses)
    .bind(role_granted)
    .bind(&created_at)
    .execute(&db.pool)
    .await?;

    Ok(InviteRow {
        id,
        household_id,
        code: code.to_owned(),
        created_by,
        expires_at: expires_at.to_owned(),
        max_uses,
        use_count: 0,
        role_granted: role_granted.to_owned(),
        created_at,
        revoked_at: None,
    })
}

pub async fn list_for_household(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<InviteRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, household_id, code, created_by, expires_at, max_uses, use_count, role_granted, created_at, revoked_at \
         FROM invite \
         WHERE household_id = ? AND revoked_at IS NULL \
         ORDER BY created_at DESC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_invite).collect()
}

pub async fn find_by_id(
    db: &Database,
    id: Uuid,
) -> Result<Option<InviteRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, code, created_by, expires_at, max_uses, use_count, role_granted, created_at, revoked_at \
         FROM invite WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_invite).transpose()
}

pub async fn find_by_code(
    db: &Database,
    code: &str,
) -> Result<Option<InviteRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, code, created_by, expires_at, max_uses, use_count, role_granted, created_at, revoked_at \
         FROM invite WHERE code = ?",
    )
    .bind(code)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_invite).transpose()
}

pub async fn revoke(
    db: &Database,
    id: Uuid,
    household_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE invite SET revoked_at = ? \
         WHERE id = ? AND household_id = ? AND revoked_at IS NULL",
    )
    .bind(now_utc_rfc3339())
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn status_for_code(
    db: &Database,
    code: &str,
) -> Result<InviteStatus, sqlx::Error> {
    let Some(invite) = find_by_code(db, code).await? else {
        return Ok(InviteStatus::NotFound);
    };
    Ok(classify(&invite))
}

pub async fn consume(
    db: &Database,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let now = now_utc_rfc3339();
    let res = sqlx::query(
        "UPDATE invite SET use_count = use_count + 1 \
         WHERE id = ? AND revoked_at IS NULL AND expires_at > ? AND use_count < max_uses",
    )
    .bind(id.to_string())
    .bind(now)
    .execute(&db.pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

pub fn classify(invite: &InviteRow) -> InviteStatus {
    if invite.revoked_at.is_some() {
        InviteStatus::Revoked
    } else if invite.use_count >= invite.max_uses {
        InviteStatus::Exhausted
    } else if invite.expires_at <= now_utc_rfc3339() {
        InviteStatus::Expired
    } else {
        InviteStatus::Active
    }
}

fn row_to_invite(row: sqlx::any::AnyRow) -> Result<InviteRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let household_id: String = row.try_get("household_id")?;
    let created_by: String = row.try_get("created_by")?;
    Ok(InviteRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&household_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        code: row.try_get("code")?,
        created_by: Uuid::parse_str(&created_by).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        expires_at: row.try_get("expires_at")?,
        max_uses: row.try_get("max_uses")?,
        use_count: row.try_get("use_count")?,
        role_granted: row.try_get("role_granted")?,
        created_at: row.try_get("created_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{households, users};

    #[tokio::test]
    async fn create_list_revoke_and_consume() {
        let db = crate::test_db().await;
        let household = households::create(&db, "Home").await.unwrap();
        let creator = users::create(&db, "alice", None, "hash").await.unwrap();
        let invite = create(
            &db,
            household.id,
            "ABC123",
            creator.id,
            "2999-01-01T00:00:00.000Z",
            2,
            "member",
        )
        .await
        .unwrap();
        assert_eq!(status_for_code(&db, "ABC123").await.unwrap(), InviteStatus::Active);
        assert!(consume(&db, invite.id).await.unwrap());
        assert!(consume(&db, invite.id).await.unwrap());
        assert!(!consume(&db, invite.id).await.unwrap());
        assert_eq!(status_for_code(&db, "ABC123").await.unwrap(), InviteStatus::Exhausted);
        assert_eq!(list_for_household(&db, household.id).await.unwrap().len(), 1);
        assert!(revoke(&db, invite.id, household.id).await.unwrap());
    }
}
