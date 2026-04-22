use jiff::{tz, Timestamp, ToSpan};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, time, Database};

pub const KIND_EXPIRY: &str = "expiry";

#[derive(Clone, Debug)]
pub struct ExpiryReminderPolicy {
    pub enabled: bool,
    pub lead_days: i64,
    pub fire_hour: u32,
    pub fire_minute: u32,
}

impl Default for ExpiryReminderPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            lead_days: 1,
            fire_hour: 9,
            fire_minute: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ReminderRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub kind: String,
    pub fire_at: String,
    pub household_timezone: String,
    pub household_fire_local_at: String,
    pub expires_on: Option<String>,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub presented_at: Option<String>,
    pub acked_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReminderPage {
    pub items: Vec<ReminderRow>,
    pub next_after_fire_at: Option<String>,
    pub next_after_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ReconcileStats {
    pub inserted: u64,
    pub deleted: u64,
}

#[derive(Debug, Clone)]
struct ReminderDraft {
    household_id: Uuid,
    batch_id: Uuid,
    product_id: Uuid,
    location_id: Uuid,
    kind: &'static str,
    fire_at: String,
    household_timezone: String,
    household_fire_local_at: String,
    expires_on: Option<String>,
    title: String,
    body: String,
}

#[derive(Debug, Clone)]
struct BatchReminderContext {
    household_id: Uuid,
    household_timezone: String,
    batch_id: Uuid,
    product_id: Uuid,
    location_id: Uuid,
    expires_on: Option<String>,
    depleted_at: Option<String>,
    product_name: String,
    location_name: String,
}

pub async fn sync_expiry_for_batch_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    batch_id: Uuid,
    policy: &ExpiryReminderPolicy,
) -> Result<(), sqlx::Error> {
    delete_pending_for_batch_kind_tx(tx, batch_id, KIND_EXPIRY).await?;
    let Some(draft) = load_batch_context_tx(tx, batch_id)
        .await?
        .and_then(|ctx| expiry_draft_for_context(&ctx, policy).transpose())
        .transpose()?
    else {
        return Ok(());
    };

    insert_draft_tx(tx, &draft).await
}

pub async fn list_due(
    db: &Database,
    household_id: Uuid,
    now_rfc3339: &str,
    after_fire_at: Option<&str>,
    after_id: Option<Uuid>,
    limit: i64,
) -> Result<ReminderPage, sqlx::Error> {
    let mut sql = String::from(
        "SELECT id, household_id, batch_id, product_id, location_id, kind, fire_at, \
                household_timezone, household_fire_local_at, expires_on, title, body, \
                created_at, presented_at, acked_at \
         FROM stock_reminder \
         WHERE household_id = ? AND acked_at IS NULL AND fire_at <= ? ",
    );
    match (after_fire_at, after_id) {
        (Some(fire_at), Some(id)) => {
            sql.push_str("AND (fire_at > ? OR (fire_at = ? AND id > ?)) ");
            sql.push_str("ORDER BY fire_at ASC, id ASC LIMIT ?");
            let rows = sqlx::query(&sql)
                .bind(household_id.to_string())
                .bind(now_rfc3339)
                .bind(fire_at)
                .bind(fire_at)
                .bind(id.to_string())
                .bind(limit)
                .fetch_all(&db.pool)
                .await?;
            return page_from_rows(rows, limit);
        }
        (Some(fire_at), None) => {
            sql.push_str("AND fire_at > ? ");
            sql.push_str("ORDER BY fire_at ASC, id ASC LIMIT ?");
            let rows = sqlx::query(&sql)
                .bind(household_id.to_string())
                .bind(now_rfc3339)
                .bind(fire_at)
                .bind(limit)
                .fetch_all(&db.pool)
                .await?;
            return page_from_rows(rows, limit);
        }
        (None, Some(_)) => {
            return Err(sqlx::Error::Protocol(
                "after_id requires after_fire_at".into(),
            ));
        }
        (None, None) => {}
    }

    sql.push_str("ORDER BY fire_at ASC, id ASC LIMIT ?");
    let rows = sqlx::query(&sql)
        .bind(household_id.to_string())
        .bind(now_rfc3339)
        .bind(limit)
        .fetch_all(&db.pool)
        .await?;
    page_from_rows(rows, limit)
}

pub async fn mark_presented(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    presented_at: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE stock_reminder SET presented_at = ? \
         WHERE id = ? AND household_id = ? AND acked_at IS NULL AND presented_at IS NULL",
    )
    .bind(presented_at)
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?
    .rows_affected();

    if updated > 0 {
        return Ok(true);
    }

    let exists = sqlx::query(
        "SELECT 1 AS x FROM stock_reminder WHERE id = ? AND household_id = ? AND acked_at IS NULL",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(exists.is_some())
}

pub async fn ack(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    acked_at: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE stock_reminder SET acked_at = ? \
         WHERE id = ? AND household_id = ? AND acked_at IS NULL",
    )
    .bind(acked_at)
    .bind(id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?
    .rows_affected();

    if updated > 0 {
        return Ok(true);
    }

    let exists = sqlx::query("SELECT 1 AS x FROM stock_reminder WHERE id = ? AND household_id = ?")
        .bind(id.to_string())
        .bind(household_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    Ok(exists.is_some())
}

pub async fn reconcile_household(
    db: &Database,
    household_id: Uuid,
    policy: &ExpiryReminderPolicy,
) -> Result<ReconcileStats, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let desired = load_household_drafts(&mut tx, household_id, policy).await?;
    let deleted = delete_pending_for_household_kind_tx(&mut tx, household_id, KIND_EXPIRY).await?;
    let mut inserted = 0;
    for draft in &desired {
        insert_draft_tx(&mut tx, draft).await?;
        inserted += 1;
    }
    tx.commit().await?;
    Ok(ReconcileStats { inserted, deleted })
}

pub async fn reconcile_all(
    db: &Database,
    policy: &ExpiryReminderPolicy,
) -> Result<ReconcileStats, sqlx::Error> {
    let household_rows = sqlx::query(
        "SELECT household_id FROM stock_batch \
         UNION \
         SELECT household_id FROM stock_reminder",
    )
    .fetch_all(&db.pool)
    .await?;

    let mut total = ReconcileStats::default();
    for row in household_rows {
        let household_id: String = row.try_get("household_id")?;
        let household_id =
            Uuid::parse_str(&household_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let stats = reconcile_household(db, household_id, policy).await?;
        total.inserted += stats.inserted;
        total.deleted += stats.deleted;
    }
    Ok(total)
}

pub fn build_expiry_reminder(
    expires_on: &str,
    household_timezone: &str,
    product_name: &str,
    location_name: &str,
    policy: &ExpiryReminderPolicy,
    now: Timestamp,
) -> Result<Option<(String, String, String, String, String)>, sqlx::Error> {
    if !policy.enabled {
        return Ok(None);
    }

    let expiry = time::parse_date(expires_on)?;
    let fire_date = expiry
        .checked_sub(policy.lead_days.days())
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let fire_civil = fire_date.at(policy.fire_hour as i8, policy.fire_minute as i8, 0, 0);
    let time_zone = tz::db()
        .get(household_timezone)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let fire_zoned = fire_civil
        .to_zoned(time_zone)
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
    let fire_at = fire_zoned.timestamp();
    if fire_at <= now {
        return Ok(None);
    }

    let title = match policy.lead_days {
        0 => format!("{product_name} expires today"),
        1 => format!("{product_name} expires tomorrow"),
        days => format!("{product_name} expires in {days} days"),
    };

    Ok(Some((
        time::format_timestamp(fire_at),
        title,
        location_name.to_owned(),
        household_timezone.to_owned(),
        time::format_zoned_with_offset(&fire_zoned),
    )))
}

async fn load_batch_context_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    batch_id: Uuid,
) -> Result<Option<BatchReminderContext>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT \
            b.household_id AS household_id, h.timezone AS household_timezone, \
            b.id AS batch_id, b.product_id AS product_id, \
            b.location_id AS location_id, b.expires_on AS expires_on, b.depleted_at AS depleted_at, \
            p.name AS product_name, l.name AS location_name \
         FROM stock_batch b \
         INNER JOIN household h ON h.id = b.household_id \
         INNER JOIN product p ON p.id = b.product_id \
         INNER JOIN location l ON l.id = b.location_id \
         WHERE b.id = ?",
    )
    .bind(batch_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;

    row.map(row_to_context).transpose()
}

async fn load_household_drafts(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    policy: &ExpiryReminderPolicy,
) -> Result<Vec<ReminderDraft>, sqlx::Error> {
    if !policy.enabled {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        "SELECT \
            b.household_id AS household_id, h.timezone AS household_timezone, \
            b.id AS batch_id, b.product_id AS product_id, \
            b.location_id AS location_id, b.expires_on AS expires_on, b.depleted_at AS depleted_at, \
            p.name AS product_name, l.name AS location_name \
         FROM stock_batch b \
         INNER JOIN household h ON h.id = b.household_id \
         INNER JOIN product p ON p.id = b.product_id \
         INNER JOIN location l ON l.id = b.location_id \
         WHERE b.household_id = ? AND b.depleted_at IS NULL",
    )
    .bind(household_id.to_string())
    .fetch_all(&mut **tx)
    .await?;

    let mut drafts = Vec::new();
    for row in rows {
        let ctx = row_to_context(row)?;
        if let Some(draft) = expiry_draft_for_context(&ctx, policy)? {
            drafts.push(draft);
        }
    }
    Ok(drafts)
}

fn expiry_draft_for_context(
    ctx: &BatchReminderContext,
    policy: &ExpiryReminderPolicy,
) -> Result<Option<ReminderDraft>, sqlx::Error> {
    if !policy.enabled || ctx.depleted_at.is_some() {
        return Ok(None);
    }
    let Some(expires_on) = ctx.expires_on.as_deref() else {
        return Ok(None);
    };
    let now = Timestamp::now();
    let Some((fire_at, title, body, household_timezone, household_fire_local_at)) =
        build_expiry_reminder(
            expires_on,
            &ctx.household_timezone,
            &ctx.product_name,
            &ctx.location_name,
            policy,
            now,
        )?
    else {
        return Ok(None);
    };

    Ok(Some(ReminderDraft {
        household_id: ctx.household_id,
        batch_id: ctx.batch_id,
        product_id: ctx.product_id,
        location_id: ctx.location_id,
        kind: KIND_EXPIRY,
        fire_at,
        household_timezone,
        household_fire_local_at,
        expires_on: ctx.expires_on.clone(),
        title,
        body,
    }))
}

async fn delete_pending_for_batch_kind_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    batch_id: Uuid,
    kind: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM stock_reminder WHERE batch_id = ? AND kind = ? AND acked_at IS NULL")
        .bind(batch_id.to_string())
        .bind(kind)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn delete_pending_for_household_kind_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    kind: &str,
) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query(
        "DELETE FROM stock_reminder WHERE household_id = ? AND kind = ? AND acked_at IS NULL",
    )
    .bind(household_id.to_string())
    .bind(kind)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(deleted)
}

async fn insert_draft_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    draft: &ReminderDraft,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO stock_reminder \
         (id, household_id, batch_id, product_id, location_id, kind, fire_at, household_timezone, \
          household_fire_local_at, expires_on, title, body, created_at, presented_at, acked_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(draft.household_id.to_string())
    .bind(draft.batch_id.to_string())
    .bind(draft.product_id.to_string())
    .bind(draft.location_id.to_string())
    .bind(draft.kind)
    .bind(&draft.fire_at)
    .bind(&draft.household_timezone)
    .bind(&draft.household_fire_local_at)
    .bind(&draft.expires_on)
    .bind(&draft.title)
    .bind(&draft.body)
    .bind(now_utc_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn page_from_rows(rows: Vec<sqlx::any::AnyRow>, limit: i64) -> Result<ReminderPage, sqlx::Error> {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(row_to_reminder(row)?);
    }
    let next = if items.len() as i64 == limit {
        items.last().map(|row| (Some(row.fire_at.clone()), Some(row.id)))
    } else {
        None
    };
    Ok(ReminderPage {
        items,
        next_after_fire_at: next.as_ref().and_then(|pair| pair.0.clone()),
        next_after_id: next.and_then(|pair| pair.1),
    })
}

fn row_to_context(row: sqlx::any::AnyRow) -> Result<BatchReminderContext, sqlx::Error> {
    Ok(BatchReminderContext {
        household_id: uuid_from(&row, "household_id")?,
        household_timezone: row.try_get("household_timezone")?,
        batch_id: uuid_from(&row, "batch_id")?,
        product_id: uuid_from(&row, "product_id")?,
        location_id: uuid_from(&row, "location_id")?,
        expires_on: row.try_get("expires_on")?,
        depleted_at: row.try_get("depleted_at")?,
        product_name: row.try_get("product_name")?,
        location_name: row.try_get("location_name")?,
    })
}

fn row_to_reminder(row: sqlx::any::AnyRow) -> Result<ReminderRow, sqlx::Error> {
    Ok(ReminderRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        batch_id: uuid_from(&row, "batch_id")?,
        product_id: uuid_from(&row, "product_id")?,
        location_id: uuid_from(&row, "location_id")?,
        kind: row.try_get("kind")?,
        fire_at: row.try_get("fire_at")?,
        household_timezone: row.try_get("household_timezone")?,
        household_fire_local_at: row.try_get("household_fire_local_at")?,
        expires_on: row.try_get("expires_on")?,
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        created_at: row.try_get("created_at")?,
        presented_at: row.try_get("presented_at")?,
        acked_at: row.try_get("acked_at")?,
    })
}

fn uuid_from(row: &sqlx::any::AnyRow, col: &str) -> Result<Uuid, sqlx::Error> {
    let s: String = row.try_get(col)?;
    Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{households, locations, memberships, products, stock, users};

    async fn setup() -> (Database, Uuid, Uuid, Uuid, Uuid) {
        let db = crate::test_db().await;
        let household = households::create(&db, "Home", "Europe/Madrid").await.unwrap();
        locations::seed_defaults(&db, household.id).await.unwrap();
        let pantry = locations::list_for_household(&db, household.id)
            .await
            .unwrap()
            .into_iter()
            .find(|row| row.kind == "pantry")
            .unwrap()
            .id;
        let user = users::create(&db, "alice", None, "hash").await.unwrap();
        memberships::insert(&db, household.id, user.id, "admin")
            .await
            .unwrap();
        let product = products::create_manual(
            &db,
            household.id,
            "Milk",
            None,
            "volume",
            Some("ml"),
            None,
            None,
        )
        .await
        .unwrap();
        (db, household.id, user.id, pantry, product.id)
    }

    fn enabled_policy() -> ExpiryReminderPolicy {
        ExpiryReminderPolicy {
            enabled: true,
            ..ExpiryReminderPolicy::default()
        }
    }

    #[test]
    fn build_expiry_reminder_formats_title_and_body() {
        let now: Timestamp = "2026-04-22T08:00:00.000Z".parse().unwrap();
        let policy = ExpiryReminderPolicy {
            enabled: true,
            lead_days: 1,
            fire_hour: 9,
            fire_minute: 0,
        };
        let reminder = build_expiry_reminder(
            "2026-04-24",
            "Europe/Madrid",
            "Milk",
            "Fridge",
            &policy,
            now,
        )
        .unwrap();
        let (fire_at, title, body, timezone, household_fire_local_at) = reminder.unwrap();
        assert_eq!(fire_at, "2026-04-23T07:00:00.000Z");
        assert_eq!(title, "Milk expires tomorrow");
        assert_eq!(body, "Fridge");
        assert_eq!(timezone, "Europe/Madrid");
        assert_eq!(household_fire_local_at, "2026-04-23T09:00:00+02:00");
    }

    #[tokio::test]
    async fn stock_create_creates_pending_reminder() {
        let (db, household_id, user_id, pantry, product_id) = setup().await;
        let batch = stock::create(
            &db,
            household_id,
            product_id,
            pantry,
            "1000",
            "ml",
            Some("2999-01-03"),
            None,
            None,
            user_id,
            Some(&enabled_policy()),
        )
        .await
        .unwrap();

        let page = list_due(&db, household_id, "3000-01-01T00:00:00.000Z", None, None, 10)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].batch_id, batch.id);
        assert_eq!(page.items[0].title, "Milk expires tomorrow");
    }

    #[tokio::test]
    async fn stock_update_replaces_pending_reminder() {
        let (db, household_id, user_id, pantry, product_id) = setup().await;
        let batch = stock::create(
            &db,
            household_id,
            product_id,
            pantry,
            "1000",
            "ml",
            Some("2999-01-03"),
            None,
            None,
            user_id,
            Some(&enabled_policy()),
        )
        .await
        .unwrap();

        stock::update_metadata(
            &db,
            household_id,
            batch.id,
            &stock::StockMetadataUpdate {
                expires_on: Some(Some("2999-01-06")),
                ..Default::default()
            },
            Some(&enabled_policy()),
        )
        .await
        .unwrap();

        let page = list_due(&db, household_id, "4000-01-01T00:00:00.000Z", None, None, 10)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].batch_id, batch.id);
        assert_eq!(page.items[0].expires_on.as_deref(), Some("2999-01-06"));
    }

    #[tokio::test]
    async fn reconcile_clears_orphaned_and_missing_rows() {
        let (db, household_id, user_id, pantry, product_id) = setup().await;
        let batch = stock::create(
            &db,
            household_id,
            product_id,
            pantry,
            "1000",
            "ml",
            Some("2999-01-03"),
            None,
            None,
            user_id,
            None,
        )
        .await
        .unwrap();

        let stats = reconcile_household(&db, household_id, &enabled_policy())
            .await
            .unwrap();
        assert_eq!(stats.inserted, 1);

        stock::discard(
            &db,
            household_id,
            batch.id,
            user_id,
            None,
            Some(&enabled_policy()),
        )
        .await
        .unwrap();

        let stats = reconcile_household(&db, household_id, &enabled_policy())
            .await
            .unwrap();
        assert!(stats.deleted <= 1);

        let page = list_due(&db, household_id, "4000-01-01T00:00:00.000Z", None, None, 10)
            .await
            .unwrap();
        assert!(page.items.is_empty());
    }
}
