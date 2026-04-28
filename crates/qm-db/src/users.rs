use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub email_verified_at: Option<String>,
    pub password_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingEmailVerificationRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub expires_at: String,
    pub created_at: String,
}

pub async fn create(
    db: &Database,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
) -> Result<UserRow, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let user = create_in_tx(&mut tx, username, email, password_hash).await?;
    tx.commit().await?;
    Ok(user)
}

pub async fn create_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
) -> Result<UserRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, created_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(&created_at)
    .execute(&mut **tx)
    .await?;

    Ok(UserRow {
        id,
        username: username.to_owned(),
        email: email.map(str::to_owned),
        email_verified_at: None,
        password_hash: password_hash.to_owned(),
        created_at,
    })
}

pub async fn find_by_username(
    db: &Database,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, username, email, email_verified_at, password_hash, created_at FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(&db.pool)
    .await?;

    row.map(row_to_user).transpose()
}

pub async fn find_by_id(db: &Database, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, username, email, email_verified_at, password_hash, created_at FROM users WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;

    row.map(row_to_user).transpose()
}

pub async fn count(db: &Database) -> Result<i64, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM users")
        .fetch_one(&db.pool)
        .await?;
    row.try_get::<i64, _>("n")
}

pub async fn create_email_verification(
    db: &Database,
    user_id: Uuid,
    email: &str,
    code_hash: &str,
    expires_at: &str,
) -> Result<PendingEmailVerificationRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO user_email_verification \
         (id, user_id, email, code_hash, expires_at, consumed_at, created_at) \
         VALUES (?, ?, ?, ?, ?, NULL, ?)",
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(email)
    .bind(code_hash)
    .bind(expires_at)
    .bind(&created_at)
    .execute(&db.pool)
    .await?;

    Ok(PendingEmailVerificationRow {
        id,
        user_id,
        email: email.to_owned(),
        expires_at: expires_at.to_owned(),
        created_at,
    })
}

pub async fn latest_pending_email_verification(
    db: &Database,
    user_id: Uuid,
    now: &str,
) -> Result<Option<PendingEmailVerificationRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, user_id, email, expires_at, created_at \
         FROM user_email_verification \
         WHERE user_id = ? AND consumed_at IS NULL AND expires_at >= ? \
         ORDER BY created_at DESC, id DESC \
         LIMIT 1",
    )
    .bind(user_id.to_string())
    .bind(now)
    .fetch_optional(&db.pool)
    .await?;

    row.map(row_to_pending_email_verification).transpose()
}

pub async fn confirm_email_verification(
    db: &Database,
    user_id: Uuid,
    code_hash: &str,
    now: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, user_id, email, expires_at, created_at \
         FROM user_email_verification \
         WHERE user_id = ? AND code_hash = ? AND consumed_at IS NULL AND expires_at >= ? \
         ORDER BY created_at DESC, id DESC \
         LIMIT 1",
    )
    .bind(user_id.to_string())
    .bind(code_hash)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(pending) = row.map(row_to_pending_email_verification).transpose()? else {
        tx.commit().await?;
        return Ok(None);
    };

    sqlx::query("UPDATE users SET email = ?, email_verified_at = ? WHERE id = ?")
        .bind(&pending.email)
        .bind(now)
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE user_email_verification \
         SET consumed_at = ? \
         WHERE user_id = ? AND consumed_at IS NULL",
    )
    .bind(now)
    .bind(user_id.to_string())
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query(
        "SELECT id, username, email, email_verified_at, password_hash, created_at \
         FROM users WHERE id = ?",
    )
    .bind(user_id.to_string())
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    row.map(row_to_user).transpose()
}

pub async fn clear_recovery_email(db: &Database, user_id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let now = now_utc_rfc3339();
    sqlx::query("UPDATE users SET email = NULL, email_verified_at = NULL WHERE id = ?")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE user_email_verification \
         SET consumed_at = ? \
         WHERE user_id = ? AND consumed_at IS NULL",
    )
    .bind(now)
    .bind(user_id.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

fn row_to_user(row: sqlx::any::AnyRow) -> Result<UserRow, sqlx::Error> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    Ok(UserRow {
        id,
        username: row.try_get("username")?,
        email: row.try_get("email")?,
        email_verified_at: row.try_get("email_verified_at")?,
        password_hash: row.try_get("password_hash")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_pending_email_verification(
    row: sqlx::any::AnyRow,
) -> Result<PendingEmailVerificationRow, sqlx::Error> {
    let id_str: String = row.try_get("id")?;
    let user_id_str: String = row.try_get("user_id")?;
    Ok(PendingEmailVerificationRow {
        id: Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: Uuid::parse_str(&user_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        email: row.try_get("email")?,
        expires_at: row.try_get("expires_at")?,
        created_at: row.try_get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_find() {
        let db = crate::test_db().await;
        let u = create(&db, "alice", Some("a@example.com"), "hash")
            .await
            .unwrap();
        assert_eq!(u.username, "alice");

        let by_name = find_by_username(&db, "alice").await.unwrap().unwrap();
        assert_eq!(by_name.id, u.id);

        let by_id = find_by_id(&db, u.id).await.unwrap().unwrap();
        assert_eq!(by_id.username, "alice");

        assert_eq!(count(&db).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn find_missing_returns_none() {
        let db = crate::test_db().await;
        assert!(find_by_username(&db, "nobody").await.unwrap().is_none());
    }
}
