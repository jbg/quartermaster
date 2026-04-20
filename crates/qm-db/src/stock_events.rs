//! Read-side helpers over the append-only `stock_event` ledger.
//!
//! No HTTP surface uses these yet — they exist for tests and for the future
//! consumption-history / analytics features the ledger exists to unlock.

use sqlx::Row;
use uuid::Uuid;

use crate::Database;

pub const EVENT_ADD: &str = "add";
pub const EVENT_CONSUME: &str = "consume";
pub const EVENT_ADJUST: &str = "adjust";
pub const EVENT_DISCARD: &str = "discard";

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
