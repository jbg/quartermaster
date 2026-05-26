use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, sql_for_backend, Backend, Database};

#[derive(Debug, Clone, Serialize)]
pub struct HouseholdRow {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub measurement_system: String,
    pub created_at: String,
    pub deletion_requested_at: Option<String>,
    pub deletion_requested_by: Option<Uuid>,
}

pub async fn create(
    db: &Database,
    name: &str,
    timezone: &str,
) -> Result<HouseholdRow, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let household = create_in_tx(&mut tx, db.backend(), name, timezone).await?;
    tx.commit().await?;
    Ok(household)
}

pub async fn create_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    name: &str,
    timezone: &str,
) -> Result<HouseholdRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(sql_for_backend(
        backend,
        "INSERT INTO household (id, name, timezone, created_at) VALUES (?, ?, ?, ?)",
        "INSERT INTO household (id, name, timezone, created_at) VALUES ($1, $2, $3, $4)",
    ))
    .bind(id.to_string())
    .bind(name)
    .bind(timezone)
    .bind(&created_at)
    .execute(&mut **tx)
    .await?;

    Ok(HouseholdRow {
        id,
        name: name.to_owned(),
        timezone: timezone.to_owned(),
        measurement_system: qm_core::units::MeasurementSystem::DEFAULT
            .as_str()
            .to_owned(),
        created_at,
        deletion_requested_at: None,
        deletion_requested_by: None,
    })
}

pub async fn find_for_user(
    db: &Database,
    user_id: Uuid,
) -> Result<Option<HouseholdRow>, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT h.id, h.name, h.timezone, h.measurement_system, h.created_at, \
                h.deletion_requested_at, h.deletion_requested_by \
         FROM household h \
         INNER JOIN membership m ON m.household_id = h.id \
         WHERE m.user_id = ? AND h.deletion_requested_at IS NULL \
         ORDER BY m.joined_at DESC, h.id DESC \
         LIMIT 1",
        "SELECT h.id, h.name, h.timezone, h.measurement_system, h.created_at, \
                h.deletion_requested_at, h.deletion_requested_by \
         FROM household h \
         INNER JOIN membership m ON m.household_id = h.id \
         WHERE m.user_id = $1 AND h.deletion_requested_at IS NULL \
         ORDER BY m.joined_at DESC, h.id DESC \
         LIMIT 1",
    ))
    .bind(user_id.to_string())
    .fetch_optional(&db.pool)
    .await?;

    row.map(|r| {
        let id_str: String = r.try_get("id")?;
        row_to_household(id_str, r)
    })
    .transpose()
}

pub async fn find_by_id(db: &Database, id: Uuid) -> Result<Option<HouseholdRow>, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT id, name, timezone, measurement_system, created_at, \
                deletion_requested_at, deletion_requested_by \
         FROM household WHERE id = ? AND deletion_requested_at IS NULL",
        "SELECT id, name, timezone, measurement_system, created_at, \
                deletion_requested_at, deletion_requested_by \
         FROM household WHERE id = $1 AND deletion_requested_at IS NULL",
    ))
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(|r| {
        let id_str: String = r.try_get("id")?;
        row_to_household(id_str, r)
    })
    .transpose()
}

pub async fn update(
    db: &Database,
    id: Uuid,
    name: &str,
    timezone: &str,
    measurement_system: &str,
) -> Result<Option<HouseholdRow>, sqlx::Error> {
    let res = sqlx::query(sql_for_backend(
        db.backend(),
        "UPDATE household SET name = ?, timezone = ?, measurement_system = ? \
         WHERE id = ? AND deletion_requested_at IS NULL",
        "UPDATE household SET name = $1, timezone = $2, measurement_system = $3 \
         WHERE id = $4 AND deletion_requested_at IS NULL",
    ))
    .bind(name)
    .bind(timezone)
    .bind(measurement_system)
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find_by_id(db, id).await
}

fn row_to_household(id_str: String, row: sqlx::any::AnyRow) -> Result<HouseholdRow, sqlx::Error> {
    let deleted_by: Option<String> = row.try_get("deletion_requested_by")?;
    Ok(HouseholdRow {
        id: Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        name: row.try_get("name")?,
        timezone: row.try_get("timezone")?,
        measurement_system: row.try_get("measurement_system")?,
        created_at: row.try_get("created_at")?,
        deletion_requested_at: row.try_get("deletion_requested_at")?,
        deletion_requested_by: deleted_by
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{memberships, test_support, users};

    async fn assert_current_household_ordering(db: &Database) {
        let user = users::create(db, "alice@example.com", "Alice", "hash")
            .await
            .unwrap();
        let older = create(db, "Older", "UTC").await.unwrap();
        let newer = create(db, "Newer", "UTC").await.unwrap();

        memberships::insert(db, older.id, user.id, "read_write")
            .await
            .unwrap();
        memberships::insert(db, newer.id, user.id, "read_write")
            .await
            .unwrap();

        let current = find_for_user(db, user.id).await.unwrap().unwrap();
        assert_eq!(current.id, newer.id);

        let tied_at = "2026-01-01T00:00:00.000Z";
        sqlx::query(sql_for_backend(
            db.backend(),
            "UPDATE membership SET joined_at = ? WHERE user_id = ?",
            "UPDATE membership SET joined_at = $1 WHERE user_id = $2",
        ))
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
