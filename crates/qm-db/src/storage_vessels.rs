use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct StorageVesselRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub name: String,
    pub tare_weight: String,
    pub tare_unit: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<StorageVesselRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, name, tare_weight, tare_unit, sort_order, created_at, updated_at \
         FROM storage_vessel \
         WHERE id = ? AND household_id = ?",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_storage_vessel).transpose()
}

pub async fn list_for_household(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<StorageVesselRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, household_id, name, tare_weight, tare_unit, sort_order, created_at, updated_at \
         FROM storage_vessel \
         WHERE household_id = ? \
         ORDER BY sort_order ASC, name ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_storage_vessel).collect()
}

pub async fn next_sort_order(db: &Database, household_id: Uuid) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COALESCE(MAX(sort_order), -1) AS n FROM storage_vessel WHERE household_id = ?",
    )
    .bind(household_id.to_string())
    .fetch_one(&db.pool)
    .await?;
    let max: i64 = row.try_get("n")?;
    Ok(max + 1)
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    name: &str,
    tare_weight: &str,
    tare_unit: &str,
    sort_order: i64,
) -> Result<StorageVesselRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO storage_vessel \
         (id, household_id, name, tare_weight, tare_unit, sort_order, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(name)
    .bind(tare_weight)
    .bind(tare_unit)
    .bind(sort_order)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;

    find(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    name: &str,
    tare_weight: &str,
    tare_unit: &str,
    sort_order: i64,
) -> Result<Option<StorageVesselRow>, sqlx::Error> {
    let updated_at = now_utc_rfc3339();
    let res = sqlx::query(
        "UPDATE storage_vessel \
         SET name = ?, tare_weight = ?, tare_unit = ?, sort_order = ?, updated_at = ? \
         WHERE id = ? AND household_id = ?",
    )
    .bind(name)
    .bind(tare_weight)
    .bind(tare_unit)
    .bind(sort_order)
    .bind(updated_at)
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find(db, household_id, id).await
}

pub async fn delete(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM storage_vessel WHERE id = ? AND household_id = ?")
        .bind(id.to_string())
        .bind(household_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

fn row_to_storage_vessel(row: sqlx::any::AnyRow) -> Result<StorageVesselRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let household_id: String = row.try_get("household_id")?;
    Ok(StorageVesselRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&household_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        name: row.try_get("name")?,
        tare_weight: row.try_get("tare_weight")?,
        tare_unit: row.try_get("tare_unit")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
