use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

pub async fn insert(
    db: &Database,
    household_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO membership (household_id, user_id, role, joined_at) VALUES (?, ?, ?, ?)",
    )
    .bind(household_id.to_string())
    .bind(user_id.to_string())
    .bind(role)
    .bind(now_utc_rfc3339())
    .execute(&db.pool)
    .await?;
    Ok(())
}
