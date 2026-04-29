use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct LabelPrinterRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub name: String,
    pub driver: String,
    pub address: String,
    pub port: i64,
    pub media: String,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewLabelPrinter<'a> {
    pub name: &'a str,
    pub driver: &'a str,
    pub address: &'a str,
    pub port: i64,
    pub media: &'a str,
    pub enabled: bool,
    pub is_default: bool,
}

#[derive(Debug, Default, Clone)]
pub struct LabelPrinterUpdate<'a> {
    pub name: Option<&'a str>,
    pub address: Option<&'a str>,
    pub port: Option<i64>,
    pub media: Option<&'a str>,
    pub enabled: Option<bool>,
    pub is_default: Option<bool>,
}

pub async fn list_for_household(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<LabelPrinterRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, household_id, name, driver, address, port, media, enabled, is_default, created_at, updated_at \
         FROM label_printer \
         WHERE household_id = ? \
         ORDER BY is_default DESC, name ASC, created_at ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_printer).collect()
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<LabelPrinterRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, name, driver, address, port, media, enabled, is_default, created_at, updated_at \
         FROM label_printer \
         WHERE household_id = ? AND id = ?",
    )
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_printer).transpose()
}

pub async fn default_enabled(
    db: &Database,
    household_id: Uuid,
) -> Result<Option<LabelPrinterRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, name, driver, address, port, media, enabled, is_default, created_at, updated_at \
         FROM label_printer \
         WHERE household_id = ? AND enabled = 1 \
         ORDER BY is_default DESC, created_at ASC \
         LIMIT 1",
    )
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_printer).transpose()
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    new: &NewLabelPrinter<'_>,
) -> Result<LabelPrinterRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;
    if new.is_default {
        clear_default_tx(&mut tx, household_id).await?;
    }
    sqlx::query(
        "INSERT INTO label_printer \
         (id, household_id, name, driver, address, port, media, enabled, is_default, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.name)
    .bind(new.driver)
    .bind(new.address)
    .bind(new.port)
    .bind(new.media)
    .bind(bool_int(new.enabled))
    .bind(bool_int(new.is_default))
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    find(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    upd: &LabelPrinterUpdate<'_>,
) -> Result<Option<LabelPrinterRow>, sqlx::Error> {
    let Some(current) = find(db, household_id, id).await? else {
        return Ok(None);
    };
    let name = upd.name.unwrap_or(&current.name);
    let address = upd.address.unwrap_or(&current.address);
    let port = upd.port.unwrap_or(current.port);
    let media = upd.media.unwrap_or(&current.media);
    let enabled = upd.enabled.unwrap_or(current.enabled);
    let is_default = upd.is_default.unwrap_or(current.is_default);
    let now = now_utc_rfc3339();

    let mut tx = db.pool.begin().await?;
    if is_default {
        clear_default_tx(&mut tx, household_id).await?;
    }
    let res = sqlx::query(
        "UPDATE label_printer \
         SET name = ?, address = ?, port = ?, media = ?, enabled = ?, is_default = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(name)
    .bind(address)
    .bind(port)
    .bind(media)
    .bind(bool_int(enabled))
    .bind(bool_int(is_default))
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find(db, household_id, id).await
}

pub async fn delete(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM label_printer WHERE household_id = ? AND id = ?")
        .bind(household_id.to_string())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

async fn clear_default_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE label_printer SET is_default = 0 WHERE household_id = ?")
        .bind(household_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn row_to_printer(row: sqlx::any::AnyRow) -> Result<LabelPrinterRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let household_id: String = row.try_get("household_id")?;
    let enabled: i64 = row.try_get("enabled")?;
    let is_default: i64 = row.try_get("is_default")?;
    Ok(LabelPrinterRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&household_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        name: row.try_get("name")?,
        driver: row.try_get("driver")?,
        address: row.try_get("address")?,
        port: row.try_get("port")?,
        media: row.try_get("media")?,
        enabled: enabled != 0,
        is_default: is_default != 0,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn bool_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::households;

    #[tokio::test]
    async fn only_one_default_printer_per_household() {
        let db = crate::test_db().await;
        let household = households::create(&db, "Kitchen", "UTC").await.unwrap();
        let first = create(
            &db,
            household.id,
            &NewLabelPrinter {
                name: "Pantry",
                driver: "brother_ql_raster",
                address: "192.0.2.10",
                port: 9100,
                media: "dk_62_continuous",
                enabled: true,
                is_default: true,
            },
        )
        .await
        .unwrap();
        let second = create(
            &db,
            household.id,
            &NewLabelPrinter {
                name: "Prep",
                driver: "brother_ql_raster",
                address: "192.0.2.11",
                port: 9100,
                media: "dk_29x90",
                enabled: true,
                is_default: true,
            },
        )
        .await
        .unwrap();

        assert!(
            !find(&db, household.id, first.id)
                .await
                .unwrap()
                .unwrap()
                .is_default
        );
        assert!(
            find(&db, household.id, second.id)
                .await
                .unwrap()
                .unwrap()
                .is_default
        );
    }
}
