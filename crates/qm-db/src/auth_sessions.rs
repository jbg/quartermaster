use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

pub const STALE_SESSION_SWEEP_BATCH_SIZE: u32 = 100;

#[derive(Debug, Clone)]
pub struct AuthSessionRow {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub active_household_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn find(db: &Database, session_id: Uuid) -> Result<Option<AuthSessionRow>, sqlx::Error> {
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

pub async fn has_live_tokens(
    db: &Database,
    session_id: Uuid,
    now_rfc3339: &str,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS live_count FROM auth_token \
         WHERE session_id = ? AND revoked_at IS NULL AND expires_at > ?",
    )
    .bind(session_id.to_string())
    .bind(now_rfc3339)
    .fetch_one(&db.pool)
    .await?;
    let live_count: i64 = row.try_get("live_count")?;
    Ok(live_count > 0)
}

pub async fn delete_if_no_live_tokens(
    db: &Database,
    session_id: Uuid,
    now_rfc3339: &str,
) -> Result<bool, sqlx::Error> {
    if has_live_tokens(db, session_id, now_rfc3339).await? {
        return Ok(false);
    }

    let deleted = sqlx::query("DELETE FROM auth_session WHERE session_id = ?")
        .bind(session_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

pub async fn delete_stale_session_batch(
    db: &Database,
    now_rfc3339: &str,
    batch_size: u32,
) -> Result<u64, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let candidate_rows = sqlx::query(
        "SELECT s.session_id \
         FROM auth_session s \
         WHERE NOT EXISTS ( \
             SELECT 1 FROM auth_token t \
             WHERE t.session_id = s.session_id \
               AND t.revoked_at IS NULL \
               AND t.expires_at > ? \
         ) \
         ORDER BY s.updated_at ASC, s.session_id ASC \
         LIMIT ?",
    )
    .bind(now_rfc3339)
    .bind(i64::from(batch_size))
    .fetch_all(&mut *tx)
    .await?;

    let mut deleted = 0;
    for row in candidate_rows {
        let session_id: String = row.try_get("session_id")?;
        deleted += sqlx::query(
            "DELETE FROM auth_session \
             WHERE session_id = ? \
               AND NOT EXISTS ( \
                   SELECT 1 FROM auth_token t \
                   WHERE t.session_id = ? \
                     AND t.revoked_at IS NULL \
                     AND t.expires_at > ? \
               )",
        )
        .bind(&session_id)
        .bind(&session_id)
        .bind(now_rfc3339)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }

    tx.commit().await?;
    Ok(deleted)
}

pub async fn delete_stale_sessions(
    db: &Database,
    now_rfc3339: &str,
    batch_size: u32,
) -> Result<u64, sqlx::Error> {
    let mut total_deleted = 0;
    loop {
        let deleted = delete_stale_session_batch(db, now_rfc3339, batch_size).await?;
        total_deleted += deleted;
        if deleted < u64::from(batch_size) {
            return Ok(total_deleted);
        }
    }
}

fn row_to_auth_session(row: sqlx::any::AnyRow) -> Result<AuthSessionRow, sqlx::Error> {
    let session_id: String = row.try_get("session_id")?;
    let user_id: String = row.try_get("user_id")?;
    let active_household_id: Option<String> = row.try_get("active_household_id")?;
    Ok(AuthSessionRow {
        session_id: Uuid::parse_str(&session_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: Uuid::parse_str(&user_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        active_household_id: active_household_id
            .map(|id| Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e))))
            .transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;
    use crate::{test_support, tokens, users};

    async fn seed_session_with_token(
        db: &Database,
        username: &str,
        session_id: Uuid,
        expires_at: chrono::DateTime<Utc>,
        revoked_at: Option<&str>,
    ) {
        let user = users::create(
            db,
            username,
            Some(&format!("{username}@example.com")),
            "hash",
        )
        .await
        .unwrap();
        upsert(db, session_id, user.id, None).await.unwrap();
        let token_id = tokens::create(
            db,
            user.id,
            session_id,
            &format!("hash-{username}"),
            tokens::KIND_ACCESS,
            Some("iPhone"),
            expires_at,
        )
        .await
        .unwrap();
        if let Some(revoked_at) = revoked_at {
            sqlx::query("UPDATE auth_token SET revoked_at = ? WHERE id = ?")
                .bind(revoked_at)
                .bind(token_id.to_string())
                .execute(&db.pool)
                .await
                .unwrap();
        }
    }

    async fn assert_delete_stale_sessions_on_backend(test_db: test_support::TestDatabase) {
        let db = test_db.db();
        let now = Utc::now();
        let stale_a = Uuid::now_v7();
        let stale_b = Uuid::now_v7();
        let live = Uuid::now_v7();

        seed_session_with_token(db, "stale-a", stale_a, now - Duration::minutes(5), None).await;
        seed_session_with_token(
            db,
            "stale-b",
            stale_b,
            now + Duration::minutes(5),
            Some(&(now - Duration::minutes(1)).to_rfc3339()),
        )
        .await;
        seed_session_with_token(db, "live", live, now + Duration::minutes(30), None).await;

        let deleted = delete_stale_sessions(db, &now.to_rfc3339(), 1)
            .await
            .unwrap();
        assert_eq!(deleted, 2);
        assert!(find(db, stale_a).await.unwrap().is_none());
        assert!(find(db, stale_b).await.unwrap().is_none());
        assert!(find(db, live).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn delete_stale_sessions_removes_only_dead_sessions_in_batches() {
        assert_delete_stale_sessions_on_backend(test_support::sqlite().await).await;
    }

    #[tokio::test]
    async fn delete_stale_sessions_removes_only_dead_sessions_in_batches_postgres() {
        let Some(test_db) = test_support::postgres().await else {
            return;
        };
        assert_delete_stale_sessions_on_backend(test_db).await;
    }
}
