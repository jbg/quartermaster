use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct LocationRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub name: String,
    pub kind: String,
    pub sort_order: i64,
    pub created_at: String,
}

pub async fn list_for_household(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<LocationRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, household_id, name, kind, sort_order, created_at \
         FROM location \
         WHERE household_id = ? \
         ORDER BY sort_order ASC, name ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;

    rows.into_iter().map(row_to_location).collect()
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    name: &str,
    kind: &str,
    sort_order: i64,
) -> Result<LocationRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO location (id, household_id, name, kind, sort_order, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(name)
    .bind(kind)
    .bind(sort_order)
    .bind(&created_at)
    .execute(&db.pool)
    .await?;

    Ok(LocationRow {
        id,
        household_id,
        name: name.to_owned(),
        kind: kind.to_owned(),
        sort_order,
        created_at,
    })
}

/// Creates pantry/fridge/freezer on a new household.
pub async fn seed_defaults(db: &Database, household_id: Uuid) -> Result<(), sqlx::Error> {
    create(db, household_id, "Pantry", "pantry", 0).await?;
    create(db, household_id, "Fridge", "fridge", 1).await?;
    create(db, household_id, "Freezer", "freezer", 2).await?;
    Ok(())
}

fn row_to_location(row: sqlx::any::AnyRow) -> Result<LocationRow, sqlx::Error> {
    let id_str: String = row.try_get("id")?;
    let hid_str: String = row.try_get("household_id")?;
    Ok(LocationRow {
        id: Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&hid_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::households;

    #[tokio::test]
    async fn seed_and_list() {
        let db = crate::test_db().await;
        let h = households::create(&db, "Test").await.unwrap();
        seed_defaults(&db, h.id).await.unwrap();

        let locs = list_for_household(&db, h.id).await.unwrap();
        assert_eq!(locs.len(), 3);
        assert_eq!(locs[0].kind, "pantry");
        assert_eq!(locs[1].kind, "fridge");
        assert_eq!(locs[2].kind, "freezer");
    }
}
