use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use qm_core::batch::{BatchConsumption, BatchRef};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::products::ProductRow;
use crate::stock_events::{EVENT_ADD, EVENT_ADJUST, EVENT_CONSUME, EVENT_DISCARD};
use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct StockBatchRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    /// Amount the batch was originally added with. Immutable after creation.
    pub initial_quantity: String,
    /// Cached current balance; sum of stock_event.quantity_delta for this batch.
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
            s.location_id AS s_location_id, s.initial_quantity AS s_initial_quantity, \
            s.quantity AS s_quantity, s.unit AS s_unit, \
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
        "SELECT id, household_id, product_id, location_id, initial_quantity, quantity, unit, \
                expires_on, opened_on, note, created_at, created_by, depleted_at \
         FROM stock_batch WHERE id = ? AND household_id = ?",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_batch).transpose()
}

/// Add a new batch. Writes both the `stock_batch` row and the initial
/// `add` event in a single transaction — the event ledger is the source
/// of truth, and it would be wrong for the batch to exist without one.
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
    let mut tx = db.pool.begin().await?;

    sqlx::query(
        "INSERT INTO stock_batch \
         (id, household_id, product_id, location_id, initial_quantity, quantity, unit, expires_on, opened_on, note, created_at, created_by, depleted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(location_id.to_string())
    .bind(quantity)
    .bind(quantity)
    .bind(unit)
    .bind(expires_on)
    .bind(opened_on)
    .bind(note)
    .bind(&created_at)
    .bind(created_by.to_string())
    .execute(&mut *tx)
    .await?;

    insert_event(
        &mut tx,
        household_id,
        id,
        EVENT_ADD,
        quantity,
        Some("initial add"),
        created_by,
        None,
    )
    .await?;

    tx.commit().await?;

    Ok(StockBatchRow {
        id,
        household_id,
        product_id,
        location_id,
        initial_quantity: quantity.to_owned(),
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

/// Metadata-only update (location, expires_on, opened_on, note). Does NOT
/// touch quantity or unit — for those, go through `adjust`. Non-quantity
/// changes don't produce ledger events.
#[derive(Debug, Default, Clone)]
pub struct StockMetadataUpdate<'a> {
    pub location_id: Option<Uuid>,
    pub expires_on: Option<Option<&'a str>>,
    pub opened_on: Option<Option<&'a str>>,
    pub note: Option<Option<&'a str>>,
}

pub async fn update_metadata(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    upd: &StockMetadataUpdate<'_>,
) -> Result<StockBatchRow, sqlx::Error> {
    let current = get(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let new_location = upd.location_id.unwrap_or(current.location_id);
    let new_expires: Option<String> = match upd.expires_on {
        Some(inner) => inner.map(str::to_owned),
        None => current.expires_on.clone(),
    };
    let new_opened: Option<String> = match upd.opened_on {
        Some(inner) => inner.map(str::to_owned),
        None => current.opened_on.clone(),
    };
    let new_note: Option<String> = match upd.note {
        Some(inner) => inner.map(str::to_owned),
        None => current.note.clone(),
    };

    sqlx::query(
        "UPDATE stock_batch SET location_id = ?, expires_on = ?, opened_on = ?, note = ? \
         WHERE id = ? AND household_id = ?",
    )
    .bind(new_location.to_string())
    .bind(new_expires.as_deref())
    .bind(new_opened.as_deref())
    .bind(new_note.as_deref())
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?;

    get(db, household_id, id).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Correct a batch's quantity. Writes an `adjust` event with `delta = new - current`
/// and updates the cached balance. Unit is immutable after creation — if the
/// caller got the unit wrong originally, delete the batch and re-add it.
pub async fn adjust(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    new_quantity: &str,
    actor: Uuid,
    note: Option<&str>,
) -> Result<StockBatchRow, sqlx::Error> {
    let mut tx = db.pool.begin().await?;

    let current = sqlx::query(
        "SELECT quantity FROM stock_batch WHERE id = ? AND household_id = ?",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;

    let current_quantity: String = current.try_get("quantity")?;
    let current_d = Decimal::from_str(&current_quantity)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let new_d = Decimal::from_str(new_quantity)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let delta = new_d - current_d;
    let depletes = new_d <= Decimal::ZERO;

    if !delta.is_zero() {
        insert_event(
            &mut tx,
            household_id,
            id,
            EVENT_ADJUST,
            &delta.to_string(),
            Some(note.unwrap_or("quantity corrected via PATCH")),
            actor,
            None,
        )
        .await?;
    }

    if depletes {
        sqlx::query(
            "UPDATE stock_batch SET quantity = '0', depleted_at = ? WHERE id = ? AND household_id = ?",
        )
        .bind(now_utc_rfc3339())
        .bind(id.to_string())
        .bind(household_id.to_string())
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "UPDATE stock_batch SET quantity = ?, depleted_at = NULL WHERE id = ? AND household_id = ?",
        )
        .bind(new_quantity)
        .bind(id.to_string())
        .bind(household_id.to_string())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    get(db, household_id, id).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Close the batch's account: write a `discard` event that zeroes the balance,
/// set `depleted_at`, and leave the row in place. The batch is no longer
/// returned from the default (active-only) list queries but its history
/// remains forever. The HTTP `DELETE /stock/{id}` endpoint funnels here.
pub async fn discard(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    actor: Uuid,
    note: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.pool.begin().await?;

    let current = sqlx::query(
        "SELECT quantity, depleted_at FROM stock_batch WHERE id = ? AND household_id = ?",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&mut *tx)
    .await?;

    let Some(row) = current else {
        return Ok(false);
    };
    let depleted_at: Option<String> = row.try_get("depleted_at")?;
    if depleted_at.is_some() {
        // Already closed; nothing to do. Report success so repeated DELETEs
        // are idempotent.
        return Ok(true);
    }
    let current_quantity: String = row.try_get("quantity")?;
    let current_d = Decimal::from_str(&current_quantity)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

    // Only record an event for a non-zero balance. Closing an already-empty
    // batch would be a no-op delta.
    if !current_d.is_zero() {
        let delta = -current_d;
        insert_event(
            &mut tx,
            household_id,
            id,
            EVENT_DISCARD,
            &delta.to_string(),
            Some(note.unwrap_or("discarded via DELETE /stock/{id}")),
            actor,
            None,
        )
        .await?;
    }

    sqlx::query(
        "UPDATE stock_batch SET quantity = '0', depleted_at = ? WHERE id = ? AND household_id = ?",
    )
    .bind(now_utc_rfc3339())
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

pub async fn list_active_batches(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    location_id: Option<Uuid>,
) -> Result<Vec<StockBatchRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT id, household_id, product_id, location_id, initial_quantity, quantity, unit, \
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

/// Write consume events and decrement balances for a consumption plan.
/// All events share a `consume_request_id` so a later timeline can collapse
/// them back into one logical action.
pub async fn apply_consumption(
    db: &Database,
    household_id: Uuid,
    consumption: &[BatchConsumption],
    actor: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let consume_request_id = Uuid::now_v7();
    let mut tx = db.pool.begin().await?;
    let now = now_utc_rfc3339();
    for c in consumption {
        insert_event(
            &mut tx,
            household_id,
            c.batch_id,
            EVENT_CONSUME,
            &(-c.quantity).to_string(),
            Some("consumed via POST /stock/consume"),
            actor,
            Some(consume_request_id),
        )
        .await?;

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
    Ok(consume_request_id)
}

/// Ancillary helper for products repo — "does this product have any active
/// (non-depleted) stock anywhere?" — used to refuse product deletion.
pub async fn has_active_stock_for_product(
    db: &Database,
    product_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(
        "SELECT 1 AS x FROM stock_batch WHERE product_id = ? AND depleted_at IS NULL LIMIT 1",
    )
    .bind(product_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.is_some())
}

/// "Does this product have any active batches whose unit is outside the
/// target family?" — used to gate a family change on a manual product.
pub async fn conflicting_units_for_family_change(
    db: &Database,
    product_id: Uuid,
    target_family: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT DISTINCT unit FROM stock_batch WHERE product_id = ? AND depleted_at IS NULL",
    )
    .bind(product_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    let mut conflicts = Vec::new();
    for row in rows {
        let unit: String = row.try_get("unit")?;
        if let Ok(u) = qm_core::units::lookup(&unit) {
            if u.family.as_str() != target_family {
                conflicts.push(unit);
            }
        }
    }
    Ok(conflicts)
}

// ----- internals -----

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    batch_id: Uuid,
    event_type: &str,
    delta: &str,
    note: Option<&str>,
    created_by: Uuid,
    consume_request_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO stock_event (id, household_id, batch_id, event_type, quantity_delta, note, created_at, created_by, consume_request_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(batch_id.to_string())
    .bind(event_type)
    .bind(delta)
    .bind(note)
    .bind(now_utc_rfc3339())
    .bind(created_by.to_string())
    .bind(consume_request_id.map(|u| u.to_string()))
    .execute(&mut **tx)
    .await?;
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
        initial_quantity: row.try_get("initial_quantity")?,
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
        initial_quantity: row.try_get("s_initial_quantity")?,
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
    use crate::{households, locations, memberships, products, stock_events, users};

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

    async fn balance_from_events(db: &Database, batch_id: Uuid) -> Decimal {
        stock_events::list_for_batch(db, batch_id)
            .await
            .unwrap()
            .into_iter()
            .map(|e| Decimal::from_str(&e.quantity_delta).unwrap())
            .sum()
    }

    #[tokio::test]
    async fn create_writes_add_event() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(&db, hid, pid, lid, "500", "g", None, None, None, uid).await.unwrap();
        let events = stock_events::list_for_batch(&db, b.id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "add");
        assert_eq!(events[0].quantity_delta, "500");
        assert_eq!(b.initial_quantity, "500");
        assert_eq!(b.quantity, "500");
    }

    #[tokio::test]
    async fn adjust_writes_adjust_event_and_updates_cache() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(&db, hid, pid, lid, "500", "g", None, None, None, uid).await.unwrap();

        let after = adjust(&db, hid, b.id, "300", uid, None).await.unwrap();
        assert_eq!(after.quantity, "300");
        assert!(after.depleted_at.is_none());

        let events = stock_events::list_for_batch(&db, b.id).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].event_type, "adjust");
        assert_eq!(events[1].quantity_delta, "-200");

        assert_eq!(balance_from_events(&db, b.id).await, Decimal::from(300));
    }

    #[tokio::test]
    async fn adjust_to_zero_depletes_batch() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(&db, hid, pid, lid, "500", "g", None, None, None, uid).await.unwrap();
        let after = adjust(&db, hid, b.id, "0", uid, None).await.unwrap();
        assert_eq!(after.quantity, "0");
        assert!(after.depleted_at.is_some());
    }

    #[tokio::test]
    async fn discard_writes_event_and_marks_depleted_without_deleting_row() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(&db, hid, pid, lid, "500", "g", None, None, None, uid).await.unwrap();
        let removed = discard(&db, hid, b.id, uid, None).await.unwrap();
        assert!(removed);

        let still_there = get(&db, hid, b.id).await.unwrap().unwrap();
        assert_eq!(still_there.quantity, "0");
        assert!(still_there.depleted_at.is_some());

        let events = stock_events::list_for_batch(&db, b.id).await.unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].event_type, "discard");
        assert_eq!(events[1].quantity_delta, "-500");

        assert_eq!(balance_from_events(&db, b.id).await, Decimal::ZERO);
    }

    #[tokio::test]
    async fn apply_consumption_correlates_events() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b1 = create(&db, hid, pid, lid, "500", "g", Some("2026-05-01"), None, None, uid).await.unwrap();
        let b2 = create(&db, hid, pid, lid, "500", "g", Some("2026-06-01"), None, None, uid).await.unwrap();

        let batches = list_active_batches(&db, hid, pid, None).await.unwrap();
        let refs: Vec<_> = batches.iter().map(|b| b.to_batch_ref().unwrap()).collect();
        let plan = qm_core::batch::plan_consumption(refs, Decimal::from(750), "g").unwrap();
        let request_id = apply_consumption(&db, hid, &plan, uid).await.unwrap();

        let events_b1 = stock_events::list_for_batch(&db, b1.id).await.unwrap();
        let events_b2 = stock_events::list_for_batch(&db, b2.id).await.unwrap();
        // b1: add + consume. b2: add + consume.
        assert_eq!(events_b1.len(), 2);
        assert_eq!(events_b2.len(), 2);
        assert_eq!(events_b1[1].event_type, "consume");
        assert_eq!(events_b1[1].consume_request_id, Some(request_id));
        assert_eq!(events_b2[1].consume_request_id, Some(request_id));

        // Balances match the ledger.
        assert_eq!(balance_from_events(&db, b1.id).await, Decimal::ZERO);
        assert_eq!(balance_from_events(&db, b2.id).await, Decimal::from(250));
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

    #[tokio::test]
    async fn metadata_update_does_not_write_events() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(&db, hid, pid, lid, "500", "g", None, None, None, uid).await.unwrap();
        let locs = locations::list_for_household(&db, hid).await.unwrap();
        let fridge = locs.iter().find(|l| l.kind == "fridge").unwrap().id;

        let after = update_metadata(
            &db, hid, b.id,
            &StockMetadataUpdate {
                location_id: Some(fridge),
                note: Some(Some("moved to fridge")),
                ..Default::default()
            },
        ).await.unwrap();
        assert_eq!(after.location_id, fridge);
        assert_eq!(after.note.as_deref(), Some("moved to fridge"));

        // Only the initial add event — metadata updates don't write events.
        let events = stock_events::list_for_batch(&db, b.id).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn has_active_stock_and_family_conflicts() {
        let (db, hid, uid, lid, pid) = setup().await;
        assert!(!has_active_stock_for_product(&db, pid).await.unwrap());

        let b = create(&db, hid, pid, lid, "500", "g", None, None, None, uid).await.unwrap();
        assert!(has_active_stock_for_product(&db, pid).await.unwrap());

        let conflicts = conflicting_units_for_family_change(&db, pid, "volume").await.unwrap();
        assert_eq!(conflicts, vec!["g".to_string()]);

        let none = conflicting_units_for_family_change(&db, pid, "mass").await.unwrap();
        assert!(none.is_empty());

        discard(&db, hid, b.id, uid, None).await.unwrap();
        assert!(!has_active_stock_for_product(&db, pid).await.unwrap());
    }
}
