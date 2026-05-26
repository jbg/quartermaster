use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, reminders, sql_for_backend, Database};

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_LEASED: &str = "leased";
pub const STATUS_RETRYABLE: &str = "retryable";
pub const STATUS_SUCCEEDED: &str = "succeeded";
pub const STATUS_FAILED: &str = "failed";

pub const KIND_AUTH_SESSION_CLEANUP: &str = "auth_session_cleanup";
pub const KIND_EXPIRY_REMINDER_RECONCILE: &str = "expiry_reminder_reconcile";
pub const KIND_BILLING_SYNC: &str = "billing_sync";
pub const KIND_HOUSEHOLD_PURGE: &str = "household_purge";
pub const KIND_SUPPLIER_CART_SUBMIT: &str = "supplier_cart_submit";
pub const KIND_SUPPLIER_ORDER_STATUS_SYNC: &str = "supplier_order_status_sync";

#[derive(Debug, Clone)]
pub struct JobRow {
    pub id: Uuid,
    pub kind: String,
    pub dedupe_key: String,
    pub payload_json: String,
    pub status: String,
    pub run_at: String,
    pub lease_owner: Option<String>,
    pub lease_until: Option<String>,
    pub attempt_count: i64,
    pub max_attempts: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewJob<'a> {
    pub kind: &'a str,
    pub dedupe_key: &'a str,
    pub payload_json: &'a str,
    pub run_at: &'a str,
    pub max_attempts: i64,
}

pub async fn enqueue_unique(db: &Database, job: &NewJob<'_>) -> Result<Option<Uuid>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let id = Uuid::now_v7();
    let inserted = sqlx::query(sql_for_backend(
        db.backend(),
        "INSERT INTO background_job \
         (id, kind, dedupe_key, payload_json, status, run_at, lease_owner, lease_until, \
          attempt_count, max_attempts, last_error, created_at, updated_at, finished_at) \
         VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 0, ?, NULL, ?, ?, NULL)",
        "INSERT INTO background_job \
         (id, kind, dedupe_key, payload_json, status, run_at, lease_owner, lease_until, \
          attempt_count, max_attempts, last_error, created_at, updated_at, finished_at) \
         VALUES ($1, $2, $3, $4, $5, $6, NULL, NULL, 0, $7, NULL, $8, $9, NULL)",
    ))
    .bind(id.to_string())
    .bind(job.kind)
    .bind(job.dedupe_key)
    .bind(job.payload_json)
    .bind(STATUS_PENDING)
    .bind(job.run_at)
    .bind(job.max_attempts)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await;

    match inserted {
        Ok(_) => Ok(Some(id)),
        Err(err) if is_unique_constraint_error(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

pub async fn active_job_exists(
    db: &Database,
    kind: &str,
    dedupe_key: &str,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT 1 AS x FROM background_job \
         WHERE kind = ? AND dedupe_key = ? AND status IN (?, ?, ?) \
         LIMIT 1",
        "SELECT 1 AS x FROM background_job \
         WHERE kind = $1 AND dedupe_key = $2 AND status IN ($3, $4, $5) \
         LIMIT 1",
    ))
    .bind(kind)
    .bind(dedupe_key)
    .bind(STATUS_PENDING)
    .bind(STATUS_LEASED)
    .bind(STATUS_RETRYABLE)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.is_some())
}

pub async fn expire_leases(
    db: &Database,
    now_rfc3339: &str,
    retry_at_rfc3339: &str,
) -> Result<u64, sqlx::Error> {
    let updated = sqlx::query(sql_for_backend(
        db.backend(),
        "UPDATE background_job \
         SET status = CASE WHEN attempt_count >= max_attempts THEN ? ELSE ? END, \
             run_at = ?, lease_owner = NULL, lease_until = NULL, \
             last_error = ?, updated_at = ?, \
             finished_at = CASE WHEN attempt_count >= max_attempts THEN ? ELSE NULL END \
         WHERE status = ? AND lease_until IS NOT NULL AND lease_until <= ?",
        "UPDATE background_job \
         SET status = CASE WHEN attempt_count >= max_attempts THEN $1 ELSE $2 END, \
             run_at = $3, lease_owner = NULL, lease_until = NULL, \
             last_error = $4, updated_at = $5, \
             finished_at = CASE WHEN attempt_count >= max_attempts THEN $6 ELSE NULL END \
         WHERE status = $7 AND lease_until IS NOT NULL AND lease_until <= $8",
    ))
    .bind(STATUS_FAILED)
    .bind(STATUS_RETRYABLE)
    .bind(retry_at_rfc3339)
    .bind("job lease expired before completion")
    .bind(now_rfc3339)
    .bind(now_rfc3339)
    .bind(STATUS_LEASED)
    .bind(now_rfc3339)
    .execute(&db.pool)
    .await?;
    Ok(updated.rows_affected())
}

pub async fn claim_due(
    db: &Database,
    now_rfc3339: &str,
    limit: i64,
    lease_owner: &str,
    lease_until_rfc3339: &str,
) -> Result<Vec<JobRow>, sqlx::Error> {
    let rows = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT id FROM background_job \
         WHERE (status IN (?, ?) AND run_at <= ?) \
            OR (status = ? AND lease_until IS NOT NULL AND lease_until <= ?) \
         ORDER BY run_at ASC, id ASC \
         LIMIT ?",
        "SELECT id FROM background_job \
         WHERE (status IN ($1, $2) AND run_at <= $3) \
            OR (status = $4 AND lease_until IS NOT NULL AND lease_until <= $5) \
         ORDER BY run_at ASC, id ASC \
         LIMIT $6",
    ))
    .bind(STATUS_PENDING)
    .bind(STATUS_RETRYABLE)
    .bind(now_rfc3339)
    .bind(STATUS_LEASED)
    .bind(now_rfc3339)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;

    let mut claimed = Vec::new();
    for row in rows {
        let id = uuid_from(&row, "id")?;
        let updated = sqlx::query(sql_for_backend(
            db.backend(),
            "UPDATE background_job \
             SET status = ?, lease_owner = ?, lease_until = ?, attempt_count = attempt_count + 1, \
                 updated_at = ? \
             WHERE id = ? \
               AND ( \
                    (status IN (?, ?) AND run_at <= ?) \
                    OR (status = ? AND lease_until IS NOT NULL AND lease_until <= ?) \
               ) \
               AND attempt_count < max_attempts",
            "UPDATE background_job \
             SET status = $1, lease_owner = $2, lease_until = $3, attempt_count = attempt_count + 1, \
                 updated_at = $4 \
             WHERE id = $5 \
               AND ( \
                    (status IN ($6, $7) AND run_at <= $8) \
                    OR (status = $9 AND lease_until IS NOT NULL AND lease_until <= $10) \
               ) \
               AND attempt_count < max_attempts",
        ))
        .bind(STATUS_LEASED)
        .bind(lease_owner)
        .bind(lease_until_rfc3339)
        .bind(now_rfc3339)
        .bind(id.to_string())
        .bind(STATUS_PENDING)
        .bind(STATUS_RETRYABLE)
        .bind(now_rfc3339)
        .bind(STATUS_LEASED)
        .bind(now_rfc3339)
        .execute(&db.pool)
        .await?;
        if updated.rows_affected() == 0 {
            continue;
        }
        if let Some(job) = find(db, id).await? {
            claimed.push(job);
        }
    }
    Ok(claimed)
}

pub async fn complete(
    db: &Database,
    id: Uuid,
    lease_owner: &str,
    finished_at: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(sql_for_backend(
        db.backend(),
        "UPDATE background_job \
         SET status = ?, lease_owner = NULL, lease_until = NULL, last_error = NULL, \
             updated_at = ?, finished_at = ? \
         WHERE id = ? AND status = ? AND lease_owner = ?",
        "UPDATE background_job \
         SET status = $1, lease_owner = NULL, lease_until = NULL, last_error = NULL, \
             updated_at = $2, finished_at = $3 \
         WHERE id = $4 AND status = $5 AND lease_owner = $6",
    ))
    .bind(STATUS_SUCCEEDED)
    .bind(finished_at)
    .bind(finished_at)
    .bind(id.to_string())
    .bind(STATUS_LEASED)
    .bind(lease_owner)
    .execute(&db.pool)
    .await?;
    Ok(updated.rows_affected() > 0)
}

pub async fn retry(
    db: &Database,
    id: Uuid,
    lease_owner: &str,
    run_at: &str,
    error: &str,
    updated_at: &str,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(sql_for_backend(
        db.backend(),
        "UPDATE background_job \
         SET status = CASE WHEN attempt_count >= max_attempts THEN ? ELSE ? END, \
             run_at = ?, lease_owner = NULL, lease_until = NULL, last_error = ?, \
             updated_at = ?, \
             finished_at = CASE WHEN attempt_count >= max_attempts THEN ? ELSE NULL END \
         WHERE id = ? AND status = ? AND lease_owner = ?",
        "UPDATE background_job \
         SET status = CASE WHEN attempt_count >= max_attempts THEN $1 ELSE $2 END, \
             run_at = $3, lease_owner = NULL, lease_until = NULL, last_error = $4, \
             updated_at = $5, \
             finished_at = CASE WHEN attempt_count >= max_attempts THEN $6 ELSE NULL END \
         WHERE id = $7 AND status = $8 AND lease_owner = $9",
    ))
    .bind(STATUS_FAILED)
    .bind(STATUS_RETRYABLE)
    .bind(run_at)
    .bind(error)
    .bind(updated_at)
    .bind(updated_at)
    .bind(id.to_string())
    .bind(STATUS_LEASED)
    .bind(lease_owner)
    .execute(&db.pool)
    .await?;
    Ok(updated.rows_affected() > 0)
}

pub async fn find(db: &Database, id: Uuid) -> Result<Option<JobRow>, sqlx::Error> {
    let row = sqlx::query(sql_for_backend(
        db.backend(),
        "SELECT id, kind, dedupe_key, payload_json, status, run_at, lease_owner, lease_until, \
                attempt_count, max_attempts, last_error, created_at, updated_at, finished_at \
         FROM background_job WHERE id = ?",
        "SELECT id, kind, dedupe_key, payload_json, status, run_at, lease_owner, lease_until, \
                attempt_count, max_attempts, last_error, created_at, updated_at, finished_at \
         FROM background_job WHERE id = $1",
    ))
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_job).transpose()
}

pub async fn enqueue_auth_session_cleanup(db: &Database) -> Result<bool, sqlx::Error> {
    let now = now_utc_rfc3339();
    Ok(enqueue_unique(
        db,
        &NewJob {
            kind: KIND_AUTH_SESSION_CLEANUP,
            dedupe_key: "stale-sessions",
            payload_json: "{}",
            run_at: &now,
            max_attempts: 5,
        },
    )
    .await?
    .is_some())
}

pub async fn enqueue_expiry_reconcile_all(db: &Database) -> Result<u64, sqlx::Error> {
    let now = now_utc_rfc3339();
    let mut queued = 0;
    for household_id in reminders::list_reconcile_household_ids(db).await? {
        let payload_json = format!(r#"{{"household_id":"{household_id}"}}"#);
        let inserted = enqueue_unique(
            db,
            &NewJob {
                kind: KIND_EXPIRY_REMINDER_RECONCILE,
                dedupe_key: &household_id.to_string(),
                payload_json: &payload_json,
                run_at: &now,
                max_attempts: 5,
            },
        )
        .await?;
        if inserted.is_some() {
            queued += 1;
        }
    }
    Ok(queued)
}

fn row_to_job(row: sqlx::any::AnyRow) -> Result<JobRow, sqlx::Error> {
    Ok(JobRow {
        id: uuid_from(&row, "id")?,
        kind: row.try_get("kind")?,
        dedupe_key: row.try_get("dedupe_key")?,
        payload_json: row.try_get("payload_json")?,
        status: row.try_get("status")?,
        run_at: row.try_get("run_at")?,
        lease_owner: row.try_get("lease_owner")?,
        lease_until: row.try_get("lease_until")?,
        attempt_count: row.try_get("attempt_count")?,
        max_attempts: row.try_get("max_attempts")?,
        last_error: row.try_get("last_error")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}

fn uuid_from(row: &sqlx::any::AnyRow, column: &str) -> Result<Uuid, sqlx::Error> {
    let raw: String = row.try_get(column)?;
    Uuid::parse_str(&raw).map_err(|e| sqlx::Error::Decode(Box::new(e)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    #[tokio::test]
    async fn enqueue_unique_allows_one_active_job() {
        let db = test_support::sqlite().await.into_db();
        let now = "2000-01-01T00:00:00.000Z";
        let job = NewJob {
            kind: KIND_BILLING_SYNC,
            dedupe_key: "tenant-1",
            payload_json: "{}",
            run_at: now,
            max_attempts: 3,
        };
        assert!(enqueue_unique(&db, &job).await.unwrap().is_some());
        assert!(enqueue_unique(&db, &job).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn postgres_concurrent_claims_choose_one_owner() {
        let Some(test_db) = test_support::postgres().await else {
            return;
        };
        let db = test_db.into_db();
        let now = "2000-01-01T00:00:00.000Z";
        enqueue_unique(
            &db,
            &NewJob {
                kind: KIND_BILLING_SYNC,
                dedupe_key: "tenant-1",
                payload_json: "{}",
                run_at: now,
                max_attempts: 3,
            },
        )
        .await
        .unwrap();

        let db1 = db.clone();
        let db2 = db.clone();
        let t1 = tokio::spawn(async move {
            claim_due(&db1, now, 10, "worker-a", "2000-01-01T00:01:00.000Z")
                .await
                .unwrap()
        });
        let t2 = tokio::spawn(async move {
            claim_due(&db2, now, 10, "worker-b", "2000-01-01T00:01:00.000Z")
                .await
                .unwrap()
        });
        let r1 = t1.await.unwrap();
        let r2 = t2.await.unwrap();
        assert_eq!(r1.len() + r2.len(), 1);
    }

    #[tokio::test]
    async fn expired_lease_becomes_claimable_again() {
        let db = test_support::sqlite().await.into_db();
        let now = "2000-01-01T00:00:00.000Z";
        let id = enqueue_unique(
            &db,
            &NewJob {
                kind: KIND_BILLING_SYNC,
                dedupe_key: "tenant-1",
                payload_json: "{}",
                run_at: now,
                max_attempts: 3,
            },
        )
        .await
        .unwrap()
        .unwrap();
        let claimed = claim_due(&db, now, 10, "worker-a", "2000-01-01T00:01:00.000Z")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        expire_leases(&db, "2000-01-01T00:02:00.000Z", "2000-01-01T00:03:00.000Z")
            .await
            .unwrap();
        let reclaimed = claim_due(
            &db,
            "2000-01-01T00:03:00.000Z",
            10,
            "worker-b",
            "2000-01-01T00:04:00.000Z",
        )
        .await
        .unwrap();
        assert_eq!(reclaimed.len(), 1);
        assert_eq!(reclaimed[0].id, id);
    }
}
