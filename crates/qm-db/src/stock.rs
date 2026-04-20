use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use qm_core::batch::{BatchConsumption, BatchRef};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::products::ProductRow;
use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct StockBatchRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub created_by: Uuid,
    pub depleted_at: Option<String>,
}

impl StockBatchRow {
    /// Parse into the pure-domain `BatchRef` used by `qm_core::batch`.
    pub fn to_batch_ref(&self) -> Result<BatchRef, sqlx::Error> {
        let qty = Decimal::from_str(&self.quantity)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let expires = match &self.expires_on {
            Some(s) => Some(
                NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            ),
            None => None,
        };
        let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&self.created_at)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?
            .with_timezone(&Utc);
        Ok(BatchRef {
            id: self.id,
            quantity: qty,
            unit: self.unit.clone(),
            expires_on: expires,
            created_at,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StockBatchWithProduct {
    pub batch: StockBatchRow,
    pub product: ProductRow,
}

#[derive(Debug, Default, Clone)]
pub struct StockFilter {
    pub location_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub expiring_before: Option<NaiveDate>,
    pub include_depleted: bool,
    pub include_undated_when_expiring_filter: bool,
}

pub async fn list(
    db: &Database,
    household_id: Uuid,
    filter: &StockFilter,
) -> Result<Vec<StockBatchWithProduct>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT \
            s.id AS s_id, s.household_id AS s_household_id, s.product_id AS s_product_id, \
            s.location_id AS s_location_id, s.quantity AS s_quantity, s.unit AS s_unit, \
            s.expires_on AS s_expires_on, s.opened_on AS s_opened_on, s.note AS s_note, \
            s.created_at AS s_created_at, s.created_by AS s_created_by, s.depleted_at AS s_depleted_at, \
            p.id AS p_id, p.source AS p_source, p.off_barcode AS p_off_barcode, p.name AS p_name, \
            p.brand AS p_brand, p.family AS p_family, p.default_unit AS p_default_unit, \
            p.image_url AS p_image_url, p.fetched_at AS p_fetched_at, \
            p.created_by_household_id AS p_created_by_household_id, p.created_at AS p_created_at \
         FROM stock_batch s \
         INNER JOIN product p ON p.id = s.product_id \
         WHERE s.household_id = ? ",
    );
    if !filter.include_depleted {
        sql.push_str("AND s.depleted_at IS NULL ");
    }
    if filter.location_id.is_some() {
        sql.push_str("AND s.location_id = ? ");
    }
    if filter.product_id.is_some() {
        sql.push_str("AND s.product_id = ? ");
    }
    if filter.expiring_before.is_some() {
        if filter.include_undated_when_expiring_filter {
            sql.push_str("AND (s.expires_on IS NULL OR s.expires_on < ?) ");
        } else {
            sql.push_str("AND s.expires_on IS NOT NULL AND s.expires_on < ? ");
        }
    }
    sql.push_str(
        "ORDER BY CASE WHEN s.expires_on IS NULL THEN 1 ELSE 0 END, s.expires_on ASC, s.created_at ASC",
    );

    let mut q = sqlx::query(&sql).bind(household_id.to_string());
    if let Some(lid) = filter.location_id {
        q = q.bind(lid.to_string());
    }
    if let Some(pid) = filter.product_id {
        q = q.bind(pid.to_string());
    }
    if let Some(d) = filter.expiring_before {
        q = q.bind(d.format("%Y-%m-%d").to_string());
    }

    let rows = q.fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_joined).collect()
}

pub async fn get(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<StockBatchRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, product_id, location_id, quantity, unit, \
                expires_on, opened_on, note, created_at, created_by, depleted_at \
         FROM stock_batch WHERE id = ? AND household_id = ?",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_batch).transpose()
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    location_id: Uuid,
    quantity: &str,
    unit: &str,
    expires_on: Option<&str>,
    opened_on: Option<&str>,
    note: Option<&str>,
    created_by: Uuid,
) -> Result<StockBatchRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO stock_batch \
         (id, household_id, product_id, location_id, quantity, unit, expires_on, opened_on, note, created_at, created_by, depleted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(location_id.to_string())
    .bind(quantity)
    .bind(unit)
    .bind(expires_on)
    .bind(opened_on)
    .bind(note)
    .bind(&created_at)
    .bind(created_by.to_string())
    .execute(&db.pool)
    .await?;

    Ok(StockBatchRow {
        id,
        household_id,
        product_id,
        location_id,
        quantity: quantity.to_owned(),
        unit: unit.to_owned(),
        expires_on: expires_on.map(str::to_owned),
        opened_on: opened_on.map(str::to_owned),
        note: note.map(str::to_owned),
        created_at,
        created_by,
        depleted_at: None,
    })
}

/// Partial update. Each field uses `Option<Option<T>>`: outer `Some` means the
/// caller supplied a value (possibly clearing), outer `None` means leave
/// unchanged. `quantity`, `unit`, `location_id` are never nullable in the DB,
/// so they're flat `Option<T>` for "change or don't".
#[derive(Debug, Default, Clone)]
pub struct StockUpdate<'a> {
    pub quantity: Option<&'a str>,
    pub unit: Option<&'a str>,
    pub location_id: Option<Uuid>,
    pub expires_on: Option<Option<&'a str>>,
    pub opened_on: Option<Option<&'a str>>,
    pub note: Option<Option<&'a str>>,
}

pub async fn update(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    upd: &StockUpdate<'_>,
) -> Result<StockBatchRow, sqlx::Error> {
    let current = get(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;

    let quantity: &str = upd.quantity.unwrap_or(&current.quantity);
    let unit: &str = upd.unit.unwrap_or(&current.unit);
    let location_id = upd.location_id.unwrap_or(current.location_id);
    let expires_on: Option<String> = match upd.expires_on {
        Some(inner) => inner.map(str::to_owned),
        None => current.expires_on.clone(),
    };
    let opened_on: Option<String> = match upd.opened_on {
        Some(inner) => inner.map(str::to_owned),
        None => current.opened_on.clone(),
    };
    let note: Option<String> = match upd.note {
        Some(inner) => inner.map(str::to_owned),
        None => current.note.clone(),
    };

    sqlx::query(
        "UPDATE stock_batch SET quantity = ?, unit = ?, location_id = ?, expires_on = ?, opened_on = ?, note = ? \
         WHERE id = ? AND household_id = ?",
    )
    .bind(quantity)
    .bind(unit)
    .bind(location_id.to_string())
    .bind(expires_on.as_deref())
    .bind(opened_on.as_deref())
    .bind(note.as_deref())
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?;

    get(db, household_id, id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn delete(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM stock_batch WHERE id = ? AND household_id = ?")
        .bind(id.to_string())
        .bind(household_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn list_active_batches(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    location_id: Option<Uuid>,
) -> Result<Vec<StockBatchRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT id, household_id, product_id, location_id, quantity, unit, \
                expires_on, opened_on, note, created_at, created_by, depleted_at \
         FROM stock_batch \
         WHERE household_id = ? AND product_id = ? AND depleted_at IS NULL ",
    );
    if location_id.is_some() {
        sql.push_str("AND location_id = ? ");
    }
    sql.push_str("ORDER BY CASE WHEN expires_on IS NULL THEN 1 ELSE 0 END, expires_on ASC, created_at ASC");

    let mut q = sqlx::query(&sql)
        .bind(household_id.to_string())
        .bind(product_id.to_string());
    if let Some(lid) = location_id {
        q = q.bind(lid.to_string());
    }
    let rows = q.fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_batch).collect()
}

pub async fn apply_consumption(
    db: &Database,
    household_id: Uuid,
    consumption: &[BatchConsumption],
) -> Result<(), sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let now = now_utc_rfc3339();
    for c in consumption {
        if c.depletes {
            sqlx::query(
                "UPDATE stock_batch SET quantity = '0', depleted_at = ? \
                 WHERE id = ? AND household_id = ?",
            )
            .bind(&now)
            .bind(c.batch_id.to_string())
            .bind(household_id.to_string())
            .execute(&mut *tx)
            .await?;
        } else {
            let row = sqlx::query(
                "SELECT quantity FROM stock_batch WHERE id = ? AND household_id = ?",
            )
            .bind(c.batch_id.to_string())
            .bind(household_id.to_string())
            .fetch_one(&mut *tx)
            .await?;
            let existing: String = row.try_get("quantity")?;
            let cur = Decimal::from_str(&existing).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            let new_qty = cur - c.quantity;
            sqlx::query(
                "UPDATE stock_batch SET quantity = ? WHERE id = ? AND household_id = ?",
            )
            .bind(new_qty.to_string())
            .bind(c.batch_id.to_string())
            .bind(household_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

fn row_to_batch(row: sqlx::any::AnyRow) -> Result<StockBatchRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let household_id: String = row.try_get("household_id")?;
    let product_id: String = row.try_get("product_id")?;
    let location_id: String = row.try_get("location_id")?;
    let created_by: String = row.try_get("created_by")?;
    Ok(StockBatchRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&household_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        product_id: Uuid::parse_str(&product_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        location_id: Uuid::parse_str(&location_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        quantity: row.try_get("quantity")?,
        unit: row.try_get("unit")?,
        expires_on: row.try_get("expires_on")?,
        opened_on: row.try_get("opened_on")?,
        note: row.try_get("note")?,
        created_at: row.try_get("created_at")?,
        created_by: Uuid::parse_str(&created_by).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        depleted_at: row.try_get("depleted_at")?,
    })
}

fn row_to_joined(row: sqlx::any::AnyRow) -> Result<StockBatchWithProduct, sqlx::Error> {
    let batch = StockBatchRow {
        id: uuid_from(&row, "s_id")?,
        household_id: uuid_from(&row, "s_household_id")?,
        product_id: uuid_from(&row, "s_product_id")?,
        location_id: uuid_from(&row, "s_location_id")?,
        quantity: row.try_get("s_quantity")?,
        unit: row.try_get("s_unit")?,
        expires_on: row.try_get("s_expires_on")?,
        opened_on: row.try_get("s_opened_on")?,
        note: row.try_get("s_note")?,
        created_at: row.try_get("s_created_at")?,
        created_by: uuid_from(&row, "s_created_by")?,
        depleted_at: row.try_get("s_depleted_at")?,
    };
    let p_household: Option<String> = row.try_get("p_created_by_household_id")?;
    let product = ProductRow {
        id: uuid_from(&row, "p_id")?,
        source: row.try_get("p_source")?,
        off_barcode: row.try_get("p_off_barcode")?,
        name: row.try_get("p_name")?,
        brand: row.try_get("p_brand")?,
        family: row.try_get("p_family")?,
        preferred_unit: row.try_get("p_default_unit")?,
        image_url: row.try_get("p_image_url")?,
        fetched_at: row.try_get("p_fetched_at")?,
        created_by_household_id: p_household
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        created_at: row.try_get("p_created_at")?,
    };
    Ok(StockBatchWithProduct { batch, product })
}

fn uuid_from(row: &sqlx::any::AnyRow, col: &str) -> Result<Uuid, sqlx::Error> {
    let s: String = row.try_get(col)?;
    Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{households, locations, memberships, products, users};

    async fn setup() -> (Database, Uuid, Uuid, Uuid, Uuid) {
        let db = crate::test_db().await;
        let h = households::create(&db, "h").await.unwrap();
        let u = users::create(&db, "u", None, "hash").await.unwrap();
        memberships::insert(&db, h.id, u.id, "admin").await.unwrap();
        locations::seed_defaults(&db, h.id).await.unwrap();
        let locs = locations::list_for_household(&db, h.id).await.unwrap();
        let pantry = locs.iter().find(|l| l.kind == "pantry").unwrap().id;
        let p = products::create_manual(&db, h.id, "Flour", None, "mass", Some("g"), None)
            .await.unwrap();
        (db, h.id, u.id, pantry, p.id)
    }

    #[tokio::test]
    async fn create_list_update_delete_round_trip() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(&db, hid, pid, lid, "500", "g", Some("2026-06-01"), None, None, uid).await.unwrap();
        assert_eq!(b.quantity, "500");

        let listed = list(&db, hid, &StockFilter::default()).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].batch.id, b.id);
        assert_eq!(listed[0].product.name, "Flour");

        let updated = update(
            &db, hid, b.id,
            &StockUpdate { quantity: Some("400"), note: Some(Some("half used")), ..Default::default() },
        ).await.unwrap();
        assert_eq!(updated.quantity, "400");
        assert_eq!(updated.note.as_deref(), Some("half used"));

        assert!(delete(&db, hid, b.id).await.unwrap());
        assert_eq!(list(&db, hid, &StockFilter::default()).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn apply_consumption_sets_depleted_or_decrements() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b1 = create(&db, hid, pid, lid, "500", "g", Some("2026-05-01"), None, None, uid).await.unwrap();
        let b2 = create(&db, hid, pid, lid, "500", "g", Some("2026-06-01"), None, None, uid).await.unwrap();

        let batches = list_active_batches(&db, hid, pid, None).await.unwrap();
        let refs: Vec<_> = batches.iter().map(|b| b.to_batch_ref().unwrap()).collect();
        let plan = qm_core::batch::plan_consumption(refs, rust_decimal::Decimal::from(750), "g").unwrap();
        assert_eq!(plan.len(), 2);

        apply_consumption(&db, hid, &plan).await.unwrap();

        let after_b1 = get(&db, hid, b1.id).await.unwrap().unwrap();
        assert_eq!(after_b1.quantity, "0");
        assert!(after_b1.depleted_at.is_some());

        let after_b2 = get(&db, hid, b2.id).await.unwrap().unwrap();
        assert_eq!(after_b2.quantity, "250");
        assert!(after_b2.depleted_at.is_none());
    }

    #[tokio::test]
    async fn list_filters_by_location_and_expiring_before() {
        let (db, hid, uid, pantry, pid) = setup().await;
        let locs = locations::list_for_household(&db, hid).await.unwrap();
        let fridge = locs.iter().find(|l| l.kind == "fridge").unwrap().id;

        create(&db, hid, pid, pantry, "100", "g", Some("2026-05-01"), None, None, uid).await.unwrap();
        create(&db, hid, pid, fridge, "200", "g", Some("2026-07-01"), None, None, uid).await.unwrap();

        let in_pantry = list(&db, hid, &StockFilter { location_id: Some(pantry), ..Default::default() }).await.unwrap();
        assert_eq!(in_pantry.len(), 1);
        assert_eq!(in_pantry[0].batch.location_id, pantry);

        let expiring = list(
            &db, hid,
            &StockFilter {
                expiring_before: Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
                ..Default::default()
            },
        ).await.unwrap();
        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].batch.location_id, pantry);
    }
}
