//! Read-side helpers over the append-only `stock_event` ledger.
//!
//! Two shapes of read:
//! - `list_for_batch` / `list_for_household` — plain event rows, used by tests
//!   and anyone who already has product/batch context.
//! - `list_timeline` — pre-joined to stock_batch / product / users so the
//!   HTTP history endpoints don't N+1. This is what the UI timeline reads.

use sqlx::Row;
use uuid::Uuid;

use crate::Database;

pub const EVENT_ADD: &str = "add";
pub const EVENT_CONSUME: &str = "consume";
pub const EVENT_ADJUST: &str = "adjust";
pub const EVENT_DISCARD: &str = "discard";
pub const EVENT_RESTORE: &str = "restore";

#[derive(Debug, Clone)]
pub struct StockEventRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub batch_id: Uuid,
    pub event_type: String,
    pub quantity_delta: String,
    pub note: Option<String>,
    pub created_at: String,
    pub created_by: Uuid,
    pub consume_request_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct TimelineEntryRow {
    pub event: StockEventRow,
    /// The batch's current unit — the event's `quantity_delta` is expressed
    /// in this unit. Included for display so the UI doesn't have to fetch
    /// the batch separately.
    pub batch_unit: String,
    /// Joined product for display. Includes soft-deleted products so the
    /// history timeline remains intact after a product is removed.
    pub product: crate::products::ProductRow,
    /// Actor's username (LEFT JOIN so a deleted actor still shows the event).
    pub created_by_username: Option<String>,
}

pub async fn list_for_batch(
    db: &Database,
    batch_id: Uuid,
) -> Result<Vec<StockEventRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, household_id, batch_id, event_type, quantity_delta, note, \
                created_at, created_by, consume_request_id \
         FROM stock_event \
         WHERE batch_id = ? \
         ORDER BY created_at ASC, id ASC",
    )
    .bind(batch_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_event).collect()
}

pub async fn list_for_household(
    db: &Database,
    household_id: Uuid,
    limit: i64,
) -> Result<Vec<StockEventRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, household_id, batch_id, event_type, quantity_delta, note, \
                created_at, created_by, consume_request_id \
         FROM stock_event \
         WHERE household_id = ? \
         ORDER BY created_at DESC, id DESC \
         LIMIT ?",
    )
    .bind(household_id.to_string())
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_event).collect()
}

/// Household-scoped timeline, optionally narrowed to a single batch. Drives
/// both `GET /stock/events` and `GET /stock/{id}/events`.
///
/// Pagination: cursor-based on `created_at`. Pass `before_created_at` equal
/// to the last-seen event's `created_at` to fetch the next page. Tie-breaks
/// within a microsecond are handled by `id DESC` in the ORDER BY, which is
/// enough for our UUIDv7 ids — per-second ordering is monotonic.
pub async fn list_timeline(
    db: &Database,
    household_id: Uuid,
    batch_id: Option<Uuid>,
    before_created_at: Option<&str>,
    limit: i64,
) -> Result<Vec<TimelineEntryRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT \
            e.id AS e_id, e.household_id AS e_household_id, e.batch_id AS e_batch_id, \
            e.event_type AS e_event_type, e.quantity_delta AS e_quantity_delta, \
            e.note AS e_note, e.created_at AS e_created_at, e.created_by AS e_created_by, \
            e.consume_request_id AS e_consume_request_id, \
            b.unit AS b_unit, \
            p.id AS p_id, p.source AS p_source, p.off_barcode AS p_off_barcode, \
            p.name AS p_name, p.brand AS p_brand, p.family AS p_family, \
            p.default_unit AS p_default_unit, p.image_url AS p_image_url, \
            p.fetched_at AS p_fetched_at, p.created_by_household_id AS p_created_by_household_id, \
            p.created_at AS p_created_at, p.deleted_at AS p_deleted_at, \
            u.username AS u_username \
         FROM stock_event e \
         INNER JOIN stock_batch b ON b.id = e.batch_id \
         INNER JOIN product p ON p.id = b.product_id \
         LEFT JOIN users u ON u.id = e.created_by \
         WHERE e.household_id = ? ",
    );
    if batch_id.is_some() {
        sql.push_str("AND e.batch_id = ? ");
    }
    if before_created_at.is_some() {
        sql.push_str("AND e.created_at < ? ");
    }
    sql.push_str("ORDER BY e.created_at DESC, e.id DESC LIMIT ?");

    let mut q = sqlx::query(&sql).bind(household_id.to_string());
    if let Some(bid) = batch_id {
        q = q.bind(bid.to_string());
    }
    if let Some(before) = before_created_at {
        q = q.bind(before);
    }
    q = q.bind(limit);

    let rows = q.fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_timeline_entry).collect()
}

/// Transactional: fetches the newest event for a batch. Used by the
/// `restore` path to decide whether the last action was a discard.
pub async fn latest_for_batch_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    batch_id: Uuid,
) -> Result<Option<StockEventRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, household_id, batch_id, event_type, quantity_delta, note, \
                created_at, created_by, consume_request_id \
         FROM stock_event \
         WHERE batch_id = ? \
         ORDER BY created_at DESC, id DESC \
         LIMIT 1",
    )
    .bind(batch_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(row_to_event).transpose()
}

fn row_to_event(row: sqlx::any::AnyRow) -> Result<StockEventRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    let household_id: String = row.try_get("household_id")?;
    let batch_id: String = row.try_get("batch_id")?;
    let created_by: String = row.try_get("created_by")?;
    let consume_request_id: Option<String> = row.try_get("consume_request_id")?;
    Ok(StockEventRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&household_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        batch_id: Uuid::parse_str(&batch_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        event_type: row.try_get("event_type")?,
        quantity_delta: row.try_get("quantity_delta")?,
        note: row.try_get("note")?,
        created_at: row.try_get("created_at")?,
        created_by: Uuid::parse_str(&created_by).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        consume_request_id: consume_request_id
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
    })
}

fn row_to_timeline_entry(row: sqlx::any::AnyRow) -> Result<TimelineEntryRow, sqlx::Error> {
    let e_id: String = row.try_get("e_id")?;
    let e_household: String = row.try_get("e_household_id")?;
    let e_batch: String = row.try_get("e_batch_id")?;
    let e_by: String = row.try_get("e_created_by")?;
    let e_corr: Option<String> = row.try_get("e_consume_request_id")?;

    let event = StockEventRow {
        id: Uuid::parse_str(&e_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        household_id: Uuid::parse_str(&e_household).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        batch_id: Uuid::parse_str(&e_batch).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        event_type: row.try_get("e_event_type")?,
        quantity_delta: row.try_get("e_quantity_delta")?,
        note: row.try_get("e_note")?,
        created_at: row.try_get("e_created_at")?,
        created_by: Uuid::parse_str(&e_by).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        consume_request_id: e_corr
            .map(|s| Uuid::parse_str(&s))
            .transpose()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
    };

    let p_id: String = row.try_get("p_id")?;
    let p_household: Option<String> = row.try_get("p_created_by_household_id")?;

    let product = crate::products::ProductRow {
        id: Uuid::parse_str(&p_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
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
        deleted_at: row.try_get("p_deleted_at")?,
    };

    Ok(TimelineEntryRow {
        event,
        batch_unit: row.try_get("b_unit")?,
        product,
        created_by_username: row.try_get("u_username")?,
    })
}
