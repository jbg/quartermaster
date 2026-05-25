use sqlx::Row;
use uuid::Uuid;

use crate::Database;

pub async fn count_household_members(
    db: &Database,
    household_id: Uuid,
) -> Result<i64, sqlx::Error> {
    scalar_count(
        db,
        "SELECT COUNT(*) AS n FROM membership WHERE household_id = ?",
        household_id,
    )
    .await
}

pub async fn count_household_products(
    db: &Database,
    household_id: Uuid,
) -> Result<i64, sqlx::Error> {
    scalar_count(
        db,
        "SELECT COUNT(*) AS n \
         FROM product \
         WHERE created_by_household_id = ? AND deleted_at IS NULL",
        household_id,
    )
    .await
}

pub async fn count_household_stock_batches(
    db: &Database,
    household_id: Uuid,
) -> Result<i64, sqlx::Error> {
    scalar_count(
        db,
        "SELECT COUNT(*) AS n \
         FROM stock_batch \
         WHERE household_id = ? AND depleted_at IS NULL",
        household_id,
    )
    .await
}

pub async fn count_household_reminders(
    db: &Database,
    household_id: Uuid,
) -> Result<i64, sqlx::Error> {
    scalar_count(
        db,
        "SELECT COUNT(*) AS n \
         FROM stock_reminder \
         WHERE household_id = ? AND acked_at IS NULL",
        household_id,
    )
    .await
}

pub async fn count_household_invites(
    db: &Database,
    household_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let now = crate::now_utc_rfc3339();
    let row = sqlx::query(
        "SELECT COUNT(*) AS n \
         FROM invite \
         WHERE household_id = ? AND revoked_at IS NULL AND expires_at > ? AND use_count < max_uses",
    )
    .bind(household_id.to_string())
    .bind(now)
    .fetch_one(&db.pool)
    .await?;
    row.try_get("n")
}

pub async fn count_user_push_devices(db: &Database, user_id: Uuid) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS n \
         FROM notification_device \
         WHERE user_id = ?",
    )
    .bind(user_id.to_string())
    .fetch_one(&db.pool)
    .await?;
    row.try_get("n")
}

async fn scalar_count(
    db: &Database,
    sql: &'static str,
    household_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(sql)
        .bind(household_id.to_string())
        .fetch_one(&db.pool)
        .await?;
    row.try_get("n")
}
