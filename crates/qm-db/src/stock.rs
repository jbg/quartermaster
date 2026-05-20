//! Event-sourced stock operations.
//!
//! Every quantity-changing function wraps its SELECT + UPDATE + event-INSERT
//! in a single transaction, and `stock_batch.quantity` is a cache of
//! `SUM(stock_event.quantity_delta)` for the batch.
//!
//! Under Postgres `READ COMMITTED`, quantity-changing paths take a row lock
//! (`SELECT ... FOR UPDATE`) before reading the cached quantity so the
//! `stock_batch.quantity` cache stays in sync with the event ledger. SQLite
//! already serialises writers, so the same helpers fall back to a normal
//! `SELECT` there.

use std::str::FromStr;

use jiff::{civil::Date, Timestamp};
use qm_core::batch::{BatchConsumption, BatchRef};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::products::ProductRow;
use crate::reminders::ExpiryReminderPolicy;
use crate::stock_events::{
    latest_for_batch_tx, EVENT_ADD, EVENT_ADJUST, EVENT_CONSUME, EVENT_DISCARD, EVENT_REPACK_IN,
    EVENT_REPACK_OUT, EVENT_RESTORE,
};
use crate::storage_vessels::StorageVesselRow;
use crate::{now_utc_rfc3339, Backend, Database};

#[derive(Debug, Clone, Serialize)]
pub struct StockBatchRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub storage_vessel_id: Option<Uuid>,
    pub source_batch_id: Option<Uuid>,
    pub source_operation_id: Option<Uuid>,
    /// Amount the batch was originally added with. Immutable after creation.
    pub initial_quantity: String,
    /// Cached current balance; sum of stock_event.quantity_delta for this batch.
    pub quantity: String,
    pub unit: String,
    pub package_quantity: Option<String>,
    pub package_unit: Option<String>,
    pub produced_on: Option<String>,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub created_by: Uuid,
    pub depleted_at: Option<String>,
}

impl StockBatchRow {
    pub fn to_batch_ref(&self) -> Result<BatchRef, sqlx::Error> {
        let qty =
            Decimal::from_str(&self.quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let expires = match &self.expires_on {
            Some(s) => Some(crate::time::parse_date(s)?),
            None => None,
        };
        let created_at: Timestamp = crate::time::parse_timestamp(&self.created_at)?;
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
    pub location_name: String,
    pub storage_vessel: Option<StorageVesselRow>,
}

#[derive(Debug)]
pub enum SplitStockError {
    NotFound,
    Depleted,
    EmptyRemainders,
    InvalidTotal,
    InvalidOperation,
    QuantityNotPositive,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for SplitStockError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

#[derive(Debug, Clone)]
pub struct SplitRemainder {
    pub location_id: Uuid,
    pub storage_vessel_id: Option<Uuid>,
    pub quantity: String,
    pub expires_on: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SplitStockResult {
    pub source: StockBatchRow,
    pub remainders: Vec<StockBatchRow>,
    pub operation_id: Uuid,
}

#[derive(Debug, Default, Clone)]
pub struct StockFilter {
    pub location_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub expiring_before: Option<Date>,
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
            s.location_id AS s_location_id, s.storage_vessel_id AS s_storage_vessel_id, \
            s.source_batch_id AS s_source_batch_id, s.source_operation_id AS s_source_operation_id, \
            s.initial_quantity AS s_initial_quantity, \
            s.quantity AS s_quantity, s.unit AS s_unit, \
            s.package_quantity AS s_package_quantity, s.package_unit AS s_package_unit, \
            s.produced_on AS s_produced_on, s.expires_on AS s_expires_on, \
            s.opened_on AS s_opened_on, s.note AS s_note, \
            s.created_at AS s_created_at, s.created_by AS s_created_by, s.depleted_at AS s_depleted_at, \
            p.id AS p_id, p.source AS p_source, p.off_barcode AS p_off_barcode, p.name AS p_name, \
            p.brand AS p_brand, p.family AS p_family, p.default_unit AS p_default_unit, \
            p.image_url AS p_image_url, p.package_quantity AS p_package_quantity, \
            p.package_unit AS p_package_unit, p.fetched_at AS p_fetched_at, \
            p.created_by_household_id AS p_created_by_household_id, p.created_at AS p_created_at, \
            p.deleted_at AS p_deleted_at, p.max_open_days AS p_max_open_days, \
            l.name AS l_name, \
            v.id AS v_id, v.household_id AS v_household_id, v.name AS v_name, \
            v.tare_weight AS v_tare_weight, v.tare_unit AS v_tare_unit, \
            v.sort_order AS v_sort_order, v.created_at AS v_created_at, v.updated_at AS v_updated_at \
         FROM stock_batch s \
         INNER JOIN product p ON p.id = s.product_id \
         INNER JOIN location l ON l.id = s.location_id \
         LEFT JOIN storage_vessel v ON v.id = s.storage_vessel_id \
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
        q = q.bind(crate::time::format_date(d));
    }

    let rows = q.fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_joined).collect()
}

pub async fn get_with_product(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<StockBatchWithProduct>, sqlx::Error> {
    get_with_product_inner(db, household_id, id, false).await
}

pub async fn get_with_product_including_deleted(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<StockBatchWithProduct>, sqlx::Error> {
    get_with_product_inner(db, household_id, id, true).await
}

async fn get_with_product_inner(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    include_deleted_product: bool,
) -> Result<Option<StockBatchWithProduct>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT \
            s.id AS s_id, s.household_id AS s_household_id, s.product_id AS s_product_id, \
            s.location_id AS s_location_id, s.storage_vessel_id AS s_storage_vessel_id, \
            s.source_batch_id AS s_source_batch_id, s.source_operation_id AS s_source_operation_id, \
            s.initial_quantity AS s_initial_quantity, \
            s.quantity AS s_quantity, s.unit AS s_unit, \
            s.package_quantity AS s_package_quantity, s.package_unit AS s_package_unit, \
            s.produced_on AS s_produced_on, s.expires_on AS s_expires_on, \
            s.opened_on AS s_opened_on, s.note AS s_note, \
            s.created_at AS s_created_at, s.created_by AS s_created_by, s.depleted_at AS s_depleted_at, \
            p.id AS p_id, p.source AS p_source, p.off_barcode AS p_off_barcode, p.name AS p_name, \
            p.brand AS p_brand, p.family AS p_family, p.default_unit AS p_default_unit, \
            p.image_url AS p_image_url, p.package_quantity AS p_package_quantity, \
            p.package_unit AS p_package_unit, p.fetched_at AS p_fetched_at, \
            p.created_by_household_id AS p_created_by_household_id, p.created_at AS p_created_at, \
            p.deleted_at AS p_deleted_at, p.max_open_days AS p_max_open_days, \
            l.name AS l_name, \
            v.id AS v_id, v.household_id AS v_household_id, v.name AS v_name, \
            v.tare_weight AS v_tare_weight, v.tare_unit AS v_tare_unit, \
            v.sort_order AS v_sort_order, v.created_at AS v_created_at, v.updated_at AS v_updated_at \
         FROM stock_batch s \
         INNER JOIN product p ON p.id = s.product_id \
         INNER JOIN location l ON l.id = s.location_id \
         LEFT JOIN storage_vessel v ON v.id = s.storage_vessel_id \
         WHERE s.household_id = ? AND s.id = ?",
    );
    if !include_deleted_product {
        sql.push_str(" AND p.deleted_at IS NULL");
    }
    let row = sqlx::query(&sql)
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_joined).transpose()
}

pub async fn get(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<StockBatchRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, product_id, location_id, storage_vessel_id, source_batch_id, source_operation_id, initial_quantity, quantity, unit, \
                package_quantity, package_unit, produced_on, expires_on, opened_on, note, \
                created_at, created_by, depleted_at \
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
    produced_on: Option<&str>,
    expires_on: Option<&str>,
    opened_on: Option<&str>,
    note: Option<&str>,
    created_by: Uuid,
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<StockBatchRow, sqlx::Error> {
    create_with_storage_vessel(
        db,
        household_id,
        product_id,
        location_id,
        None,
        quantity,
        unit,
        produced_on,
        expires_on,
        opened_on,
        note,
        created_by,
        reminder_policy,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_with_storage_vessel(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    location_id: Uuid,
    storage_vessel_id: Option<Uuid>,
    quantity: &str,
    unit: &str,
    produced_on: Option<&str>,
    expires_on: Option<&str>,
    opened_on: Option<&str>,
    note: Option<&str>,
    created_by: Uuid,
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<StockBatchRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let created_at = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;

    sqlx::query(
        "INSERT INTO stock_batch \
         (id, household_id, product_id, location_id, storage_vessel_id, source_batch_id, source_operation_id, initial_quantity, quantity, unit, package_quantity, package_unit, produced_on, expires_on, opened_on, note, created_at, created_by, depleted_at) \
         VALUES (?, ?, ?, ?, ?, NULL, NULL, ?, ?, ?, (SELECT package_quantity FROM product WHERE id = ?), (SELECT package_unit FROM product WHERE id = ?), ?, ?, ?, ?, ?, ?, NULL)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(location_id.to_string())
    .bind(storage_vessel_id.map(|id| id.to_string()))
    .bind(quantity)
    .bind(quantity)
    .bind(unit)
    .bind(product_id.to_string())
    .bind(product_id.to_string())
    .bind(produced_on)
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

    if let Some(policy) = reminder_policy {
        crate::reminders::sync_expiry_for_batch_tx(&mut tx, id, policy).await?;
    }

    tx.commit().await?;

    get(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// Metadata-only update (location, produced_on, expires_on, opened_on, note). Does NOT
/// touch quantity or unit — for those, go through `adjust`. Non-quantity
/// changes don't produce ledger events.
#[derive(Debug, Default, Clone)]
pub struct StockMetadataUpdate<'a> {
    pub location_id: Option<Uuid>,
    pub storage_vessel_id: Option<Option<Uuid>>,
    pub produced_on: Option<Option<&'a str>>,
    pub expires_on: Option<Option<&'a str>>,
    pub opened_on: Option<Option<&'a str>>,
    pub note: Option<Option<&'a str>>,
}

pub async fn update_metadata(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    upd: &StockMetadataUpdate<'_>,
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<StockBatchRow, sqlx::Error> {
    let current = get(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let new_location = upd.location_id.unwrap_or(current.location_id);
    let new_storage_vessel = match upd.storage_vessel_id {
        Some(inner) => inner,
        None => current.storage_vessel_id,
    };
    let new_expires: Option<String> = match upd.expires_on {
        Some(inner) => inner.map(str::to_owned),
        None => current.expires_on.clone(),
    };
    let new_produced: Option<String> = match upd.produced_on {
        Some(inner) => inner.map(str::to_owned),
        None => current.produced_on.clone(),
    };
    let new_opened: Option<String> = match upd.opened_on {
        Some(inner) => inner.map(str::to_owned),
        None => current.opened_on.clone(),
    };
    let new_note: Option<String> = match upd.note {
        Some(inner) => inner.map(str::to_owned),
        None => current.note.clone(),
    };

    let mut tx = db.pool.begin().await?;
    sqlx::query(
        "UPDATE stock_batch SET location_id = ?, storage_vessel_id = ?, produced_on = ?, expires_on = ?, opened_on = ?, note = ? \
         WHERE id = ? AND household_id = ?",
    )
    .bind(new_location.to_string())
    .bind(new_storage_vessel.map(|id| id.to_string()))
    .bind(new_produced.as_deref())
    .bind(new_expires.as_deref())
    .bind(new_opened.as_deref())
    .bind(new_note.as_deref())
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&mut *tx)
    .await?;

    if let Some(policy) = reminder_policy {
        crate::reminders::sync_expiry_for_batch_tx(&mut tx, id, policy).await?;
    }

    tx.commit().await?;

    get(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
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
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<StockBatchRow, sqlx::Error> {
    let mut tx = db.pool.begin().await?;

    let current = fetch_locked_batch_row(&mut tx, db.backend(), household_id, id, "quantity")
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;

    let current_quantity: String = current.try_get("quantity")?;
    let current_d =
        Decimal::from_str(&current_quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let new_d = Decimal::from_str(new_quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
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

    if let Some(policy) = reminder_policy {
        crate::reminders::sync_expiry_for_batch_tx(&mut tx, id, policy).await?;
    }

    tx.commit().await?;
    get(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
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
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.pool.begin().await?;

    let current = fetch_locked_batch_row(
        &mut tx,
        db.backend(),
        household_id,
        id,
        "quantity, depleted_at",
    )
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
    let current_d =
        Decimal::from_str(&current_quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

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

    if let Some(policy) = reminder_policy {
        crate::reminders::sync_expiry_for_batch_tx(&mut tx, id, policy).await?;
    }

    tx.commit().await?;
    Ok(true)
}

#[derive(Debug)]
pub enum RestoreError {
    NotFound,
    /// The batch's most recent event isn't a `discard`. Restore only undoes
    /// an explicit discard; fully-consumed batches are out of scope (the
    /// last consume event doesn't carry enough information to reconstruct
    /// the pre-consume state honestly).
    NotRestorable,
    /// Bulk-restore rolled back because at least one of the inputs wasn't
    /// restorable. The vector names exactly the IDs that failed so the
    /// caller can point the user at them.
    NotRestorableMany(Vec<Uuid>),
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RestoreError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

/// Undo a discard: re-activate the batch at the exact pre-discard balance.
/// Rejects with `NotRestorable` if the batch was consumed to zero (no
/// discard event), already restored, or otherwise doesn't end on a discard.
pub async fn restore(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    actor: Uuid,
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<StockBatchRow, RestoreError> {
    let mut tx = db.pool.begin().await?;
    restore_in_tx(
        &mut tx,
        db.backend(),
        household_id,
        id,
        actor,
        reminder_policy,
    )
    .await?;
    tx.commit().await?;
    get(db, household_id, id)
        .await?
        .ok_or(RestoreError::NotFound)
}

/// Bulk version of `restore`. One transaction spans every ID. Visits each
/// in turn; if any is unrecoverable (unknown batch or last event isn't a
/// discard), continues collecting unrestorable IDs rather than bailing on
/// the first — then rolls back the whole transaction and returns the list.
/// Lets the caller tell the user exactly which rows were the problem
/// instead of a generic "one of these failed".
pub async fn restore_many(
    db: &Database,
    household_id: Uuid,
    ids: &[Uuid],
    actor: Uuid,
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<Vec<StockBatchRow>, RestoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut tx = db.pool.begin().await?;
    let mut unrestorable: Vec<Uuid> = Vec::new();

    for id in ids {
        match restore_in_tx(
            &mut tx,
            db.backend(),
            household_id,
            *id,
            actor,
            reminder_policy,
        )
        .await
        {
            Ok(()) => {}
            // `NotFound` on a bulk op is "unknown-or-not-yours" — treat it
            // like any other unrestorable so the caller gets one clean list.
            Err(RestoreError::NotFound) | Err(RestoreError::NotRestorable) => {
                unrestorable.push(*id);
            }
            Err(other) => return Err(other),
        }
    }

    if !unrestorable.is_empty() {
        // Dropping `tx` without commit rolls back any restores we wrote
        // before we discovered the problem rows. The caller is left with
        // the same state they started in.
        return Err(RestoreError::NotRestorableMany(unrestorable));
    }

    tx.commit().await?;

    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let row = get(db, household_id, *id)
            .await?
            .ok_or(RestoreError::NotFound)?;
        out.push(row);
    }
    Ok(out)
}

/// Internal restore primitive. Runs inside the caller's transaction so
/// `restore_many` can batch many IDs under a single commit boundary.
async fn restore_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    household_id: Uuid,
    id: Uuid,
    actor: Uuid,
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<(), RestoreError> {
    // Verify the batch belongs to the household (mirrors `get` but inside tx).
    let present = fetch_locked_batch_row(tx, backend, household_id, id, "id").await?;
    if present.is_none() {
        return Err(RestoreError::NotFound);
    }

    let latest = latest_for_batch_tx(tx, id).await?;
    let Some(latest) = latest else {
        return Err(RestoreError::NotRestorable);
    };
    if latest.event_type != EVENT_DISCARD {
        return Err(RestoreError::NotRestorable);
    }

    // The discard event's delta is negative (e.g. -500). The restore is its
    // inverse (+500). Parse, negate, serialise back.
    let discard_delta =
        Decimal::from_str(&latest.quantity_delta).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let restored_delta = -discard_delta;
    let new_quantity = restored_delta; // since batch was at zero post-discard

    insert_event(
        tx,
        household_id,
        id,
        EVENT_RESTORE,
        &restored_delta.to_string(),
        Some("restored via POST /stock/{id}/restore"),
        actor,
        None,
    )
    .await?;

    sqlx::query(
        "UPDATE stock_batch SET quantity = ?, depleted_at = NULL \
         WHERE id = ? AND household_id = ?",
    )
    .bind(new_quantity.to_string())
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&mut **tx)
    .await?;

    if let Some(policy) = reminder_policy {
        crate::reminders::sync_expiry_for_batch_tx(tx, id, policy).await?;
    }

    Ok(())
}

pub async fn list_active_batches(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    location_id: Option<Uuid>,
) -> Result<Vec<StockBatchRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT id, household_id, product_id, location_id, storage_vessel_id, source_batch_id, source_operation_id, initial_quantity, quantity, unit, \
                package_quantity, package_unit, produced_on, expires_on, opened_on, note, \
                created_at, created_by, depleted_at \
         FROM stock_batch \
         WHERE household_id = ? AND product_id = ? AND depleted_at IS NULL ",
    );
    if location_id.is_some() {
        sql.push_str("AND location_id = ? ");
    }
    sql.push_str(
        "ORDER BY CASE WHEN expires_on IS NULL THEN 1 ELSE 0 END, expires_on ASC, created_at ASC",
    );

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
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<Uuid, sqlx::Error> {
    let consume_request_id = Uuid::now_v7();
    let mut tx = db.pool.begin().await?;
    let now = now_utc_rfc3339();
    for c in consumption {
        let row =
            fetch_locked_batch_row(&mut tx, db.backend(), household_id, c.batch_id, "quantity")
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
        let existing: String = row.try_get("quantity")?;
        let cur = Decimal::from_str(&existing).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let applied = if c.quantity >= cur { cur } else { c.quantity };
        let new_qty = cur - applied;
        let depletes = new_qty <= Decimal::ZERO;

        if applied.is_zero() {
            continue;
        }

        insert_event(
            &mut tx,
            household_id,
            c.batch_id,
            EVENT_CONSUME,
            &(-applied).to_string(),
            Some("consumed via POST /stock/consume"),
            actor,
            Some(consume_request_id),
        )
        .await?;

        if depletes {
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
            sqlx::query(
                "UPDATE stock_batch SET quantity = ?, depleted_at = NULL WHERE id = ? AND household_id = ?",
            )
            .bind(new_qty.to_string())
            .bind(c.batch_id.to_string())
            .bind(household_id.to_string())
            .execute(&mut *tx)
            .await?;
        }

        if let Some(policy) = reminder_policy {
            crate::reminders::sync_expiry_for_batch_tx(&mut tx, c.batch_id, policy).await?;
        }
    }
    tx.commit().await?;
    Ok(consume_request_id)
}

#[allow(clippy::too_many_arguments)]
pub async fn split_repack(
    db: &Database,
    household_id: Uuid,
    source_batch_id: Uuid,
    operation_id: Option<Uuid>,
    used_quantity: &str,
    opened_on: &str,
    operation_note: Option<&str>,
    remainders: &[SplitRemainder],
    actor: Uuid,
    reminder_policy: Option<&ExpiryReminderPolicy>,
) -> Result<SplitStockResult, SplitStockError> {
    if remainders.is_empty() {
        return Err(SplitStockError::EmptyRemainders);
    }
    let used = Decimal::from_str(used_quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    if used < Decimal::ZERO {
        return Err(SplitStockError::QuantityNotPositive);
    }

    let mut parsed_remainders = Vec::with_capacity(remainders.len());
    for remainder in remainders {
        let quantity =
            Decimal::from_str(&remainder.quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        if quantity <= Decimal::ZERO {
            return Err(SplitStockError::QuantityNotPositive);
        }
        parsed_remainders.push(quantity);
    }

    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;

    let row = fetch_locked_batch_row(
        &mut tx,
        db.backend(),
        household_id,
        source_batch_id,
        "id, household_id, product_id, location_id, storage_vessel_id, source_batch_id, \
         source_operation_id, initial_quantity, quantity, unit, package_quantity, package_unit, \
         produced_on, expires_on, opened_on, note, created_at, created_by, depleted_at",
    )
    .await?
    .ok_or(SplitStockError::NotFound)?;
    let source = row_to_batch(row)?;
    if source.depleted_at.is_some() {
        return Err(SplitStockError::Depleted);
    }
    let operation_id = match operation_id {
        Some(operation_id) => {
            let row = sqlx::query(
                "SELECT id FROM stock_event \
                 WHERE household_id = ? AND batch_id = ? AND operation_id = ? \
                 LIMIT 1",
            )
            .bind(household_id.to_string())
            .bind(source_batch_id.to_string())
            .bind(operation_id.to_string())
            .fetch_optional(&mut *tx)
            .await?;
            if row.is_none() {
                return Err(SplitStockError::InvalidOperation);
            }
            operation_id
        }
        None => Uuid::now_v7(),
    };

    let current =
        Decimal::from_str(&source.quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let remainder_total: Decimal = parsed_remainders.iter().copied().sum();
    let transferred_total = used + remainder_total;
    if transferred_total > current {
        return Err(SplitStockError::InvalidTotal);
    }
    let remaining = current - transferred_total;
    let depletes = remaining <= Decimal::ZERO;

    if !used.is_zero() {
        insert_event_with_operation(
            &mut tx,
            household_id,
            source_batch_id,
            EVENT_CONSUME,
            &(-used).to_string(),
            operation_note.or(Some("used during split/repack")),
            actor,
            None,
            Some(operation_id),
        )
        .await?;
    }

    insert_event_with_operation(
        &mut tx,
        household_id,
        source_batch_id,
        EVENT_REPACK_OUT,
        &(-remainder_total).to_string(),
        operation_note.or(Some("repacked into remainder batches")),
        actor,
        None,
        Some(operation_id),
    )
    .await?;

    if depletes {
        sqlx::query(
            "UPDATE stock_batch SET quantity = '0', opened_on = COALESCE(opened_on, ?), depleted_at = ? \
             WHERE id = ? AND household_id = ?",
        )
        .bind(opened_on)
        .bind(&now)
        .bind(source_batch_id.to_string())
        .bind(household_id.to_string())
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "UPDATE stock_batch SET quantity = ?, opened_on = COALESCE(opened_on, ?), depleted_at = NULL \
             WHERE id = ? AND household_id = ?",
        )
        .bind(remaining.to_string())
        .bind(opened_on)
        .bind(source_batch_id.to_string())
        .bind(household_id.to_string())
        .execute(&mut *tx)
        .await?;
    }

    let mut remainder_ids = Vec::with_capacity(remainders.len());
    for (remainder, quantity) in remainders.iter().zip(parsed_remainders.iter()) {
        let remainder_id = Uuid::now_v7();
        remainder_ids.push(remainder_id);
        sqlx::query(
            "INSERT INTO stock_batch \
             (id, household_id, product_id, location_id, storage_vessel_id, source_batch_id, \
              source_operation_id, initial_quantity, quantity, unit, package_quantity, package_unit, \
              produced_on, expires_on, opened_on, note, created_at, created_by, depleted_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(remainder_id.to_string())
        .bind(household_id.to_string())
        .bind(source.product_id.to_string())
        .bind(remainder.location_id.to_string())
        .bind(remainder.storage_vessel_id.map(|id| id.to_string()))
        .bind(source_batch_id.to_string())
        .bind(operation_id.to_string())
        .bind(quantity.to_string())
        .bind(quantity.to_string())
        .bind(&source.unit)
        .bind(source.package_quantity.as_deref())
        .bind(source.package_unit.as_deref())
        .bind(source.produced_on.as_deref())
        .bind(remainder.expires_on.as_deref())
        .bind(opened_on)
        .bind(remainder.note.as_deref())
        .bind(&now)
        .bind(actor.to_string())
        .execute(&mut *tx)
        .await?;

        insert_event_with_operation(
            &mut tx,
            household_id,
            remainder_id,
            EVENT_REPACK_IN,
            &quantity.to_string(),
            remainder.note.as_deref().or(Some("repacked remainder")),
            actor,
            None,
            Some(operation_id),
        )
        .await?;

        if let Some(policy) = reminder_policy {
            crate::reminders::sync_expiry_for_batch_tx(&mut tx, remainder_id, policy).await?;
        }
    }

    if let Some(policy) = reminder_policy {
        crate::reminders::sync_expiry_for_batch_tx(&mut tx, source_batch_id, policy).await?;
    }

    tx.commit().await?;

    let source = get(db, household_id, source_batch_id)
        .await?
        .ok_or(SplitStockError::NotFound)?;
    let mut remainder_rows = Vec::with_capacity(remainder_ids.len());
    for id in remainder_ids {
        remainder_rows.push(
            get(db, household_id, id)
                .await?
                .ok_or(SplitStockError::NotFound)?,
        );
    }
    Ok(SplitStockResult {
        source,
        remainders: remainder_rows,
        operation_id,
    })
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

/// Reinterpret active count stock as retail packages after a product family
/// correction. One `piece` becomes one package of `package_quantity package_unit`.
pub async fn convert_active_piece_stock_to_package_unit(
    db: &Database,
    product_id: Uuid,
    package_quantity: &str,
    package_unit: &str,
) -> Result<(), sqlx::Error> {
    let multiplier =
        Decimal::from_str(package_quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let mut tx = db.pool.begin().await?;
    let mut sql = "SELECT id, initial_quantity, quantity FROM stock_batch \
                   WHERE product_id = ? AND depleted_at IS NULL AND unit = 'piece'"
        .to_owned();
    if db.backend() == Backend::Postgres {
        sql.push_str(" FOR UPDATE");
    }
    let batches = sqlx::query(&sql)
        .bind(product_id.to_string())
        .fetch_all(&mut *tx)
        .await?;

    for batch in batches {
        let batch_id: String = batch.try_get("id")?;
        let initial_quantity: String = batch.try_get("initial_quantity")?;
        let quantity: String = batch.try_get("quantity")?;
        let corrected_initial = scale_quantity(&initial_quantity, multiplier)?;
        let corrected_quantity = scale_quantity(&quantity, multiplier)?;

        sqlx::query(
            "UPDATE stock_batch \
             SET initial_quantity = ?, quantity = ?, unit = ?, package_quantity = ?, package_unit = ? \
             WHERE id = ?",
        )
        .bind(corrected_initial)
        .bind(corrected_quantity)
        .bind(package_unit)
        .bind(package_quantity)
        .bind(package_unit)
        .bind(&batch_id)
        .execute(&mut *tx)
        .await?;

        let events = sqlx::query("SELECT id, quantity_delta FROM stock_event WHERE batch_id = ?")
            .bind(&batch_id)
            .fetch_all(&mut *tx)
            .await?;
        for event in events {
            let event_id: String = event.try_get("id")?;
            let quantity_delta: String = event.try_get("quantity_delta")?;
            let corrected_delta = scale_quantity(&quantity_delta, multiplier)?;
            sqlx::query(
                "UPDATE stock_event \
                 SET quantity_delta = ?, package_quantity = ?, package_unit = ? \
                 WHERE id = ?",
            )
            .bind(corrected_delta)
            .bind(package_quantity)
            .bind(package_unit)
            .bind(event_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await
}

// ----- internals -----

fn scale_quantity(quantity: &str, multiplier: Decimal) -> Result<String, sqlx::Error> {
    let parsed = Decimal::from_str(quantity).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    Ok((parsed * multiplier).normalize().to_string())
}

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
    insert_event_with_operation(
        tx,
        household_id,
        batch_id,
        event_type,
        delta,
        note,
        created_by,
        consume_request_id,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn insert_event_with_operation(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    batch_id: Uuid,
    event_type: &str,
    delta: &str,
    note: Option<&str>,
    created_by: Uuid,
    consume_request_id: Option<Uuid>,
    operation_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO stock_event (id, household_id, batch_id, event_type, quantity_delta, package_quantity, package_unit, note, created_at, created_by, consume_request_id, operation_id) \
         VALUES (?, ?, ?, ?, ?, (SELECT package_quantity FROM stock_batch WHERE id = ?), (SELECT package_unit FROM stock_batch WHERE id = ?), ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(batch_id.to_string())
    .bind(event_type)
    .bind(delta)
    .bind(batch_id.to_string())
    .bind(batch_id.to_string())
    .bind(note)
    .bind(now_utc_rfc3339())
    .bind(created_by.to_string())
    .bind(consume_request_id.map(|u| u.to_string()))
    .bind(operation_id.map(|u| u.to_string()))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn fetch_locked_batch_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    household_id: Uuid,
    id: Uuid,
    columns: &str,
) -> Result<Option<sqlx::any::AnyRow>, sqlx::Error> {
    let mut sql = format!("SELECT {columns} FROM stock_batch WHERE id = ? AND household_id = ?");
    if backend == Backend::Postgres {
        sql.push_str(" FOR UPDATE");
    }
    sqlx::query(&sql)
        .bind(id.to_string())
        .bind(household_id.to_string())
        .fetch_optional(&mut **tx)
        .await
}

fn row_to_batch(row: sqlx::any::AnyRow) -> Result<StockBatchRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let household_id: String = row.try_get("household_id")?;
    let product_id: String = row.try_get("product_id")?;
    let location_id: String = row.try_get("location_id")?;
    let storage_vessel_id: Option<String> = row.try_get("storage_vessel_id")?;
    let source_batch_id: Option<String> = row.try_get("source_batch_id")?;
    let source_operation_id: Option<String> = row.try_get("source_operation_id")?;
    let created_by: String = row.try_get("created_by")?;
    Ok(StockBatchRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&household_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        product_id: Uuid::parse_str(&product_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        location_id: Uuid::parse_str(&location_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        storage_vessel_id: storage_vessel_id
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        source_batch_id: source_batch_id
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        source_operation_id: source_operation_id
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        initial_quantity: row.try_get("initial_quantity")?,
        quantity: row.try_get("quantity")?,
        unit: row.try_get("unit")?,
        package_quantity: row.try_get("package_quantity")?,
        package_unit: row.try_get("package_unit")?,
        produced_on: row.try_get("produced_on")?,
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
        storage_vessel_id: row
            .try_get::<Option<String>, _>("s_storage_vessel_id")?
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        source_batch_id: row
            .try_get::<Option<String>, _>("s_source_batch_id")?
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        source_operation_id: row
            .try_get::<Option<String>, _>("s_source_operation_id")?
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        initial_quantity: row.try_get("s_initial_quantity")?,
        quantity: row.try_get("s_quantity")?,
        unit: row.try_get("s_unit")?,
        package_quantity: row.try_get("s_package_quantity")?,
        package_unit: row.try_get("s_package_unit")?,
        produced_on: row.try_get("s_produced_on")?,
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
        package_quantity: row.try_get("p_package_quantity")?,
        package_unit: row.try_get("p_package_unit")?,
        package_size_local_override: false,
        off_name: None,
        off_brand: None,
        off_package_quantity: None,
        off_package_unit: None,
        name_local_override: false,
        brand_local_override: false,
        family_local_override: false,
        fetched_at: row.try_get("p_fetched_at")?,
        created_by_household_id: p_household
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        created_at: row.try_get("p_created_at")?,
        deleted_at: row.try_get("p_deleted_at")?,
        max_open_days: row.try_get("p_max_open_days")?,
    };
    let vessel_id: Option<String> = row.try_get("v_id")?;
    let storage_vessel = match vessel_id {
        Some(id) => Some(StorageVesselRow {
            id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            household_id: uuid_from(&row, "v_household_id")?,
            name: row.try_get("v_name")?,
            tare_weight: row.try_get("v_tare_weight")?,
            tare_unit: row.try_get("v_tare_unit")?,
            sort_order: row.try_get("v_sort_order")?,
            created_at: row.try_get("v_created_at")?,
            updated_at: row.try_get("v_updated_at")?,
        }),
        None => None,
    };
    Ok(StockBatchWithProduct {
        batch,
        product,
        location_name: row.try_get("l_name")?,
        storage_vessel,
    })
}

fn uuid_from(row: &sqlx::any::AnyRow, col: &str) -> Result<Uuid, sqlx::Error> {
    let s: String = row.try_get(col)?;
    Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{households, locations, memberships, products, stock_events, users};
    use tokio::{
        sync::{Barrier, Notify},
        time::{sleep, Duration},
    };

    async fn setup_with_db(db: &Database) -> (Uuid, Uuid, Uuid, Uuid) {
        let h = households::create(db, "h", "UTC").await.unwrap();
        let u = users::create(db, "u@example.com", "User", "hash")
            .await
            .unwrap();
        memberships::insert(db, h.id, u.id, "admin").await.unwrap();
        locations::seed_defaults(db, h.id).await.unwrap();
        let locs = locations::list_for_household(db, h.id).await.unwrap();
        let pantry = locs.iter().find(|l| l.kind == "pantry").unwrap().id;
        let p = products::create_manual(db, h.id, "Flour", None, "mass", Some("g"), None, None)
            .await
            .unwrap();
        (h.id, u.id, pantry, p.id)
    }

    async fn setup() -> (Database, Uuid, Uuid, Uuid, Uuid) {
        let db = crate::test_db().await;
        let (hid, uid, lid, pid) = setup_with_db(&db).await;
        (db, hid, uid, lid, pid)
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
    async fn split_repack_closes_source_and_creates_remainder_batches() {
        let (db, hid, uid, pantry, pid) = setup().await;
        let fridge = locations::list_for_household(&db, hid)
            .await
            .unwrap()
            .into_iter()
            .find(|l| l.kind == "fridge")
            .unwrap()
            .id;
        let batch = create(
            &db,
            hid,
            pid,
            pantry,
            "500",
            "g",
            None,
            Some("2026-12-31"),
            None,
            None,
            uid,
            None,
        )
        .await
        .unwrap();

        let result = split_repack(
            &db,
            hid,
            batch.id,
            None,
            "125",
            "2026-05-01",
            Some("stored in bowl"),
            &[
                SplitRemainder {
                    location_id: fridge,
                    storage_vessel_id: None,
                    quantity: "200".into(),
                    expires_on: Some("2026-05-04".into()),
                    note: Some("stored in bowl A".into()),
                },
                SplitRemainder {
                    location_id: fridge,
                    storage_vessel_id: None,
                    quantity: "175".into(),
                    expires_on: Some("2026-05-04".into()),
                    note: Some("stored in bowl B".into()),
                },
            ],
            uid,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.source.quantity, "0");
        assert!(result.source.depleted_at.is_some());
        assert_eq!(result.remainders.len(), 2);
        assert_eq!(result.remainders[0].location_id, fridge);
        assert_eq!(result.remainders[0].quantity, "200");
        assert_eq!(result.remainders[0].initial_quantity, "200");
        assert_eq!(
            result.remainders[0].opened_on.as_deref(),
            Some("2026-05-01")
        );
        assert_eq!(
            result.remainders[0].expires_on.as_deref(),
            Some("2026-05-04")
        );
        assert_eq!(result.remainders[0].source_batch_id, Some(batch.id));
        assert_eq!(
            result.remainders[0].source_operation_id,
            Some(result.operation_id)
        );
        assert_eq!(result.remainders[1].quantity, "175");
        assert_eq!(balance_from_events(&db, batch.id).await, Decimal::ZERO);
        assert_eq!(
            balance_from_events(&db, result.remainders[0].id).await,
            Decimal::from(200)
        );

        let source_events = stock_events::list_for_batch(&db, batch.id).await.unwrap();
        assert_eq!(source_events.len(), 3);
        assert_eq!(source_events[1].event_type, "consume");
        assert_eq!(source_events[1].operation_id, Some(result.operation_id));
        assert_eq!(source_events[2].event_type, "repack_out");
        assert_eq!(source_events[2].operation_id, Some(result.operation_id));
        let remainder_events = stock_events::list_for_batch(&db, result.remainders[0].id)
            .await
            .unwrap();
        assert_eq!(remainder_events[0].event_type, "repack_in");
        assert_eq!(remainder_events[0].operation_id, Some(result.operation_id));
    }

    #[tokio::test]
    async fn split_repack_can_allocate_source_incrementally() {
        let (db, hid, uid, pantry, pid) = setup().await;
        let fridge = locations::list_for_household(&db, hid)
            .await
            .unwrap()
            .into_iter()
            .find(|l| l.kind == "fridge")
            .unwrap()
            .id;
        let batch = create(
            &db, hid, pid, pantry, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();

        let first = split_repack(
            &db,
            hid,
            batch.id,
            None,
            "125",
            "2026-05-01",
            Some("first weigh-in"),
            &[SplitRemainder {
                location_id: fridge,
                storage_vessel_id: None,
                quantity: "200".into(),
                expires_on: None,
                note: Some("first container".into()),
            }],
            uid,
            None,
        )
        .await
        .unwrap();

        assert_eq!(first.source.quantity, "175");
        assert!(first.source.depleted_at.is_none());
        assert_eq!(first.source.opened_on.as_deref(), Some("2026-05-01"));
        assert_eq!(first.remainders.len(), 1);
        assert_eq!(first.remainders[0].source_batch_id, Some(batch.id));
        assert_eq!(
            first.remainders[0].source_operation_id,
            Some(first.operation_id)
        );
        assert_eq!(balance_from_events(&db, batch.id).await, Decimal::from(175));

        let err = split_repack(
            &db,
            hid,
            batch.id,
            Some(Uuid::now_v7()),
            "0",
            "2026-05-02",
            None,
            &[SplitRemainder {
                location_id: fridge,
                storage_vessel_id: None,
                quantity: "1".into(),
                expires_on: None,
                note: None,
            }],
            uid,
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, SplitStockError::InvalidOperation));

        let second = split_repack(
            &db,
            hid,
            batch.id,
            Some(first.operation_id),
            "0",
            "2026-05-02",
            Some("second weigh-in"),
            &[SplitRemainder {
                location_id: fridge,
                storage_vessel_id: None,
                quantity: "175".into(),
                expires_on: None,
                note: Some("second container".into()),
            }],
            uid,
            None,
        )
        .await
        .unwrap();

        assert_eq!(second.operation_id, first.operation_id);
        assert_eq!(second.source.quantity, "0");
        assert!(second.source.depleted_at.is_some());
        assert_eq!(second.source.opened_on.as_deref(), Some("2026-05-01"));
        assert_eq!(
            second.remainders[0].source_operation_id,
            Some(first.operation_id)
        );
        assert_eq!(balance_from_events(&db, batch.id).await, Decimal::ZERO);

        let source_events = stock_events::list_for_batch(&db, batch.id).await.unwrap();
        let operation_events: Vec<_> = source_events
            .iter()
            .filter(|event| event.operation_id == Some(first.operation_id))
            .collect();
        assert_eq!(operation_events.len(), 3);
        assert_eq!(
            operation_events
                .iter()
                .filter(|event| event.event_type == EVENT_CONSUME)
                .count(),
            1
        );
        assert_eq!(
            operation_events
                .iter()
                .filter(|event| event.event_type == EVENT_REPACK_OUT)
                .count(),
            2
        );
    }

    async fn assert_stock_ledger_parity(db: &Database) {
        let (hid, uid, lid, pid) = setup_with_db(db).await;
        let batch = create(
            db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();

        assert_eq!(balance_from_events(db, batch.id).await, Decimal::from(500));

        let adjusted = adjust(db, hid, batch.id, "300", uid, None, None)
            .await
            .unwrap();
        assert_eq!(adjusted.quantity, "300");
        assert_eq!(balance_from_events(db, batch.id).await, Decimal::from(300));

        let refs = vec![adjusted.to_batch_ref().unwrap()];
        let plan = qm_core::batch::plan_consumption(refs, Decimal::from(100), "g").unwrap();
        apply_consumption(db, hid, &plan, uid, None).await.unwrap();
        assert_eq!(balance_from_events(db, batch.id).await, Decimal::from(200));

        discard(db, hid, batch.id, uid, None, None).await.unwrap();
        assert_eq!(balance_from_events(db, batch.id).await, Decimal::ZERO);

        let restored = restore(db, hid, batch.id, uid, None).await.unwrap();
        assert_eq!(restored.quantity, "200");
        assert_eq!(balance_from_events(db, batch.id).await, Decimal::from(200));

        let other = create(
            db, hid, pid, lid, "125", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        discard(db, hid, batch.id, uid, None, None).await.unwrap();
        discard(db, hid, other.id, uid, None, None).await.unwrap();
        let restored_many = restore_many(db, hid, &[batch.id, other.id], uid, None)
            .await
            .unwrap();
        assert_eq!(restored_many.len(), 2);
        assert_eq!(balance_from_events(db, batch.id).await, Decimal::from(200));
        assert_eq!(balance_from_events(db, other.id).await, Decimal::from(125));
    }

    #[tokio::test]
    async fn create_writes_add_event() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let events = stock_events::list_for_batch(&db, b.id).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "add");
        assert_eq!(events[0].quantity_delta, "500");
        assert_eq!(b.initial_quantity, "500");
        assert_eq!(b.quantity, "500");
    }

    #[tokio::test]
    async fn package_size_is_snapshotted_on_batch_and_events() {
        let (db, hid, uid, lid, _pid) = setup().await;
        let product = products::upsert_from_off(
            &db,
            hid,
            "1234567890123",
            "Beans",
            None,
            "mass",
            Some("g"),
            None,
            Some("400"),
            Some("g"),
        )
        .await
        .unwrap();

        let batch = create(
            &db, hid, product.id, lid, "1600", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        assert_eq!(batch.package_quantity.as_deref(), Some("400"));
        assert_eq!(batch.package_unit.as_deref(), Some("g"));

        products::upsert_from_off(
            &db,
            hid,
            "1234567890123",
            "Beans",
            None,
            "mass",
            Some("g"),
            None,
            Some("300"),
            Some("g"),
        )
        .await
        .unwrap();

        let refreshed_batch = get(&db, hid, batch.id).await.unwrap().unwrap();
        assert_eq!(refreshed_batch.package_quantity.as_deref(), Some("400"));
        assert_eq!(refreshed_batch.package_unit.as_deref(), Some("g"));

        let refs = vec![refreshed_batch.to_batch_ref().unwrap()];
        let plan = qm_core::batch::plan_consumption(refs, Decimal::from(400), "g").unwrap();
        apply_consumption(&db, hid, &plan, uid, None).await.unwrap();

        let events = stock_events::list_for_batch(&db, batch.id).await.unwrap();
        assert_eq!(events[0].package_quantity.as_deref(), Some("400"));
        assert_eq!(events[0].package_unit.as_deref(), Some("g"));
        assert_eq!(events[1].event_type, "consume");
        assert_eq!(events[1].package_quantity.as_deref(), Some("400"));
        assert_eq!(events[1].package_unit.as_deref(), Some("g"));
    }

    #[tokio::test]
    async fn adjust_writes_adjust_event_and_updates_cache() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();

        let after = adjust(&db, hid, b.id, "300", uid, None, None)
            .await
            .unwrap();
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
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let after = adjust(&db, hid, b.id, "0", uid, None, None).await.unwrap();
        assert_eq!(after.quantity, "0");
        assert!(after.depleted_at.is_some());
    }

    #[tokio::test]
    async fn discard_writes_event_and_marks_depleted_without_deleting_row() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let removed = discard(&db, hid, b.id, uid, None, None).await.unwrap();
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
    async fn restore_after_discard() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        discard(&db, hid, b.id, uid, None, None).await.unwrap();

        let restored = restore(&db, hid, b.id, uid, None).await.expect("restore");
        assert_eq!(restored.quantity, "500");
        assert!(restored.depleted_at.is_none());

        let events = stock_events::list_for_batch(&db, b.id).await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, "add");
        assert_eq!(events[1].event_type, "discard");
        assert_eq!(events[2].event_type, "restore");
        assert_eq!(events[2].quantity_delta, "500");

        // Ledger sum matches cached quantity.
        assert_eq!(balance_from_events(&db, b.id).await, Decimal::from(500));
    }

    #[tokio::test]
    async fn restore_rejects_when_last_event_isnt_discard() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        // Fully consume the batch via apply_consumption.
        let refs = vec![b.to_batch_ref().unwrap()];
        let plan = qm_core::batch::plan_consumption(refs, Decimal::from(500), "g").unwrap();
        apply_consumption(&db, hid, &plan, uid, None).await.unwrap();

        let err = restore(&db, hid, b.id, uid, None)
            .await
            .err()
            .expect("should fail");
        assert!(matches!(err, RestoreError::NotRestorable));
    }

    #[tokio::test]
    async fn restore_rejects_after_double_restore() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        discard(&db, hid, b.id, uid, None, None).await.unwrap();
        restore(&db, hid, b.id, uid, None)
            .await
            .expect("first restore");

        let err = restore(&db, hid, b.id, uid, None)
            .await
            .err()
            .expect("should fail");
        assert!(matches!(err, RestoreError::NotRestorable));
    }

    #[tokio::test]
    async fn restore_many_atomic_success() {
        let (db, hid, uid, lid, pid) = setup().await;
        let a = create(
            &db, hid, pid, lid, "100", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let b = create(
            &db, hid, pid, lid, "200", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        discard(&db, hid, a.id, uid, None, None).await.unwrap();
        discard(&db, hid, b.id, uid, None, None).await.unwrap();

        let restored = restore_many(&db, hid, &[a.id, b.id], uid, None)
            .await
            .expect("restore_many");
        assert_eq!(restored.len(), 2);
        for row in restored {
            assert!(row.depleted_at.is_none());
        }
        for batch_id in [a.id, b.id] {
            let events = stock_events::list_for_batch(&db, batch_id).await.unwrap();
            assert_eq!(events.last().unwrap().event_type, "restore");
        }
    }

    #[tokio::test]
    async fn restore_many_rolls_back_when_one_isnt_discardable() {
        let (db, hid, uid, lid, pid) = setup().await;
        // Batch A has been discarded (restorable). Batch B is still active
        // (not restorable). Asking for both should leave both untouched.
        let a = create(
            &db, hid, pid, lid, "100", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let b = create(
            &db, hid, pid, lid, "200", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        discard(&db, hid, a.id, uid, None, None).await.unwrap();

        let err = restore_many(&db, hid, &[a.id, b.id], uid, None)
            .await
            .err()
            .expect("should fail");
        match err {
            RestoreError::NotRestorableMany(ids) => {
                // Only B is unrestorable; A is, but we rolled back because of B.
                assert_eq!(ids, vec![b.id]);
            }
            other => panic!("expected NotRestorableMany, got {other:?}"),
        }

        // A remains discarded (still 0, depleted_at set). No stray events.
        let a_after = get(&db, hid, a.id).await.unwrap().unwrap();
        assert!(a_after.depleted_at.is_some(), "A should still be discarded");
        let a_events = stock_events::list_for_batch(&db, a.id).await.unwrap();
        assert_eq!(
            a_events.len(),
            2,
            "A should have only add + discard, no restore"
        );

        // B untouched.
        let b_after = get(&db, hid, b.id).await.unwrap().unwrap();
        assert_eq!(b_after.quantity, "200");
        assert!(b_after.depleted_at.is_none());
    }

    #[tokio::test]
    async fn restore_many_reports_every_unrestorable_id() {
        let (db, hid, uid, lid, pid) = setup().await;
        // Two active batches (neither discarded) — both should show up in
        // the unrestorable list, not just the first.
        let a = create(
            &db, hid, pid, lid, "100", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let b = create(
            &db, hid, pid, lid, "200", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();

        let err = restore_many(&db, hid, &[a.id, b.id], uid, None)
            .await
            .err()
            .expect("should fail");
        match err {
            RestoreError::NotRestorableMany(ids) => {
                assert_eq!(ids.len(), 2);
                assert!(ids.contains(&a.id));
                assert!(ids.contains(&b.id));
            }
            other => panic!("expected NotRestorableMany, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn apply_consumption_correlates_events() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b1 = create(
            &db,
            hid,
            pid,
            lid,
            "500",
            "g",
            None,
            Some("2026-05-01"),
            None,
            None,
            uid,
            None,
        )
        .await
        .unwrap();
        let b2 = create(
            &db,
            hid,
            pid,
            lid,
            "500",
            "g",
            None,
            Some("2026-06-01"),
            None,
            None,
            uid,
            None,
        )
        .await
        .unwrap();

        let batches = list_active_batches(&db, hid, pid, None).await.unwrap();
        let refs: Vec<_> = batches.iter().map(|b| b.to_batch_ref().unwrap()).collect();
        let plan = qm_core::batch::plan_consumption(refs, Decimal::from(750), "g").unwrap();
        let request_id = apply_consumption(&db, hid, &plan, uid, None).await.unwrap();

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

        create(
            &db,
            hid,
            pid,
            pantry,
            "100",
            "g",
            None,
            Some("2026-05-01"),
            None,
            None,
            uid,
            None,
        )
        .await
        .unwrap();
        create(
            &db,
            hid,
            pid,
            fridge,
            "200",
            "g",
            None,
            Some("2026-07-01"),
            None,
            None,
            uid,
            None,
        )
        .await
        .unwrap();

        let in_pantry = list(
            &db,
            hid,
            &StockFilter {
                location_id: Some(pantry),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(in_pantry.len(), 1);
        assert_eq!(in_pantry[0].batch.location_id, pantry);

        let expiring = list(
            &db,
            hid,
            &StockFilter {
                expiring_before: Some("2026-06-01".parse().unwrap()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(expiring.len(), 1);
        assert_eq!(expiring[0].batch.location_id, pantry);
    }

    #[tokio::test]
    async fn metadata_update_does_not_write_events() {
        let (db, hid, uid, lid, pid) = setup().await;
        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let locs = locations::list_for_household(&db, hid).await.unwrap();
        let fridge = locs.iter().find(|l| l.kind == "fridge").unwrap().id;

        let after = update_metadata(
            &db,
            hid,
            b.id,
            &StockMetadataUpdate {
                location_id: Some(fridge),
                note: Some(Some("moved to fridge")),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
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

        let b = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        assert!(has_active_stock_for_product(&db, pid).await.unwrap());

        let conflicts = conflicting_units_for_family_change(&db, pid, "volume")
            .await
            .unwrap();
        assert_eq!(conflicts, vec!["g".to_string()]);

        let none = conflicting_units_for_family_change(&db, pid, "mass")
            .await
            .unwrap();
        assert!(none.is_empty());

        discard(&db, hid, b.id, uid, None, None).await.unwrap();
        assert!(!has_active_stock_for_product(&db, pid).await.unwrap());
    }

    #[tokio::test]
    async fn stock_ledger_parity_matches_on_sqlite() {
        let db = crate::test_db().await;
        assert_stock_ledger_parity(&db).await;
    }

    #[tokio::test]
    async fn stock_ledger_parity_matches_on_postgres() {
        let Some(test_db) = crate::test_support::postgres().await else {
            return;
        };
        assert_stock_ledger_parity(test_db.db()).await;
    }

    #[tokio::test]
    async fn restore_many_rollback_matches_on_postgres() {
        let Some(test_db) = crate::test_support::postgres().await else {
            return;
        };
        let db = test_db.db();
        let (hid, uid, lid, pid) = setup_with_db(db).await;
        let a = create(
            db, hid, pid, lid, "100", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        let b = create(
            db, hid, pid, lid, "200", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();
        discard(db, hid, a.id, uid, None, None).await.unwrap();

        let err = restore_many(db, hid, &[a.id, b.id], uid, None)
            .await
            .err()
            .unwrap();
        match err {
            RestoreError::NotRestorableMany(ids) => assert_eq!(ids, vec![b.id]),
            other => panic!("expected NotRestorableMany, got {other:?}"),
        }
        assert_eq!(balance_from_events(db, a.id).await, Decimal::ZERO);
        assert_eq!(balance_from_events(db, b.id).await, Decimal::from(200));
    }

    #[tokio::test]
    async fn postgres_row_locking_serializes_overlapping_adjusts() {
        let Some(test_db) = crate::test_support::postgres().await else {
            return;
        };
        let db = test_db.db().clone();
        let (hid, uid, lid, pid) = setup_with_db(&db).await;
        let batch = create(
            &db, hid, pid, lid, "500", "g", None, None, None, None, uid, None,
        )
        .await
        .unwrap();

        let locked = Arc::new(Barrier::new(2));
        let release = Arc::new(Notify::new());

        let db1 = db.clone();
        let locked1 = locked.clone();
        let release1 = release.clone();
        let t1 = tokio::spawn(async move {
            let mut tx = db1.pool.begin().await.unwrap();
            let current = fetch_locked_batch_row(&mut tx, db1.backend(), hid, batch.id, "quantity")
                .await
                .unwrap()
                .unwrap();
            let current_qty =
                Decimal::from_str(&current.try_get::<String, _>("quantity").unwrap()).unwrap();
            locked1.wait().await;
            release1.notified().await;

            let new_qty = Decimal::from(400);
            let delta = new_qty - current_qty;
            let depleted_at = if new_qty.is_zero() {
                Some(now_utc_rfc3339())
            } else {
                None
            };
            sqlx::query("UPDATE stock_batch SET quantity = ?, depleted_at = ? WHERE id = ? AND household_id = ?")
                .bind(new_qty.to_string())
                .bind(depleted_at)
                .bind(batch.id.to_string())
                .bind(hid.to_string())
                .execute(&mut *tx)
                .await
                .unwrap();
            insert_event(
                &mut tx,
                hid,
                batch.id,
                EVENT_ADJUST,
                &delta.to_string(),
                None,
                uid,
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        });

        let db2 = db.clone();
        let locked2 = locked.clone();
        let t2 = tokio::spawn(async move {
            locked2.wait().await;
            adjust(&db2, hid, batch.id, "300", uid, None, None)
                .await
                .unwrap()
        });

        sleep(Duration::from_millis(100)).await;
        release.notify_one();

        t1.await.unwrap();
        let final_batch = t2.await.unwrap();

        assert_eq!(final_batch.quantity, "300");
        assert_eq!(balance_from_events(&db, batch.id).await, Decimal::from(300));
        let events = stock_events::list_for_batch(&db, batch.id).await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].quantity_delta, "-100");
        assert_eq!(events[2].quantity_delta, "-100");
    }
}
