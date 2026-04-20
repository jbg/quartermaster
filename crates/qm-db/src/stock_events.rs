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
    /// The batch's current expiry date (YYYY-MM-DD), if any. Lets the UI
    /// contextualise events with "expiring tomorrow" badges.
    pub batch_expires_on: Option<String>,
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
/// Pagination: pass `(before_created_at, before_id)` as a pair equal to the
/// last-seen row's `(created_at, id)`. The predicate
/// `created_at < X OR (created_at = X AND id < Y)` walks the compound order
/// key exactly once per page — no duplicates when two events land in the
/// same millisecond. Omitting `before_id` falls back to a plain
/// `created_at < ?`, preserving the older single-field cursor behaviour for
/// any caller that never paginates past page one.
pub async fn list_timeline(
    db: &Database,
    household_id: Uuid,
    batch_id: Option<Uuid>,
    before_created_at: Option<&str>,
    before_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<TimelineEntryRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT \
            e.id AS e_id, e.household_id AS e_household_id, e.batch_id AS e_batch_id, \
            e.event_type AS e_event_type, e.quantity_delta AS e_quantity_delta, \
            e.note AS e_note, e.created_at AS e_created_at, e.created_by AS e_created_by, \
            e.consume_request_id AS e_consume_request_id, \
            b.unit AS b_unit, b.expires_on AS b_expires_on, \
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
    match (before_created_at.is_some(), before_id.is_some()) {
        (true, true) => sql.push_str("AND (e.created_at < ? OR (e.created_at = ? AND e.id < ?)) "),
        (true, false) => sql.push_str("AND e.created_at < ? "),
        _ => {}
    }
    sql.push_str("ORDER BY e.created_at DESC, e.id DESC LIMIT ?");

    let mut q = sqlx::query(&sql).bind(household_id.to_string());
    if let Some(bid) = batch_id {
        q = q.bind(bid.to_string());
    }
    match (before_created_at, before_id) {
        (Some(c), Some(i)) => {
            q = q.bind(c).bind(c).bind(i.to_string());
        }
        (Some(c), None) => {
            q = q.bind(c);
        }
        _ => {}
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
        batch_expires_on: row.try_get("b_expires_on")?,
        product,
        created_by_username: row.try_get("u_username")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{households, locations, memberships, products, stock, users};

    async fn setup() -> (crate::Database, Uuid, Uuid, Uuid, Uuid) {
        let db = crate::test_db().await;
        let h = households::create(&db, "h").await.unwrap();
        let u = users::create(&db, "u", None, "hash").await.unwrap();
        memberships::insert(&db, h.id, u.id, "admin").await.unwrap();
        locations::seed_defaults(&db, h.id).await.unwrap();
        let locs = locations::list_for_household(&db, h.id).await.unwrap();
        let pantry = locs.iter().find(|l| l.kind == "pantry").unwrap().id;
        let p = products::create_manual(&db, h.id, "Flour", None, "mass", Some("g"), None)
            .await
            .unwrap();
        (db, h.id, u.id, pantry, p.id)
    }

    #[tokio::test]
    async fn list_timeline_includes_batch_expiry() {
        let (db, hid, uid, lid, pid) = setup().await;
        stock::create(&db, hid, pid, lid, "500", "g", Some("2026-06-01"), None, None, uid)
            .await
            .unwrap();
        stock::create(&db, hid, pid, lid, "200", "g", None, None, None, uid)
            .await
            .unwrap();

        let rows = list_timeline(&db, hid, None, None, None, 10).await.unwrap();
        // Two adds, newest first.
        assert_eq!(rows.len(), 2);
        // One carries the expiry; the other is None. Order depends on which was
        // inserted second (that one is newest and has no expiry).
        assert!(rows.iter().any(|r| r.batch_expires_on.as_deref() == Some("2026-06-01")));
        assert!(rows.iter().any(|r| r.batch_expires_on.is_none()));
    }

    #[tokio::test]
    async fn list_timeline_cursor_respects_id_tiebreak() {
        let (db, hid, uid, lid, pid) = setup().await;
        // Seed a single batch so we can hang hand-crafted events off it.
        let b = stock::create(&db, hid, pid, lid, "1", "g", None, None, None, uid)
            .await
            .unwrap();
        // Drop the create's `add` event so the fixture contains only the pair
        // we control below, keeping the test assertions simple.
        sqlx::query("DELETE FROM stock_event WHERE batch_id = ?")
            .bind(b.id.to_string())
            .execute(&db.pool)
            .await
            .unwrap();

        // Two events with identical `created_at` — the cursor must still
        // walk them without duplicates or gaps.
        let shared_ts = "2026-04-20T12:00:00.000Z";
        let id_a = Uuid::now_v7();
        let id_b = Uuid::now_v7(); // id_b > id_a under UUIDv7 time-ordering
        insert_raw(&db, id_a, hid, b.id, "adjust", "1", shared_ts, uid).await;
        insert_raw(&db, id_b, hid, b.id, "adjust", "2", shared_ts, uid).await;

        // ORDER BY created_at DESC, id DESC → id_b is first page.
        let page1 = list_timeline(&db, hid, None, None, None, 1).await.unwrap();
        assert_eq!(page1.len(), 1);
        assert_eq!(page1[0].event.id, id_b);

        // Cursor pair drives page 2 — id_a is left.
        let page2 = list_timeline(&db, hid, None, Some(shared_ts), Some(id_b), 1)
            .await
            .unwrap();
        assert_eq!(page2.len(), 1, "tiebreak should surface the remaining event, not drop it");
        assert_eq!(page2[0].event.id, id_a);

        // One more hop returns empty.
        let page3 = list_timeline(&db, hid, None, Some(shared_ts), Some(id_a), 1)
            .await
            .unwrap();
        assert!(page3.is_empty());
    }

    async fn insert_raw(
        db: &crate::Database,
        id: Uuid,
        household_id: Uuid,
        batch_id: Uuid,
        event_type: &str,
        delta: &str,
        created_at: &str,
        created_by: Uuid,
    ) {
        sqlx::query(
            "INSERT INTO stock_event \
             (id, household_id, batch_id, event_type, quantity_delta, note, created_at, created_by, consume_request_id) \
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, NULL)",
        )
        .bind(id.to_string())
        .bind(household_id.to_string())
        .bind(batch_id.to_string())
        .bind(event_type)
        .bind(delta)
        .bind(created_at)
        .bind(created_by.to_string())
        .execute(&db.pool)
        .await
        .unwrap();
    }
}
