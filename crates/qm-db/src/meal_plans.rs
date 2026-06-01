use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{audited_sql, now_utc_rfc3339, Database};

pub const PLAN_STATUS_DRAFT: &str = "draft";
pub const PLAN_STATUS_ACTIVE: &str = "active";
pub const PLAN_STATUS_COMPLETED: &str = "completed";

pub const MEAL_STATUS_PLANNED: &str = "planned";
pub const MEAL_STATUS_SKIPPED: &str = "skipped";
pub const MEAL_STATUS_COOKED: &str = "cooked";
pub const MEAL_STATUS_CONFLICTED: &str = "conflicted";

pub const RESERVATION_ACTIVE: &str = "active";
pub const RESERVATION_RELEASED: &str = "released";
pub const RESERVATION_CONSUMED: &str = "consumed";
pub const RESERVATION_CONFLICTED: &str = "conflicted";

const PLAN_COLS: &str = "id, household_id, title, status, constraints_json, ai_task_id, \
                         created_at, updated_at, created_by, updated_by";
const DAY_COLS: &str = "id, household_id, meal_plan_id, plan_date, sort_order, created_at";
const MEAL_COLS: &str = "id, household_id, meal_plan_id, meal_plan_day_id, plan_date, \
                         slot_key, slot_label, sort_order, recipe_id, recipe_version_id, \
                         recipe_name, serving_scale, status, preflight_json, warnings_json, \
                         conflicts_json, created_at, updated_at";
const RESERVATION_COLS: &str = "id, household_id, meal_plan_id, meal_plan_meal_id, batch_id, \
                                product_id, quantity, unit, status, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct MealPlanRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub title: String,
    pub status: String,
    pub constraints_json: String,
    pub ai_task_id: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MealPlanDayRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub meal_plan_id: Uuid,
    pub plan_date: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MealPlanMealRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub meal_plan_id: Uuid,
    pub meal_plan_day_id: Uuid,
    pub plan_date: String,
    pub slot_key: String,
    pub slot_label: String,
    pub sort_order: i64,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub recipe_name: Option<String>,
    pub serving_scale: String,
    pub status: String,
    pub preflight_json: Option<String>,
    pub warnings_json: String,
    pub conflicts_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StockReservationRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub meal_plan_id: Uuid,
    pub meal_plan_meal_id: Uuid,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct MealPlanFull {
    pub plan: MealPlanRow,
    pub days: Vec<MealPlanDayFull>,
}

#[derive(Debug, Clone)]
pub struct MealPlanDayFull {
    pub day: MealPlanDayRow,
    pub meals: Vec<MealPlanMealFull>,
}

#[derive(Debug, Clone)]
pub struct MealPlanMealFull {
    pub meal: MealPlanMealRow,
    pub reservations: Vec<StockReservationRow>,
}

#[derive(Debug, Clone)]
pub struct NewMealPlan<'a> {
    pub title: &'a str,
    pub status: &'a str,
    pub constraints_json: &'a str,
    pub days: Vec<NewMealPlanDay<'a>>,
}

#[derive(Debug, Clone)]
pub struct NewMealPlanDay<'a> {
    pub plan_date: &'a str,
    pub meals: Vec<NewMealPlanMeal<'a>>,
}

#[derive(Debug, Clone)]
pub struct NewMealPlanMeal<'a> {
    pub slot_key: &'a str,
    pub slot_label: &'a str,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub recipe_name: Option<&'a str>,
    pub serving_scale: &'a str,
    pub status: &'a str,
    pub preflight_json: Option<&'a str>,
    pub warnings_json: &'a str,
    pub conflicts_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewStockReservation<'a> {
    pub meal_plan_id: Uuid,
    pub meal_plan_meal_id: Uuid,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub quantity: &'a str,
    pub unit: &'a str,
    pub status: &'a str,
}

pub async fn list(db: &Database, household_id: Uuid) -> Result<Vec<MealPlanRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {PLAN_COLS} FROM meal_plan \
         WHERE household_id = ? ORDER BY updated_at DESC, id DESC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_plan).collect()
}

pub async fn find(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<MealPlanFull>, sqlx::Error> {
    let sql = format!("SELECT {PLAN_COLS} FROM meal_plan WHERE household_id = ? AND id = ?");
    let Some(plan) = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?
        .map(row_to_plan)
        .transpose()?
    else {
        return Ok(None);
    };

    let mut days = Vec::new();
    for day in list_days(db, household_id, id).await? {
        let mut meals = Vec::new();
        for meal in list_meals_for_day(db, household_id, day.id).await? {
            let reservations = list_reservations_for_meal(db, household_id, meal.id).await?;
            meals.push(MealPlanMealFull { meal, reservations });
        }
        days.push(MealPlanDayFull { day, meals });
    }

    Ok(Some(MealPlanFull { plan, days }))
}

pub async fn create(
    db: &Database,
    household_id: Uuid,
    actor: Uuid,
    new: &NewMealPlan<'_>,
) -> Result<MealPlanFull, sqlx::Error> {
    let plan_id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;
    insert_plan_tx(&mut tx, household_id, actor, plan_id, new, &now).await?;
    tx.commit().await?;
    find(db, household_id, plan_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn replace(
    db: &Database,
    household_id: Uuid,
    actor: Uuid,
    id: Uuid,
    new: &NewMealPlan<'_>,
) -> Result<Option<MealPlanFull>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;
    release_plan_reservations_tx(&mut tx, household_id, id, &now).await?;
    sqlx::query("DELETE FROM meal_plan_day WHERE household_id = ? AND meal_plan_id = ?")
        .bind(household_id.to_string())
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
    let res = sqlx::query(
        "UPDATE meal_plan \
         SET title = ?, status = ?, constraints_json = ?, updated_at = ?, updated_by = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(new.title)
    .bind(new.status)
    .bind(new.constraints_json)
    .bind(&now)
    .bind(actor.to_string())
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    insert_days_tx(&mut tx, household_id, id, &new.days, &now).await?;
    tx.commit().await?;
    find(db, household_id, id).await
}

pub async fn delete(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;
    release_plan_reservations_tx(&mut tx, household_id, id, &now).await?;
    let res = sqlx::query("DELETE FROM meal_plan WHERE household_id = ? AND id = ?")
        .bind(household_id.to_string())
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(res.rows_affected() > 0)
}

pub async fn find_meal(
    db: &Database,
    household_id: Uuid,
    plan_id: Uuid,
    meal_id: Uuid,
) -> Result<Option<MealPlanMealRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {MEAL_COLS} FROM meal_plan_meal \
         WHERE household_id = ? AND meal_plan_id = ? AND id = ?"
    );
    let row = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(plan_id.to_string())
        .bind(meal_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_meal).transpose()
}

pub async fn update_meal_plan(
    db: &Database,
    household_id: Uuid,
    meal_id: Uuid,
    recipe_id: Option<Uuid>,
    recipe_version_id: Option<Uuid>,
    recipe_name: Option<&str>,
    preflight_json: Option<&str>,
    warnings_json: &str,
    conflicts_json: &str,
    status: &str,
) -> Result<Option<MealPlanMealRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let res = sqlx::query(
        "UPDATE meal_plan_meal \
         SET recipe_id = ?, recipe_version_id = ?, recipe_name = ?, preflight_json = ?, \
             warnings_json = ?, conflicts_json = ?, status = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(recipe_id.map(|id| id.to_string()))
    .bind(recipe_version_id.map(|id| id.to_string()))
    .bind(recipe_name)
    .bind(preflight_json)
    .bind(warnings_json)
    .bind(conflicts_json)
    .bind(status)
    .bind(&now)
    .bind(household_id.to_string())
    .bind(meal_id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find_meal_by_id(db, household_id, meal_id).await
}

pub async fn set_meal_status(
    db: &Database,
    household_id: Uuid,
    meal_id: Uuid,
    status: &str,
) -> Result<Option<MealPlanMealRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let res = sqlx::query(
        "UPDATE meal_plan_meal SET status = ?, updated_at = ? WHERE household_id = ? AND id = ?",
    )
    .bind(status)
    .bind(&now)
    .bind(household_id.to_string())
    .bind(meal_id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find_meal_by_id(db, household_id, meal_id).await
}

pub async fn set_plan_ai_task_id(
    db: &Database,
    household_id: Uuid,
    plan_id: Uuid,
    ai_task_id: Uuid,
) -> Result<Option<MealPlanRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let res = sqlx::query(
        "UPDATE meal_plan SET ai_task_id = ?, updated_at = ? WHERE household_id = ? AND id = ?",
    )
    .bind(ai_task_id.to_string())
    .bind(&now)
    .bind(household_id.to_string())
    .bind(plan_id.to_string())
    .execute(&db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    find_plan_row(db, household_id, plan_id).await
}

pub async fn create_reservations(
    db: &Database,
    household_id: Uuid,
    reservations: &[NewStockReservation<'_>],
) -> Result<Vec<StockReservationRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let mut out = Vec::with_capacity(reservations.len());
    let mut tx = db.pool.begin().await?;
    for reservation in reservations {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO stock_reservation \
             (id, household_id, meal_plan_id, meal_plan_meal_id, batch_id, product_id, \
              quantity, unit, status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(household_id.to_string())
        .bind(reservation.meal_plan_id.to_string())
        .bind(reservation.meal_plan_meal_id.to_string())
        .bind(reservation.batch_id.to_string())
        .bind(reservation.product_id.to_string())
        .bind(reservation.quantity)
        .bind(reservation.unit)
        .bind(reservation.status)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        out.push(id);
    }
    tx.commit().await?;

    let mut rows = Vec::with_capacity(out.len());
    for id in out {
        rows.push(
            find_reservation(db, household_id, id)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
        );
    }
    Ok(rows)
}

pub async fn active_reservations_excluding_plan(
    db: &Database,
    household_id: Uuid,
    plan_id: Option<Uuid>,
) -> Result<Vec<StockReservationRow>, sqlx::Error> {
    let mut sql = format!(
        "SELECT {RESERVATION_COLS} FROM stock_reservation \
         WHERE household_id = ? AND status = ?"
    );
    if plan_id.is_some() {
        sql.push_str(" AND meal_plan_id != ?");
    }
    let mut query = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(RESERVATION_ACTIVE);
    if let Some(plan_id) = plan_id {
        query = query.bind(plan_id.to_string());
    }
    let rows = query.fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_reservation).collect()
}

pub async fn release_reservations_for_meal(
    db: &Database,
    household_id: Uuid,
    meal_id: Uuid,
) -> Result<(), sqlx::Error> {
    let now = now_utc_rfc3339();
    sqlx::query(
        "UPDATE stock_reservation SET status = ?, updated_at = ? \
         WHERE household_id = ? AND meal_plan_meal_id = ? AND status = ?",
    )
    .bind(RESERVATION_RELEASED)
    .bind(now)
    .bind(household_id.to_string())
    .bind(meal_id.to_string())
    .bind(RESERVATION_ACTIVE)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn mark_reservations_consumed_for_meal(
    db: &Database,
    household_id: Uuid,
    meal_id: Uuid,
) -> Result<(), sqlx::Error> {
    let now = now_utc_rfc3339();
    sqlx::query(
        "UPDATE stock_reservation SET status = ?, updated_at = ? \
         WHERE household_id = ? AND meal_plan_meal_id = ? AND status = ?",
    )
    .bind(RESERVATION_CONSUMED)
    .bind(now)
    .bind(household_id.to_string())
    .bind(meal_id.to_string())
    .bind(RESERVATION_ACTIVE)
    .execute(&db.pool)
    .await?;
    Ok(())
}

async fn insert_plan_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    actor: Uuid,
    plan_id: Uuid,
    new: &NewMealPlan<'_>,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO meal_plan \
         (id, household_id, title, status, constraints_json, ai_task_id, created_at, \
          updated_at, created_by, updated_by) \
         VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?)",
    )
    .bind(plan_id.to_string())
    .bind(household_id.to_string())
    .bind(new.title)
    .bind(new.status)
    .bind(new.constraints_json)
    .bind(now)
    .bind(now)
    .bind(actor.to_string())
    .bind(actor.to_string())
    .execute(&mut **tx)
    .await?;
    insert_days_tx(tx, household_id, plan_id, &new.days, now).await
}

async fn insert_days_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    plan_id: Uuid,
    days: &[NewMealPlanDay<'_>],
    now: &str,
) -> Result<(), sqlx::Error> {
    for (day_idx, day) in days.iter().enumerate() {
        let day_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO meal_plan_day \
             (id, household_id, meal_plan_id, plan_date, sort_order, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(day_id.to_string())
        .bind(household_id.to_string())
        .bind(plan_id.to_string())
        .bind(day.plan_date)
        .bind(day_idx as i64)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        for (meal_idx, meal) in day.meals.iter().enumerate() {
            sqlx::query(
                "INSERT INTO meal_plan_meal \
                 (id, household_id, meal_plan_id, meal_plan_day_id, plan_date, slot_key, \
                  slot_label, sort_order, recipe_id, recipe_version_id, recipe_name, \
                  serving_scale, status, preflight_json, warnings_json, conflicts_json, \
                  created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(household_id.to_string())
            .bind(plan_id.to_string())
            .bind(day_id.to_string())
            .bind(day.plan_date)
            .bind(meal.slot_key)
            .bind(meal.slot_label)
            .bind(meal_idx as i64)
            .bind(meal.recipe_id.map(|id| id.to_string()))
            .bind(meal.recipe_version_id.map(|id| id.to_string()))
            .bind(meal.recipe_name)
            .bind(meal.serving_scale)
            .bind(meal.status)
            .bind(meal.preflight_json)
            .bind(meal.warnings_json)
            .bind(meal.conflicts_json)
            .bind(now)
            .bind(now)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn release_plan_reservations_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    plan_id: Uuid,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE stock_reservation SET status = ?, updated_at = ? \
         WHERE household_id = ? AND meal_plan_id = ? AND status = ?",
    )
    .bind(RESERVATION_RELEASED)
    .bind(now)
    .bind(household_id.to_string())
    .bind(plan_id.to_string())
    .bind(RESERVATION_ACTIVE)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn list_days(
    db: &Database,
    household_id: Uuid,
    plan_id: Uuid,
) -> Result<Vec<MealPlanDayRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {DAY_COLS} FROM meal_plan_day \
         WHERE household_id = ? AND meal_plan_id = ? ORDER BY sort_order ASC, plan_date ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(plan_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_day).collect()
}

async fn list_meals_for_day(
    db: &Database,
    household_id: Uuid,
    day_id: Uuid,
) -> Result<Vec<MealPlanMealRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {MEAL_COLS} FROM meal_plan_meal \
         WHERE household_id = ? AND meal_plan_day_id = ? ORDER BY sort_order ASC, id ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(day_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_meal).collect()
}

pub async fn list_reservations_for_meal(
    db: &Database,
    household_id: Uuid,
    meal_id: Uuid,
) -> Result<Vec<StockReservationRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {RESERVATION_COLS} FROM stock_reservation \
         WHERE household_id = ? AND meal_plan_meal_id = ? ORDER BY created_at ASC, id ASC"
    );
    let rows = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(meal_id.to_string())
        .fetch_all(&db.pool)
        .await?;
    rows.into_iter().map(row_to_reservation).collect()
}

async fn find_meal_by_id(
    db: &Database,
    household_id: Uuid,
    meal_id: Uuid,
) -> Result<Option<MealPlanMealRow>, sqlx::Error> {
    let sql = format!("SELECT {MEAL_COLS} FROM meal_plan_meal WHERE household_id = ? AND id = ?");
    let row = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(meal_id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_meal).transpose()
}

async fn find_plan_row(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<MealPlanRow>, sqlx::Error> {
    let sql = format!("SELECT {PLAN_COLS} FROM meal_plan WHERE household_id = ? AND id = ?");
    let row = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_plan).transpose()
}

async fn find_reservation(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<StockReservationRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {RESERVATION_COLS} FROM stock_reservation WHERE household_id = ? AND id = ?"
    );
    let row = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(id.to_string())
        .fetch_optional(&db.pool)
        .await?;
    row.map(row_to_reservation).transpose()
}

fn row_to_plan(row: sqlx::any::AnyRow) -> Result<MealPlanRow, sqlx::Error> {
    Ok(MealPlanRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        title: row.try_get("title")?,
        status: row.try_get("status")?,
        constraints_json: row.try_get("constraints_json")?,
        ai_task_id: optional_uuid_from(&row, "ai_task_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        created_by: optional_uuid_from(&row, "created_by")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
    })
}

fn row_to_day(row: sqlx::any::AnyRow) -> Result<MealPlanDayRow, sqlx::Error> {
    Ok(MealPlanDayRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        meal_plan_id: uuid_from(&row, "meal_plan_id")?,
        plan_date: row.try_get("plan_date")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_meal(row: sqlx::any::AnyRow) -> Result<MealPlanMealRow, sqlx::Error> {
    Ok(MealPlanMealRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        meal_plan_id: uuid_from(&row, "meal_plan_id")?,
        meal_plan_day_id: uuid_from(&row, "meal_plan_day_id")?,
        plan_date: row.try_get("plan_date")?,
        slot_key: row.try_get("slot_key")?,
        slot_label: row.try_get("slot_label")?,
        sort_order: row.try_get("sort_order")?,
        recipe_id: optional_uuid_from(&row, "recipe_id")?,
        recipe_version_id: optional_uuid_from(&row, "recipe_version_id")?,
        recipe_name: row.try_get("recipe_name")?,
        serving_scale: row.try_get("serving_scale")?,
        status: row.try_get("status")?,
        preflight_json: row.try_get("preflight_json")?,
        warnings_json: row.try_get("warnings_json")?,
        conflicts_json: row.try_get("conflicts_json")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_reservation(row: sqlx::any::AnyRow) -> Result<StockReservationRow, sqlx::Error> {
    Ok(StockReservationRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        meal_plan_id: uuid_from(&row, "meal_plan_id")?,
        meal_plan_meal_id: uuid_from(&row, "meal_plan_meal_id")?,
        batch_id: uuid_from(&row, "batch_id")?,
        product_id: uuid_from(&row, "product_id")?,
        quantity: row.try_get("quantity")?,
        unit: row.try_get("unit")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn uuid_from(row: &sqlx::any::AnyRow, name: &str) -> Result<Uuid, sqlx::Error> {
    let raw: String = row.try_get(name)?;
    Uuid::parse_str(&raw).map_err(|err| sqlx::Error::Decode(Box::new(err)))
}

fn optional_uuid_from(row: &sqlx::any::AnyRow, name: &str) -> Result<Option<Uuid>, sqlx::Error> {
    let raw: Option<String> = row.try_get(name)?;
    raw.map(|value| Uuid::parse_str(&value).map_err(|err| sqlx::Error::Decode(Box::new(err))))
        .transpose()
}
