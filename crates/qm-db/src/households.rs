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
         ORDER BY m.joined_at ASC \
         LIMIT 1",
    )
    .bind(user_id.to_string())
    .fetch_optional(&db.pool)
    .await?;

    row.map(|r| {
        let id_str: String = r.try_get("id")?;
        let id = Uuid::parse_str(&id_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok::<_, sqlx::Error>(HouseholdRow {
            id,
            name: r.try_get("name")?,
            created_at: r.try_get("created_at")?,
        })
    })
    .transpose()
}
