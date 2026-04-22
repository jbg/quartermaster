use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
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
        password_hash: password_hash.to_owned(),
        created_at,
    })
}

pub async fn find_by_username(
    db: &Database,
    username: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, username, email, password_hash, created_at FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(&db.pool)
    .await?;

    row.map(row_to_user).transpose()
}

pub async fn find_by_id(db: &Database, id: Uuid) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, username, email, password_hash, created_at FROM users WHERE id = ?",
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

fn row_to_user(row: sqlx::any::AnyRow) -> Result<UserRow, sqlx::Error> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    Ok(UserRow {
        id,
        username: row.try_get("username")?,
        email: row.try_get("email")?,
        password_hash: row.try_get("password_hash")?,
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
