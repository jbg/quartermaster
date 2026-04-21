use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{now_utc_rfc3339, Database};

#[derive(Debug, Clone, Serialize)]
pub struct MembershipRow {
    pub household_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MembershipWithUserRow {
    pub membership: MembershipRow,
    pub username: String,
    pub email: Option<String>,
}

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

pub async fn find(
    db: &Database,
    household_id: Uuid,
    user_id: Uuid,
) -> Result<Option<MembershipRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT household_id, user_id, role, joined_at \
         FROM membership WHERE household_id = ? AND user_id = ?",
    )
    .bind(household_id.to_string())
    .bind(user_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_membership).transpose()
}

pub async fn list_members(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<MembershipWithUserRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT m.household_id, m.user_id, m.role, m.joined_at, u.username, u.email \
         FROM membership m \
         INNER JOIN users u ON u.id = m.user_id \
         WHERE m.household_id = ? \
         ORDER BY m.joined_at ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok::<_, sqlx::Error>(MembershipWithUserRow {
                membership: row_to_membership_ref(&row)?,
                username: row.try_get("username")?,
                email: row.try_get("email")?,
            })
        })
        .collect()
}

pub async fn count_admins(
    db: &Database,
    household_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS n FROM membership WHERE household_id = ? AND role = 'admin'",
    )
    .bind(household_id.to_string())
    .fetch_one(&db.pool)
    .await?;
    row.try_get("n")
}

pub async fn remove(
    db: &Database,
    household_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM membership WHERE household_id = ? AND user_id = ?")
        .bind(household_id.to_string())
        .bind(user_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

fn row_to_membership(row: sqlx::any::AnyRow) -> Result<MembershipRow, sqlx::Error> {
    row_to_membership_ref(&row)
}

fn row_to_membership_ref(row: &sqlx::any::AnyRow) -> Result<MembershipRow, sqlx::Error> {
    let household_id: String = row.try_get("household_id")?;
    let user_id: String = row.try_get("user_id")?;
    Ok(MembershipRow {
        household_id: Uuid::parse_str(&household_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        user_id: Uuid::parse_str(&user_id)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        role: row.try_get("role")?,
        joined_at: row.try_get("joined_at")?,
    })
}
