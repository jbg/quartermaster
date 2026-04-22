use jiff::{tz, Timestamp, ToSpan};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{devices, now_utc_rfc3339, time, Database};

pub const KIND_EXPIRY: &str = "expiry";
pub const CHANNEL_APNS: &str = "apns";
pub const DELIVERY_STATUS_SENDING: &str = "sending";
pub const DELIVERY_STATUS_SUCCEEDED: &str = "succeeded";
pub const DELIVERY_STATUS_FAILED_RETRYABLE: &str = "failed_retryable";
pub const DELIVERY_STATUS_FAILED_PERMANENT: &str = "failed_permanent";

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
    pub presented_on_device_at: Option<String>,
    pub opened_on_device_at: Option<String>,
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

#[derive(Debug, Clone)]
pub struct PushWorkItem {
    pub attempt_id: Uuid,
    pub reminder_id: Uuid,
    pub household_id: Uuid,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub device_row_id: Uuid,
    pub device_token: String,
}

#[derive(Debug, Clone)]
pub struct PushDeliveryResult {
    pub status: &'static str,
    pub finished_at: String,
    pub next_retry_at: Option<String>,
    pub provider_message_id: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
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
    session_id: Uuid,
    now_rfc3339: &str,
    after_fire_at: Option<&str>,
    after_id: Option<Uuid>,
    limit: i64,
) -> Result<ReminderPage, sqlx::Error> {
    let current_device = devices::find_latest_for_session(db, session_id).await?;
    let (mut sql, bind_device_id) = if let Some(device) = current_device {
        (
            String::from(
                "SELECT r.id, r.household_id, r.batch_id, r.product_id, r.location_id, r.kind, \
                        r.fire_at, r.household_timezone, r.household_fire_local_at, \
                        r.expires_on, r.title, r.body, r.created_at, \
                        s.first_presented_at AS presented_on_device_at, \
                        s.opened_at AS opened_on_device_at \
                 FROM stock_reminder r \
                 LEFT JOIN reminder_device_state s \
                   ON s.reminder_id = r.id AND s.device_id = ? \
                 WHERE r.household_id = ? AND r.acked_at IS NULL AND r.fire_at <= ? ",
            ),
            Some(device.id.to_string()),
        )
    } else {
        (
            String::from(
                "SELECT r.id, r.household_id, r.batch_id, r.product_id, r.location_id, r.kind, \
                        r.fire_at, r.household_timezone, r.household_fire_local_at, \
                        r.expires_on, r.title, r.body, r.created_at, \
                        NULL AS presented_on_device_at, \
                        NULL AS opened_on_device_at \
                 FROM stock_reminder r \
                 WHERE r.household_id = ? AND r.acked_at IS NULL AND r.fire_at <= ? ",
            ),
            None,
        )
    };
    match (after_fire_at, after_id) {
        (Some(fire_at), Some(id)) => {
            sql.push_str("AND (fire_at > ? OR (fire_at = ? AND id > ?)) ");
            sql.push_str("ORDER BY fire_at ASC, id ASC LIMIT ?");
            let mut query = sqlx::query(&sql);
            if let Some(device_id) = &bind_device_id {
                query = query.bind(device_id);
            }
            let rows = query
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
            let mut query = sqlx::query(&sql);
            if let Some(device_id) = &bind_device_id {
                query = query.bind(device_id);
            }
            let rows = query
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
    let mut query = sqlx::query(&sql);
    if let Some(device_id) = &bind_device_id {
        query = query.bind(device_id);
    }
    let rows = query
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
    session_id: Uuid,
    id: Uuid,
    presented_at: &str,
) -> Result<bool, sqlx::Error> {
    if !pending_exists(db, household_id, id).await? {
        return Ok(false);
    }
    if let Some(device) = devices::find_latest_for_session(db, session_id).await? {
        upsert_device_state_seen(db, id, device.id, Some(presented_at), None).await?;
    }
    Ok(true)
}

pub async fn mark_opened(
    db: &Database,
    household_id: Uuid,
    session_id: Uuid,
    id: Uuid,
    opened_at: &str,
) -> Result<bool, sqlx::Error> {
    if !pending_exists(db, household_id, id).await? {
        return Ok(false);
    }
    if let Some(device) = devices::find_latest_for_session(db, session_id).await? {
        upsert_device_state_seen(db, id, device.id, Some(opened_at), Some(opened_at)).await?;
    }
    Ok(true)
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

pub async fn expire_stale_push_claims(
    db: &Database,
    now_rfc3339: &str,
    retry_at_rfc3339: &str,
) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, reminder_id, device_id \
         FROM reminder_delivery \
         WHERE channel = ? AND status = ? AND claim_until IS NOT NULL AND claim_until <= ?",
    )
    .bind(CHANNEL_APNS)
    .bind(DELIVERY_STATUS_SENDING)
    .bind(now_rfc3339)
    .fetch_all(&db.pool)
    .await?;

    let mut expired = 0;
    for row in rows {
        let attempt_id = uuid_from(&row, "id")?;
        let reminder_id = uuid_from(&row, "reminder_id")?;
        let device_id = uuid_from(&row, "device_id")?;
        sqlx::query(
            "UPDATE reminder_delivery \
             SET status = ?, finished_at = ?, claim_until = NULL, error_code = ?, error_message = ? \
             WHERE id = ? AND status = ?",
        )
        .bind(DELIVERY_STATUS_FAILED_RETRYABLE)
        .bind(now_rfc3339)
        .bind("claim_expired")
        .bind("push delivery claim expired before completion")
        .bind(attempt_id.to_string())
        .bind(DELIVERY_STATUS_SENDING)
        .execute(&db.pool)
        .await?;
        upsert_device_state_delivery(
            db,
            reminder_id,
            device_id,
            None,
            now_rfc3339,
            DELIVERY_STATUS_FAILED_RETRYABLE,
            retry_at_rfc3339,
            Some("claim_expired"),
            Some("push delivery claim expired before completion"),
        )
        .await?;
        expired += 1;
    }
    Ok(expired)
}

pub async fn claim_due_push_work(
    db: &Database,
    now_rfc3339: &str,
    limit: i64,
    claim_until_rfc3339: &str,
) -> Result<Vec<PushWorkItem>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT r.id AS reminder_id, r.household_id, r.batch_id, r.product_id, r.location_id, \
                r.kind, r.title, r.body, d.id AS device_row_id, d.push_token \
         FROM stock_reminder r \
         INNER JOIN membership m ON m.household_id = r.household_id \
         INNER JOIN notification_device d ON d.user_id = m.user_id \
         LEFT JOIN reminder_device_state s \
           ON s.reminder_id = r.id AND s.device_id = d.id \
         WHERE r.acked_at IS NULL \
           AND r.fire_at <= ? \
           AND d.push_token IS NOT NULL \
           AND d.push_token <> '' \
           AND d.push_authorization IN ('authorized', 'provisional') \
           AND (s.last_push_status IS NULL OR s.last_push_status <> ?) \
           AND NOT (s.last_push_status = ? AND s.last_push_token = d.push_token) \
           AND (s.next_retry_at IS NULL OR s.next_retry_at <= ?) \
           AND NOT EXISTS ( \
               SELECT 1 FROM reminder_delivery rd \
               WHERE rd.reminder_id = r.id \
                 AND rd.device_id = d.id \
                 AND rd.channel = ? \
                 AND rd.status = ? \
                 AND (rd.claim_until IS NULL OR rd.claim_until > ?) \
           ) \
         ORDER BY r.fire_at ASC, r.id ASC, d.updated_at DESC, d.id ASC \
         LIMIT ?",
    )
    .bind(now_rfc3339)
    .bind(DELIVERY_STATUS_SUCCEEDED)
    .bind(DELIVERY_STATUS_FAILED_PERMANENT)
    .bind(now_rfc3339)
    .bind(CHANNEL_APNS)
    .bind(DELIVERY_STATUS_SENDING)
    .bind(now_rfc3339)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;

    let mut claimed = Vec::new();
    for row in rows {
        let reminder_id = uuid_from(&row, "reminder_id")?;
        let device_row_id = uuid_from(&row, "device_row_id")?;
        let push_token: Option<String> = row.try_get("push_token")?;
        let Some(push_token) = push_token.filter(|value| !value.is_empty()) else {
            continue;
        };
        let attempt_id = Uuid::now_v7();
        let inserted = sqlx::query(
            "INSERT INTO reminder_delivery \
             (id, reminder_id, device_id, channel, status, created_at, attempted_at, claim_until) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(attempt_id.to_string())
        .bind(reminder_id.to_string())
        .bind(device_row_id.to_string())
        .bind(CHANNEL_APNS)
        .bind(DELIVERY_STATUS_SENDING)
        .bind(now_rfc3339)
        .bind(now_rfc3339)
        .bind(claim_until_rfc3339)
        .execute(&db.pool)
        .await;
        match inserted {
            Ok(_) => {
                upsert_device_state_delivery(
                    db,
                    reminder_id,
                    device_row_id,
                    Some(&push_token),
                    now_rfc3339,
                    DELIVERY_STATUS_SENDING,
                    "",
                    None,
                    None,
                )
                .await?;
                claimed.push(PushWorkItem {
                    attempt_id,
                    reminder_id,
                    household_id: uuid_from(&row, "household_id")?,
                    batch_id: uuid_from(&row, "batch_id")?,
                    product_id: uuid_from(&row, "product_id")?,
                    location_id: uuid_from(&row, "location_id")?,
                    kind: row.try_get("kind")?,
                    title: row.try_get("title")?,
                    body: row.try_get("body")?,
                    device_row_id,
                    device_token: push_token,
                });
            }
            Err(err) if is_unique_constraint_error(&err) => continue,
            Err(err) => return Err(err),
        }
    }
    Ok(claimed)
}

pub async fn complete_push_attempt(
    db: &Database,
    work: &PushWorkItem,
    outcome: &PushDeliveryResult,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE reminder_delivery \
         SET status = ?, finished_at = ?, claim_until = NULL, provider_message_id = ?, \
             error_code = ?, error_message = ? \
         WHERE id = ?",
    )
    .bind(outcome.status)
    .bind(&outcome.finished_at)
    .bind(&outcome.provider_message_id)
    .bind(&outcome.error_code)
    .bind(&outcome.error_message)
    .bind(work.attempt_id.to_string())
    .execute(&db.pool)
    .await?;
    upsert_device_state_delivery(
        db,
        work.reminder_id,
        work.device_row_id,
        Some(&work.device_token),
        &outcome.finished_at,
        outcome.status,
        outcome.next_retry_at.as_deref().unwrap_or(""),
        outcome.error_code.as_deref(),
        outcome.error_message.as_deref(),
    )
    .await
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
          household_fire_local_at, expires_on, title, body, created_at, acked_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
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

async fn pending_exists(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let exists = sqlx::query(
        "SELECT 1 AS x FROM stock_reminder WHERE id = ? AND household_id = ? AND acked_at IS NULL",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(exists.is_some())
}

async fn upsert_device_state_seen(
    db: &Database,
    reminder_id: Uuid,
    device_id: Uuid,
    presented_at: Option<&str>,
    opened_at: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = match opened_at.or(presented_at) {
        Some(value) => value.to_owned(),
        None => now_utc_rfc3339(),
    };
    let updated = sqlx::query(
        "UPDATE reminder_device_state \
         SET first_presented_at = COALESCE(first_presented_at, ?), \
             opened_at = COALESCE(opened_at, ?), \
             updated_at = ? \
         WHERE reminder_id = ? AND device_id = ?",
    )
    .bind(presented_at)
    .bind(opened_at)
    .bind(&now)
    .bind(reminder_id.to_string())
    .bind(device_id.to_string())
    .execute(&db.pool)
    .await?
    .rows_affected();
    if updated > 0 {
        return Ok(());
    }

    let inserted = sqlx::query(
        "INSERT INTO reminder_device_state \
         (reminder_id, device_id, first_push_attempted_at, last_push_attempted_at, last_push_status, \
          last_push_token, next_retry_at, last_error_code, last_error_message, first_presented_at, \
          opened_at, created_at, updated_at) \
         VALUES (?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL, ?, ?, ?, ?)",
    )
    .bind(reminder_id.to_string())
    .bind(device_id.to_string())
    .bind(presented_at)
    .bind(opened_at)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await;
    match inserted {
        Ok(_) => Ok(()),
        Err(err) if is_unique_constraint_error(&err) => {
            sqlx::query(
                "UPDATE reminder_device_state \
                 SET first_presented_at = COALESCE(first_presented_at, ?), \
                     opened_at = COALESCE(opened_at, ?), \
                     updated_at = ? \
                 WHERE reminder_id = ? AND device_id = ?",
            )
            .bind(presented_at)
            .bind(opened_at)
            .bind(&now)
            .bind(reminder_id.to_string())
            .bind(device_id.to_string())
            .execute(&db.pool)
            .await?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

async fn upsert_device_state_delivery(
    db: &Database,
    reminder_id: Uuid,
    device_id: Uuid,
    push_token: Option<&str>,
    attempted_at: &str,
    status: &str,
    next_retry_at: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> Result<(), sqlx::Error> {
    let next_retry = if next_retry_at.is_empty() {
        None
    } else {
        Some(next_retry_at)
    };
    let updated = sqlx::query(
        "UPDATE reminder_device_state \
         SET first_push_attempted_at = COALESCE(first_push_attempted_at, ?), \
             last_push_attempted_at = ?, \
             last_push_status = ?, \
             last_push_token = ?, \
             next_retry_at = ?, \
             last_error_code = ?, \
             last_error_message = ?, \
             updated_at = ? \
         WHERE reminder_id = ? AND device_id = ?",
    )
    .bind(attempted_at)
    .bind(attempted_at)
    .bind(status)
    .bind(push_token)
    .bind(next_retry)
    .bind(error_code)
    .bind(error_message)
    .bind(attempted_at)
    .bind(reminder_id.to_string())
    .bind(device_id.to_string())
    .execute(&db.pool)
    .await?
    .rows_affected();
    if updated > 0 {
        return Ok(());
    }

    let inserted = sqlx::query(
        "INSERT INTO reminder_device_state \
         (reminder_id, device_id, first_push_attempted_at, last_push_attempted_at, last_push_status, \
          last_push_token, next_retry_at, last_error_code, last_error_message, first_presented_at, \
          opened_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?, ?)",
    )
    .bind(reminder_id.to_string())
    .bind(device_id.to_string())
    .bind(attempted_at)
    .bind(attempted_at)
    .bind(status)
    .bind(push_token)
    .bind(next_retry)
    .bind(error_code)
    .bind(error_message)
    .bind(attempted_at)
    .bind(attempted_at)
    .execute(&db.pool)
    .await;
    match inserted {
        Ok(_) => Ok(()),
        Err(err) if is_unique_constraint_error(&err) => {
            sqlx::query(
                "UPDATE reminder_device_state \
                 SET first_push_attempted_at = COALESCE(first_push_attempted_at, ?), \
                     last_push_attempted_at = ?, \
                     last_push_status = ?, \
                     last_push_token = ?, \
                     next_retry_at = ?, \
                     last_error_code = ?, \
                     last_error_message = ?, \
                     updated_at = ? \
                 WHERE reminder_id = ? AND device_id = ?",
            )
            .bind(attempted_at)
            .bind(attempted_at)
            .bind(status)
            .bind(push_token)
            .bind(next_retry)
            .bind(error_code)
            .bind(error_message)
            .bind(attempted_at)
            .bind(reminder_id.to_string())
            .bind(device_id.to_string())
            .execute(&db.pool)
            .await?;
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn is_unique_constraint_error(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db_err) => {
            let message = db_err.message().to_ascii_lowercase();
            message.contains("unique") || message.contains("duplicate")
        }
        _ => false,
    }
}

fn page_from_rows(rows: Vec<sqlx::any::AnyRow>, limit: i64) -> Result<ReminderPage, sqlx::Error> {
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(row_to_reminder(row)?);
    }
    let next = if items.len() as i64 == limit {
        items
            .last()
            .map(|row| (Some(row.fire_at.clone()), Some(row.id)))
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
        presented_on_device_at: row.try_get("presented_on_device_at")?,
        opened_on_device_at: row.try_get("opened_on_device_at")?,
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
        let household = households::create(&db, "Home", "Europe/Madrid")
            .await
            .unwrap();
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

        let page = list_due(
            &db,
            household_id,
            Uuid::nil(),
            "3000-01-01T00:00:00.000Z",
            None,
            None,
            10,
        )
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

        let page = list_due(
            &db,
            household_id,
            Uuid::nil(),
            "4000-01-01T00:00:00.000Z",
            None,
            None,
            10,
        )
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

        let page = list_due(
            &db,
            household_id,
            Uuid::nil(),
            "4000-01-01T00:00:00.000Z",
            None,
            None,
            10,
        )
        .await
        .unwrap();
        assert!(page.items.is_empty());
    }
}
