use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct HouseholdRow {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
}

pub async fn create(db: &Database, name: &str) -> Result<HouseholdRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query("INSERT INTO household (id, name, created_at) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(name)
        .bind(&created_at)
        .execute(&db.pool)
        .await?;

    Ok(HouseholdRow {
        id,
        name: name.to_owned(),
        created_at,
    })
}

pub async fn find_for_user(
    db: &Database,
    user_id: Uuid,
) -> Result<Option<HouseholdRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT h.id, h.name, h.created_at \
         FROM household h \
         INNER JOIN membership m ON m.household_id = h.id \
         WHERE m.user_id = ? \
         ORDER BY m.joined_at DESC, h.id DESC \
         LIMIT 1",
    )
    .bind(user_id.to_string())
    .fetch_optional(&db.pool)
    .await?;

    row.map(|r| {
        let id_str: String = r.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok::<_, sqlx::Error>(HouseholdRow {
            id,
            name: r.try_get("name")?,
            created_at: r.try_get("created_at")?,
        })
    })
    .transpose()
}

pub async fn find_by_id(db: &Database, id: Uuid) -> Result<Option<HouseholdRow>, sqlx::Error> {
    let row = sqlx::query("SELECT id, name, created_at FROM household WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(|r| {
        let id_str: String = r.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok::<_, sqlx::Error>(HouseholdRow {
            id,
            name: r.try_get("name")?,
            created_at: r.try_get("created_at")?,
        })
    })
    .transpose()
}

pub async fn rename(
    db: &Database,
    id: Uuid,
    name: &str,
) -> Result<Option<HouseholdRow>, sqlx::Error> {
    let res = sqlx::query("UPDATE household SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find_by_id(db, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{memberships, test_support, users};

    async fn assert_current_household_ordering(db: &Database) {
        let user = users::create(db, "alice", None, "hash").await.unwrap();
        let older = create(db, "Older").await.unwrap();
        let newer = create(db, "Newer").await.unwrap();

        memberships::insert(db, older.id, user.id, "member")
            .await
            .unwrap();
        memberships::insert(db, newer.id, user.id, "member")
            .await
            .unwrap();

        let current = find_for_user(db, user.id).await.unwrap().unwrap();
        assert_eq!(current.id, newer.id);

        let tied_at = "2026-01-01T00:00:00.000Z";
        sqlx::query("UPDATE membership SET joined_at = ? WHERE user_id = ?")
            .bind(tied_at)
            .bind(user.id.to_string())
            .execute(&db.pool)
            .await
            .unwrap();

        let tie_winner = find_for_user(db, user.id).await.unwrap().unwrap();
        assert_eq!(tie_winner.id, older.id.max(newer.id));
    }

    #[tokio::test]
    async fn current_household_ordering_matches_on_sqlite() {
        let db = crate::test_db().await;
        assert_current_household_ordering(&db).await;
    }

    #[tokio::test]
    async fn current_household_ordering_matches_on_postgres() {
        let Some(test_db) = test_support::postgres().await else {
            return;
        };
        assert_current_household_ordering(test_db.db()).await;
    }
}
