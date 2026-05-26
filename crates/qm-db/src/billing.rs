use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, sql_for_backend, Backend, Database};

pub const DEFAULT_PLAN_KEY: &str = "self_hosted";

#[derive(Debug, Clone)]
pub struct BillingAccountRow {
    pub id: Uuid,
    pub plan_key: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create(db: &Database, plan_key: &str) -> Result<BillingAccountRow, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let row = create_in_tx(&mut tx, db.backend(), plan_key).await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn create_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    plan_key: &str,
) -> Result<BillingAccountRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(sql_for_backend(
        backend,
        "INSERT INTO billing_account (id, plan_key, created_at, updated_at) VALUES (?, ?, ?, ?)",
        "INSERT INTO billing_account (id, plan_key, created_at, updated_at) VALUES ($1, $2, $3, $4)",
    ))
    .bind(id.to_string())
    .bind(plan_key)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await?;

    Ok(BillingAccountRow {
        id,
        plan_key: plan_key.to_owned(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub async fn ensure_for_household(
    db: &Database,
    household_id: Uuid,
    plan_key: &str,
) -> Result<BillingAccountRow, sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    let row = ensure_for_household_in_tx(&mut tx, db.backend(), household_id, plan_key).await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn ensure_for_household_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    household_id: Uuid,
    plan_key: &str,
) -> Result<BillingAccountRow, sqlx::Error> {
    if let Some(row) = find_for_household_in_tx(&mut *tx, backend, household_id).await? {
        return Ok(row);
    }
    let account = create_in_tx(&mut *tx, backend, plan_key).await?;
    attach_household_in_tx(&mut *tx, backend, household_id, account.id).await?;
    Ok(account)
}

pub async fn find_for_household(
    db: &Database,
    household_id: Uuid,
) -> Result<Option<BillingAccountRow>, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT b.id, b.plan_key, b.created_at, b.updated_at \
         FROM billing_account b \
         INNER JOIN household h ON h.billing_account_id = b.id \
         WHERE h.id = ?",
        "SELECT b.id, b.plan_key, b.created_at, b.updated_at \
         FROM billing_account b \
         INNER JOIN household h ON h.billing_account_id = b.id \
         WHERE h.id = $1",
    ))
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_billing_account).transpose()
}

pub async fn find_for_household_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    household_id: Uuid,
) -> Result<Option<BillingAccountRow>, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        backend,
        "SELECT b.id, b.plan_key, b.created_at, b.updated_at \
         FROM billing_account b \
         INNER JOIN household h ON h.billing_account_id = b.id \
         WHERE h.id = ?",
        "SELECT b.id, b.plan_key, b.created_at, b.updated_at \
         FROM billing_account b \
         INNER JOIN household h ON h.billing_account_id = b.id \
         WHERE h.id = $1",
    ))
    .bind(household_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(row_to_billing_account).transpose()
}

pub async fn attach_household(
    db: &Database,
    household_id: Uuid,
    billing_account_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(sql_for_backend(
        db.backend(),
        "UPDATE household SET billing_account_id = ? WHERE id = ?",
        "UPDATE household SET billing_account_id = $1 WHERE id = $2",
    ))
    .bind(billing_account_id.to_string())
    .bind(household_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn attach_household_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    backend: Backend,
    household_id: Uuid,
    billing_account_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(sql_for_backend(
        backend,
        "UPDATE household SET billing_account_id = ? WHERE id = ?",
        "UPDATE household SET billing_account_id = $1 WHERE id = $2",
    ))
    .bind(billing_account_id.to_string())
    .bind(household_id.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn count_households(db: &Database, billing_account_id: Uuid) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT COUNT(*) AS n \
         FROM household \
         WHERE billing_account_id = ? AND deletion_requested_at IS NULL",
        "SELECT COUNT(*) AS n \
         FROM household \
         WHERE billing_account_id = $1 AND deletion_requested_at IS NULL",
    ))
    .bind(billing_account_id.to_string())
    .fetch_one(&db.pool)
    .await?;
    row.try_get("n")
}

fn row_to_billing_account(row: sqlx::any::AnyRow) -> Result<BillingAccountRow, sqlx::Error> {
    let id: String = row.try_get("id")?;
    Ok(BillingAccountRow {
        id: Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        plan_key: row.try_get("plan_key")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
