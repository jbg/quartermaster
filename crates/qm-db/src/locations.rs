use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, sql_for_backend, Backend, Database};

#[derive(Debug, Clone, Serialize)]
pub struct LocationRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub name: String,
    pub kind: String,
    pub sort_order: i64,
    pub created_at: String,
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<LocationRow>, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT id, household_id, name, kind, sort_order, created_at \
         FROM location \
         WHERE id = ? AND household_id = ?",
        "SELECT id, household_id, name, kind, sort_order, created_at \
         FROM location \
         WHERE id = $1 AND household_id = $2",
    ))
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_location).transpose()
}

pub async fn list_for_household(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<LocationRow>, sqlx::Error> {
    let rows = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT id, household_id, name, kind, sort_order, created_at \
         FROM location \
         WHERE household_id = ? \
         ORDER BY sort_order ASC, name ASC",
        "SELECT id, household_id, name, kind, sort_order, created_at \
         FROM location \
         WHERE household_id = $1 \
         ORDER BY sort_order ASC, name ASC",
    ))
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
    let mut tx = db.pool.begin().await?;
    let location =
        create_in_tx(&mut tx, db.backend(), household_id, name, kind, sort_order).await?;
    tx.commit().await?;
    Ok(location)
}

pub async fn create_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    household_id: Uuid,
    name: &str,
    kind: &str,
    sort_order: i64,
) -> Result<LocationRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(sql_for_backend(
        backend,
        "INSERT INTO location (id, household_id, name, kind, sort_order, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
        "INSERT INTO location (id, household_id, name, kind, sort_order, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    ))
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(name)
    .bind(kind)
    .bind(sort_order)
    .bind(&created_at)
    .execute(&mut **tx)
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

pub async fn next_sort_order(db: &Database, household_id: Uuid) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT COALESCE(MAX(sort_order), -1) AS n FROM location WHERE household_id = ?",
        "SELECT COALESCE(MAX(sort_order), -1) AS n FROM location WHERE household_id = $1",
    ))
    .bind(household_id.to_string())
    .fetch_one(&db.pool)
    .await?;
    let max: i64 = row.try_get("n")?;
    Ok(max + 1)
}

pub async fn update(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    name: &str,
    kind: &str,
    sort_order: i64,
) -> Result<Option<LocationRow>, sqlx::Error> {
    let res = sqlx::query(sql_for_backend(
        db.backend(),
        "UPDATE location SET name = ?, kind = ?, sort_order = ? \
         WHERE id = ? AND household_id = ?",
        "UPDATE location SET name = $1, kind = $2, sort_order = $3 \
         WHERE id = $4 AND household_id = $5",
    ))
    .bind(name)
    .bind(kind)
    .bind(sort_order)
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find(db, household_id, id).await
}

pub async fn has_active_stock(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT 1 AS x FROM stock_batch \
         WHERE household_id = ? AND location_id = ? AND depleted_at IS NULL LIMIT 1",
        "SELECT 1 AS x FROM stock_batch \
         WHERE household_id = $1 AND location_id = $2 AND depleted_at IS NULL LIMIT 1",
    ))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.is_some())
}

pub async fn delete(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(sql_for_backend(
        db.backend(),
        "DELETE FROM location WHERE id = ? AND household_id = ?",
        "DELETE FROM location WHERE id = $1 AND household_id = $2",
    ))
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Creates pantry/fridge/freezer on a new household.
pub async fn seed_defaults(db: &Database, household_id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    seed_defaults_in_tx(&mut tx, db.backend(), household_id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn seed_defaults_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    household_id: Uuid,
) -> Result<(), sqlx::Error> {
    create_in_tx(tx, backend, household_id, "Pantry", "pantry", 0).await?;
    create_in_tx(tx, backend, household_id, "Fridge", "fridge", 1).await?;
    create_in_tx(tx, backend, household_id, "Freezer", "freezer", 2).await?;
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
        let h = households::create(&db, "Test", "UTC").await.unwrap();
        seed_defaults(&db, h.id).await.unwrap();

        let locs = list_for_household(&db, h.id).await.unwrap();
        assert_eq!(locs.len(), 3);
        assert_eq!(locs[0].kind, "pantry");
        assert_eq!(locs[1].kind, "fridge");
        assert_eq!(locs[2].kind, "freezer");
    }
}
