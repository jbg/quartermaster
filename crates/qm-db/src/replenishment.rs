use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{audited_sql, now_utc_rfc3339, Database};

pub const AUTOMATION_OFF: &str = "off";
pub const AUTOMATION_SUGGESTIONS: &str = "suggestions";
pub const AUTOMATION_CONFIRM_TO_SUBMIT: &str = "confirm_to_submit";
pub const AUTOMATION_TRUSTED_AUTO_SUBMIT: &str = "trusted_auto_submit";

pub const DEMAND_SIGNAL_MANUAL_SHOPPING: &str = "manual_shopping";
pub const DEMAND_SIGNAL_UPCOMING_RECIPE: &str = "upcoming_recipe";
pub const DEMAND_SIGNAL_ACTIVE: &str = "active";
pub const DEMAND_SIGNAL_DISMISSED: &str = "dismissed";
pub const DEMAND_SIGNAL_FULFILLED: &str = "fulfilled";

pub const CART_RUN_STATUS_DRAFT_CREATED: &str = "draft_created";
pub const CART_RUN_STATUS_BLOCKED: &str = "blocked";
pub const CART_RUN_STATUS_SUBMITTED: &str = "submitted";

pub const GUARDRAIL_ALLOWED: &str = "allowed";
pub const GUARDRAIL_NEEDS_APPROVAL: &str = "needs_approval";
pub const GUARDRAIL_BLOCKED: &str = "blocked";

pub const CART_SOURCE_REPLENISHMENT: &str = "replenishment";

const RULE_COLS: &str = "id, household_id, product_id, location_id, minimum_quantity, \
                         target_quantity, unit, preferred_supplier_id, \
                         preferred_supplier_item_id, preferred_package_quantity, \
                         preferred_package_unit, automation_level, expiry_suppression_days, \
                         paused_at, pause_reason, spend_cap_amount, spend_cap_currency, \
                         created_by, updated_by, created_at, updated_at";
const SETTINGS_COLS: &str = "household_id, global_disabled, default_spend_cap_amount, \
                             default_spend_cap_currency, notification_lead_minutes, \
                             quiet_hours_start, quiet_hours_end, updated_by, created_at, updated_at";
const POLICY_COLS: &str = "id, household_id, supplier_id, disabled, spend_cap_amount, \
                           spend_cap_currency, quiet_hours_start, quiet_hours_end, \
                           updated_by, created_at, updated_at";
const SIGNAL_COLS: &str = "id, household_id, product_id, location_id, signal_type, status, \
                           quantity, unit, recipe_id, recipe_version_id, desired_on, \
                           supplier_id, supplier_item_id, note, metadata_json, created_by, \
                           updated_by, created_at, updated_at";
const CART_RUN_COLS: &str = "id, household_id, draft_id, order_id, supplier_id, status, source, \
                             guardrail_decision, guardrail_snapshot_json, recommendations_json, \
                             suppressions_json, ai_explanation_json, created_by, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct ReplenishmentRuleRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub minimum_quantity: String,
    pub target_quantity: String,
    pub unit: String,
    pub preferred_supplier_id: Option<String>,
    pub preferred_supplier_item_id: Option<String>,
    pub preferred_package_quantity: Option<String>,
    pub preferred_package_unit: Option<String>,
    pub automation_level: String,
    pub expiry_suppression_days: Option<i64>,
    pub paused_at: Option<String>,
    pub pause_reason: Option<String>,
    pub spend_cap_amount: Option<String>,
    pub spend_cap_currency: Option<String>,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplenishmentSettingsRow {
    pub household_id: Uuid,
    pub global_disabled: bool,
    pub default_spend_cap_amount: Option<String>,
    pub default_spend_cap_currency: Option<String>,
    pub notification_lead_minutes: i64,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplenishmentSupplierPolicyRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub supplier_id: String,
    pub disabled: bool,
    pub spend_cap_amount: Option<String>,
    pub spend_cap_currency: Option<String>,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplenishmentDemandSignalRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub signal_type: String,
    pub status: String,
    pub quantity: String,
    pub unit: String,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub desired_on: Option<String>,
    pub supplier_id: Option<String>,
    pub supplier_item_id: Option<String>,
    pub note: Option<String>,
    pub metadata_json: String,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplenishmentCartRunRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub draft_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub supplier_id: Option<String>,
    pub status: String,
    pub source: String,
    pub guardrail_decision: String,
    pub guardrail_snapshot_json: String,
    pub recommendations_json: String,
    pub suppressions_json: String,
    pub ai_explanation_json: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StockQuantityRow {
    pub batch_id: Uuid,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConsumptionQuantityRow {
    pub quantity_delta: String,
    pub unit: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewReplenishmentRule<'a> {
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub minimum_quantity: &'a str,
    pub target_quantity: &'a str,
    pub unit: &'a str,
    pub preferred_supplier_id: Option<&'a str>,
    pub preferred_supplier_item_id: Option<&'a str>,
    pub preferred_package_quantity: Option<&'a str>,
    pub preferred_package_unit: Option<&'a str>,
    pub automation_level: &'a str,
    pub expiry_suppression_days: Option<i64>,
    pub spend_cap_amount: Option<&'a str>,
    pub spend_cap_currency: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct UpdateReplenishmentRule<'a> {
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub minimum_quantity: &'a str,
    pub target_quantity: &'a str,
    pub unit: &'a str,
    pub preferred_supplier_id: Option<&'a str>,
    pub preferred_supplier_item_id: Option<&'a str>,
    pub preferred_package_quantity: Option<&'a str>,
    pub preferred_package_unit: Option<&'a str>,
    pub automation_level: &'a str,
    pub expiry_suppression_days: Option<i64>,
    pub spend_cap_amount: Option<&'a str>,
    pub spend_cap_currency: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct UpsertReplenishmentSettings<'a> {
    pub global_disabled: bool,
    pub default_spend_cap_amount: Option<&'a str>,
    pub default_spend_cap_currency: Option<&'a str>,
    pub notification_lead_minutes: i64,
    pub quiet_hours_start: Option<&'a str>,
    pub quiet_hours_end: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct UpsertSupplierPolicy<'a> {
    pub supplier_id: &'a str,
    pub disabled: bool,
    pub spend_cap_amount: Option<&'a str>,
    pub spend_cap_currency: Option<&'a str>,
    pub quiet_hours_start: Option<&'a str>,
    pub quiet_hours_end: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewDemandSignal<'a> {
    pub product_id: Uuid,
    pub location_id: Option<Uuid>,
    pub signal_type: &'a str,
    pub quantity: &'a str,
    pub unit: &'a str,
    pub recipe_id: Option<Uuid>,
    pub recipe_version_id: Option<Uuid>,
    pub desired_on: Option<&'a str>,
    pub supplier_id: Option<&'a str>,
    pub supplier_item_id: Option<&'a str>,
    pub note: Option<&'a str>,
    pub metadata_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewCartRun<'a> {
    pub draft_id: Option<Uuid>,
    pub supplier_id: Option<&'a str>,
    pub status: &'a str,
    pub source: &'a str,
    pub guardrail_decision: &'a str,
    pub guardrail_snapshot_json: &'a str,
    pub recommendations_json: &'a str,
    pub suppressions_json: &'a str,
    pub ai_explanation_json: Option<&'a str>,
}

pub async fn list_rules(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ReplenishmentRuleRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(format!(
        "SELECT {RULE_COLS} FROM replenishment_rule \
         WHERE household_id = ? ORDER BY created_at DESC, id DESC"
    )))
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_rule).collect()
}

pub async fn find_rule(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<ReplenishmentRuleRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {RULE_COLS} FROM replenishment_rule WHERE household_id = ? AND id = ?"
    )))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_rule).transpose()
}

pub async fn create_rule(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    new: &NewReplenishmentRule<'_>,
) -> Result<ReplenishmentRuleRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO replenishment_rule \
         (id, household_id, product_id, location_id, minimum_quantity, target_quantity, unit, \
          preferred_supplier_id, preferred_supplier_item_id, preferred_package_quantity, \
          preferred_package_unit, automation_level, expiry_suppression_days, paused_at, \
          pause_reason, spend_cap_amount, spend_cap_currency, created_by, updated_by, \
          created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.product_id.to_string())
    .bind(new.location_id.map(|id| id.to_string()))
    .bind(new.minimum_quantity)
    .bind(new.target_quantity)
    .bind(new.unit)
    .bind(new.preferred_supplier_id)
    .bind(new.preferred_supplier_item_id)
    .bind(new.preferred_package_quantity)
    .bind(new.preferred_package_unit)
    .bind(new.automation_level)
    .bind(new.expiry_suppression_days)
    .bind(new.spend_cap_amount)
    .bind(new.spend_cap_currency)
    .bind(actor_id.to_string())
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_rule(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn update_rule(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    actor_id: Uuid,
    update: &UpdateReplenishmentRule<'_>,
) -> Result<Option<ReplenishmentRuleRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let result = sqlx::query(
        "UPDATE replenishment_rule \
         SET product_id = ?, location_id = ?, minimum_quantity = ?, target_quantity = ?, \
             unit = ?, preferred_supplier_id = ?, preferred_supplier_item_id = ?, \
             preferred_package_quantity = ?, preferred_package_unit = ?, automation_level = ?, \
             expiry_suppression_days = ?, spend_cap_amount = ?, spend_cap_currency = ?, \
             updated_by = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(update.product_id.to_string())
    .bind(update.location_id.map(|id| id.to_string()))
    .bind(update.minimum_quantity)
    .bind(update.target_quantity)
    .bind(update.unit)
    .bind(update.preferred_supplier_id)
    .bind(update.preferred_supplier_item_id)
    .bind(update.preferred_package_quantity)
    .bind(update.preferred_package_unit)
    .bind(update.automation_level)
    .bind(update.expiry_suppression_days)
    .bind(update.spend_cap_amount)
    .bind(update.spend_cap_currency)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if result.rows_affected() == 0 {
        Ok(None)
    } else {
        find_rule(db, household_id, id).await
    }
}

pub async fn set_rule_paused(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    actor_id: Uuid,
    paused: bool,
    reason: Option<&str>,
) -> Result<Option<ReplenishmentRuleRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let result = sqlx::query(
        "UPDATE replenishment_rule \
         SET paused_at = ?, pause_reason = ?, updated_by = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(if paused { Some(now.as_str()) } else { None })
    .bind(if paused { reason } else { None })
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if result.rows_affected() == 0 {
        Ok(None)
    } else {
        find_rule(db, household_id, id).await
    }
}

pub async fn delete_rule(db: &Database, household_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM replenishment_rule WHERE household_id = ? AND id = ?")
        .bind(household_id.to_string())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_or_create_settings(
    db: &Database,
    household_id: Uuid,
) -> Result<ReplenishmentSettingsRow, sqlx::Error> {
    if let Some(settings) = find_settings(db, household_id).await? {
        return Ok(settings);
    }
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO replenishment_settings \
         (household_id, global_disabled, default_spend_cap_amount, default_spend_cap_currency, \
          notification_lead_minutes, quiet_hours_start, quiet_hours_end, updated_by, created_at, updated_at) \
         VALUES (?, 0, NULL, NULL, 0, NULL, NULL, NULL, ?, ?)",
    )
    .bind(household_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_settings(db, household_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_settings(
    db: &Database,
    household_id: Uuid,
) -> Result<Option<ReplenishmentSettingsRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {SETTINGS_COLS} FROM replenishment_settings WHERE household_id = ?"
    )))
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_settings).transpose()
}

pub async fn upsert_settings(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    settings: &UpsertReplenishmentSettings<'_>,
) -> Result<ReplenishmentSettingsRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO replenishment_settings \
         (household_id, global_disabled, default_spend_cap_amount, default_spend_cap_currency, \
          notification_lead_minutes, quiet_hours_start, quiet_hours_end, updated_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(household_id) DO UPDATE SET \
             global_disabled = excluded.global_disabled, \
             default_spend_cap_amount = excluded.default_spend_cap_amount, \
             default_spend_cap_currency = excluded.default_spend_cap_currency, \
             notification_lead_minutes = excluded.notification_lead_minutes, \
             quiet_hours_start = excluded.quiet_hours_start, \
             quiet_hours_end = excluded.quiet_hours_end, \
             updated_by = excluded.updated_by, updated_at = excluded.updated_at",
    )
    .bind(household_id.to_string())
    .bind(if settings.global_disabled { 1 } else { 0 })
    .bind(settings.default_spend_cap_amount)
    .bind(settings.default_spend_cap_currency)
    .bind(settings.notification_lead_minutes)
    .bind(settings.quiet_hours_start)
    .bind(settings.quiet_hours_end)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_settings(db, household_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_supplier_policy(
    db: &Database,
    household_id: Uuid,
    supplier_id: &str,
) -> Result<Option<ReplenishmentSupplierPolicyRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {POLICY_COLS} FROM replenishment_supplier_policy \
         WHERE household_id = ? AND supplier_id = ?"
    )))
    .bind(household_id.to_string())
    .bind(supplier_id)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_policy).transpose()
}

pub async fn upsert_supplier_policy(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    policy: &UpsertSupplierPolicy<'_>,
) -> Result<ReplenishmentSupplierPolicyRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    let existing = sqlx::query(
        "SELECT id FROM replenishment_supplier_policy WHERE household_id = ? AND supplier_id = ?",
    )
    .bind(household_id.to_string())
    .bind(policy.supplier_id)
    .fetch_optional(&db.pool)
    .await?;
    let id = existing
        .as_ref()
        .map(|row| uuid_from(row, "id"))
        .transpose()?
        .unwrap_or_else(Uuid::now_v7);
    sqlx::query(
        "INSERT INTO replenishment_supplier_policy \
         (id, household_id, supplier_id, disabled, spend_cap_amount, spend_cap_currency, \
          quiet_hours_start, quiet_hours_end, updated_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(household_id, supplier_id) DO UPDATE SET \
             disabled = excluded.disabled, spend_cap_amount = excluded.spend_cap_amount, \
             spend_cap_currency = excluded.spend_cap_currency, \
             quiet_hours_start = excluded.quiet_hours_start, quiet_hours_end = excluded.quiet_hours_end, \
             updated_by = excluded.updated_by, updated_at = excluded.updated_at",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(policy.supplier_id)
    .bind(if policy.disabled { 1 } else { 0 })
    .bind(policy.spend_cap_amount)
    .bind(policy.spend_cap_currency)
    .bind(policy.quiet_hours_start)
    .bind(policy.quiet_hours_end)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_supplier_policy(db, household_id, policy.supplier_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn create_demand_signal(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    new: &NewDemandSignal<'_>,
) -> Result<ReplenishmentDemandSignalRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO replenishment_demand_signal \
         (id, household_id, product_id, location_id, signal_type, status, quantity, unit, \
          recipe_id, recipe_version_id, desired_on, supplier_id, supplier_item_id, note, \
          metadata_json, created_by, updated_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.product_id.to_string())
    .bind(new.location_id.map(|id| id.to_string()))
    .bind(new.signal_type)
    .bind(DEMAND_SIGNAL_ACTIVE)
    .bind(new.quantity)
    .bind(new.unit)
    .bind(new.recipe_id.map(|id| id.to_string()))
    .bind(new.recipe_version_id.map(|id| id.to_string()))
    .bind(new.desired_on)
    .bind(new.supplier_id)
    .bind(new.supplier_item_id)
    .bind(new.note)
    .bind(new.metadata_json)
    .bind(actor_id.to_string())
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_demand_signal(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_demand_signals(
    db: &Database,
    household_id: Uuid,
    active_only: bool,
) -> Result<Vec<ReplenishmentDemandSignalRow>, sqlx::Error> {
    let mut sql =
        format!("SELECT {SIGNAL_COLS} FROM replenishment_demand_signal WHERE household_id = ? ");
    if active_only {
        sql.push_str("AND status = ? ");
    }
    sql.push_str("ORDER BY created_at DESC, id DESC");
    let mut query = sqlx::query(audited_sql(sql)).bind(household_id.to_string());
    if active_only {
        query = query.bind(DEMAND_SIGNAL_ACTIVE);
    }
    let rows = query.fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_signal).collect()
}

pub async fn find_demand_signal(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<ReplenishmentDemandSignalRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {SIGNAL_COLS} FROM replenishment_demand_signal WHERE household_id = ? AND id = ?"
    )))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_signal).transpose()
}

pub async fn update_demand_signal_status(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    actor_id: Uuid,
    status: &str,
) -> Result<Option<ReplenishmentDemandSignalRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let result = sqlx::query(
        "UPDATE replenishment_demand_signal \
         SET status = ?, updated_by = ?, updated_at = ? WHERE household_id = ? AND id = ?",
    )
    .bind(status)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if result.rows_affected() == 0 {
        Ok(None)
    } else {
        find_demand_signal(db, household_id, id).await
    }
}

pub async fn create_cart_run(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    new: &NewCartRun<'_>,
) -> Result<ReplenishmentCartRunRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO replenishment_cart_run \
         (id, household_id, draft_id, order_id, supplier_id, status, source, guardrail_decision, \
          guardrail_snapshot_json, recommendations_json, suppressions_json, ai_explanation_json, \
          created_by, created_at, updated_at) \
         VALUES (?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.draft_id.map(|id| id.to_string()))
    .bind(new.supplier_id)
    .bind(new.status)
    .bind(new.source)
    .bind(new.guardrail_decision)
    .bind(new.guardrail_snapshot_json)
    .bind(new.recommendations_json)
    .bind(new.suppressions_json)
    .bind(new.ai_explanation_json)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_cart_run(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_cart_run(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<ReplenishmentCartRunRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {CART_RUN_COLS} FROM replenishment_cart_run WHERE household_id = ? AND id = ?"
    )))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_cart_run).transpose()
}

pub async fn find_cart_run_for_draft(
    db: &Database,
    household_id: Uuid,
    draft_id: Uuid,
) -> Result<Option<ReplenishmentCartRunRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {CART_RUN_COLS} FROM replenishment_cart_run \
         WHERE household_id = ? AND draft_id = ? ORDER BY created_at DESC, id DESC LIMIT 1"
    )))
    .bind(household_id.to_string())
    .bind(draft_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_cart_run).transpose()
}

pub async fn mark_cart_run_submitted(
    db: &Database,
    household_id: Uuid,
    draft_id: Uuid,
    order_id: Uuid,
    guardrail_snapshot_json: &str,
) -> Result<Option<ReplenishmentCartRunRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let result = sqlx::query(
        "UPDATE replenishment_cart_run \
         SET status = ?, order_id = ?, guardrail_decision = ?, guardrail_snapshot_json = ?, \
             updated_at = ? \
         WHERE household_id = ? AND draft_id = ?",
    )
    .bind(CART_RUN_STATUS_SUBMITTED)
    .bind(order_id.to_string())
    .bind(GUARDRAIL_ALLOWED)
    .bind(guardrail_snapshot_json)
    .bind(&now)
    .bind(household_id.to_string())
    .bind(draft_id.to_string())
    .execute(&db.pool)
    .await?;
    if result.rows_affected() == 0 {
        Ok(None)
    } else {
        find_cart_run_for_draft(db, household_id, draft_id).await
    }
}

pub async fn active_stock_for_product(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    location_id: Option<Uuid>,
) -> Result<Vec<StockQuantityRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT id, quantity, unit, expires_on FROM stock_batch \
         WHERE household_id = ? AND product_id = ? AND depleted_at IS NULL",
    );
    if location_id.is_some() {
        sql.push_str(" AND location_id = ?");
    }
    let mut query = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(product_id.to_string());
    if let Some(location_id) = location_id {
        query = query.bind(location_id.to_string());
    }
    let rows = query.fetch_all(&db.pool).await?;
    rows.into_iter().map(row_to_stock_quantity).collect()
}

pub async fn consumption_for_product_since(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    since: &str,
) -> Result<Vec<ConsumptionQuantityRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(String::from(
        "SELECT e.quantity_delta, b.unit, e.created_at \
         FROM stock_event e \
         INNER JOIN stock_batch b ON b.id = e.batch_id \
         WHERE e.household_id = ? AND b.product_id = ? AND e.event_type = ? AND e.created_at >= ? \
         ORDER BY e.created_at ASC, e.id ASC",
    )))
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(crate::stock_events::EVENT_CONSUME)
    .bind(since)
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_consumption).collect()
}

pub async fn find_confirmed_mapping_for_product(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    supplier_id: Option<&str>,
) -> Result<Option<crate::suppliers::ProductSupplierMappingRow>, sqlx::Error> {
    let mut sql = String::from(
        "SELECT id, household_id, product_id, supplier_id, supplier_item_id, confidence, \
                confirmed_at, substitute_policy_json, created_by, updated_by, created_at, updated_at \
         FROM product_supplier_mapping \
         WHERE household_id = ? AND product_id = ? AND confidence = ? ",
    );
    if supplier_id.is_some() {
        sql.push_str("AND supplier_id = ? ");
    }
    sql.push_str("ORDER BY confirmed_at DESC, updated_at DESC LIMIT 1");
    let mut query = sqlx::query(audited_sql(sql))
        .bind(household_id.to_string())
        .bind(product_id.to_string())
        .bind("confirmed");
    if let Some(supplier_id) = supplier_id {
        query = query.bind(supplier_id);
    }
    let row = query.fetch_optional(&db.pool).await?;
    row.map(row_to_mapping).transpose()
}

pub async fn pending_replenishment_exists(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let draft = sqlx::query(
        "SELECT 1 AS x \
         FROM supplier_cart_draft d \
         INNER JOIN supplier_cart_line l ON l.draft_id = d.id \
         WHERE d.household_id = ? AND l.product_id = ? AND d.source = ? \
           AND d.status IN (?, ?, ?) \
         LIMIT 1",
    )
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(CART_SOURCE_REPLENISHMENT)
    .bind(crate::suppliers::CART_STATUS_DRAFT)
    .bind(crate::suppliers::CART_STATUS_NEEDS_REVIEW)
    .bind(crate::suppliers::CART_STATUS_READY)
    .fetch_optional(&db.pool)
    .await?;
    if draft.is_some() {
        return Ok(true);
    }

    let order = sqlx::query(
        "SELECT 1 AS x \
         FROM supplier_order o \
         INNER JOIN supplier_cart_line l ON l.draft_id = o.draft_id \
         WHERE o.household_id = ? AND l.product_id = ? \
           AND o.status NOT IN (?, ?, ?) \
         LIMIT 1",
    )
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(crate::suppliers::ORDER_STATUS_DELIVERED)
    .bind("cancelled")
    .bind("failed")
    .fetch_optional(&db.pool)
    .await?;
    Ok(order.is_some())
}

fn row_to_rule(row: sqlx::any::AnyRow) -> Result<ReplenishmentRuleRow, sqlx::Error> {
    Ok(ReplenishmentRuleRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        product_id: uuid_from(&row, "product_id")?,
        location_id: optional_uuid_from(&row, "location_id")?,
        minimum_quantity: row.try_get("minimum_quantity")?,
        target_quantity: row.try_get("target_quantity")?,
        unit: row.try_get("unit")?,
        preferred_supplier_id: row.try_get("preferred_supplier_id")?,
        preferred_supplier_item_id: row.try_get("preferred_supplier_item_id")?,
        preferred_package_quantity: row.try_get("preferred_package_quantity")?,
        preferred_package_unit: row.try_get("preferred_package_unit")?,
        automation_level: row.try_get("automation_level")?,
        expiry_suppression_days: row.try_get("expiry_suppression_days")?,
        paused_at: row.try_get("paused_at")?,
        pause_reason: row.try_get("pause_reason")?,
        spend_cap_amount: row.try_get("spend_cap_amount")?,
        spend_cap_currency: row.try_get("spend_cap_currency")?,
        created_by: optional_uuid_from(&row, "created_by")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_settings(row: sqlx::any::AnyRow) -> Result<ReplenishmentSettingsRow, sqlx::Error> {
    Ok(ReplenishmentSettingsRow {
        household_id: uuid_from(&row, "household_id")?,
        global_disabled: row_bool(&row, "global_disabled")?,
        default_spend_cap_amount: row.try_get("default_spend_cap_amount")?,
        default_spend_cap_currency: row.try_get("default_spend_cap_currency")?,
        notification_lead_minutes: row.try_get("notification_lead_minutes")?,
        quiet_hours_start: row.try_get("quiet_hours_start")?,
        quiet_hours_end: row.try_get("quiet_hours_end")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_policy(row: sqlx::any::AnyRow) -> Result<ReplenishmentSupplierPolicyRow, sqlx::Error> {
    Ok(ReplenishmentSupplierPolicyRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        supplier_id: row.try_get("supplier_id")?,
        disabled: row_bool(&row, "disabled")?,
        spend_cap_amount: row.try_get("spend_cap_amount")?,
        spend_cap_currency: row.try_get("spend_cap_currency")?,
        quiet_hours_start: row.try_get("quiet_hours_start")?,
        quiet_hours_end: row.try_get("quiet_hours_end")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_signal(row: sqlx::any::AnyRow) -> Result<ReplenishmentDemandSignalRow, sqlx::Error> {
    Ok(ReplenishmentDemandSignalRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        product_id: uuid_from(&row, "product_id")?,
        location_id: optional_uuid_from(&row, "location_id")?,
        signal_type: row.try_get("signal_type")?,
        status: row.try_get("status")?,
        quantity: row.try_get("quantity")?,
        unit: row.try_get("unit")?,
        recipe_id: optional_uuid_from(&row, "recipe_id")?,
        recipe_version_id: optional_uuid_from(&row, "recipe_version_id")?,
        desired_on: row.try_get("desired_on")?,
        supplier_id: row.try_get("supplier_id")?,
        supplier_item_id: row.try_get("supplier_item_id")?,
        note: row.try_get("note")?,
        metadata_json: row.try_get("metadata_json")?,
        created_by: optional_uuid_from(&row, "created_by")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_cart_run(row: sqlx::any::AnyRow) -> Result<ReplenishmentCartRunRow, sqlx::Error> {
    Ok(ReplenishmentCartRunRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        draft_id: optional_uuid_from(&row, "draft_id")?,
        order_id: optional_uuid_from(&row, "order_id")?,
        supplier_id: row.try_get("supplier_id")?,
        status: row.try_get("status")?,
        source: row.try_get("source")?,
        guardrail_decision: row.try_get("guardrail_decision")?,
        guardrail_snapshot_json: row.try_get("guardrail_snapshot_json")?,
        recommendations_json: row.try_get("recommendations_json")?,
        suppressions_json: row.try_get("suppressions_json")?,
        ai_explanation_json: row.try_get("ai_explanation_json")?,
        created_by: optional_uuid_from(&row, "created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_stock_quantity(row: sqlx::any::AnyRow) -> Result<StockQuantityRow, sqlx::Error> {
    Ok(StockQuantityRow {
        batch_id: uuid_from(&row, "id")?,
        quantity: row.try_get("quantity")?,
        unit: row.try_get("unit")?,
        expires_on: row.try_get("expires_on")?,
    })
}

fn row_to_consumption(row: sqlx::any::AnyRow) -> Result<ConsumptionQuantityRow, sqlx::Error> {
    Ok(ConsumptionQuantityRow {
        quantity_delta: row.try_get("quantity_delta")?,
        unit: row.try_get("unit")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_mapping(
    row: sqlx::any::AnyRow,
) -> Result<crate::suppliers::ProductSupplierMappingRow, sqlx::Error> {
    Ok(crate::suppliers::ProductSupplierMappingRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        product_id: uuid_from(&row, "product_id")?,
        supplier_id: row.try_get("supplier_id")?,
        supplier_item_id: row.try_get("supplier_item_id")?,
        confidence: row.try_get("confidence")?,
        confirmed_at: row.try_get("confirmed_at")?,
        substitute_policy_json: row.try_get("substitute_policy_json")?,
        created_by: optional_uuid_from(&row, "created_by")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_bool(row: &sqlx::any::AnyRow, name: &str) -> Result<bool, sqlx::Error> {
    Ok(row.try_get::<i64, _>(name)? != 0)
}

fn uuid_from(row: &sqlx::any::AnyRow, name: &str) -> Result<Uuid, sqlx::Error> {
    let value: String = row.try_get(name)?;
    Uuid::parse_str(&value).map_err(|err| sqlx::Error::Decode(Box::new(err)))
}

fn optional_uuid_from(row: &sqlx::any::AnyRow, name: &str) -> Result<Option<Uuid>, sqlx::Error> {
    let value: Option<String> = row.try_get(name)?;
    value
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|err| sqlx::Error::Decode(Box::new(err)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{households, products, suppliers, test_support, users};

    #[tokio::test]
    async fn replenishment_persistence_round_trips() {
        let db = test_support::sqlite().await.into_db();
        let user = users::create(&db, "replenishment@example.com", "Replenishment", "hash")
            .await
            .unwrap();
        let household = households::create(&db, "Kitchen", "UTC").await.unwrap();
        let product = products::create_manual(
            &db,
            household.id,
            "Rice",
            None,
            "mass",
            Some("g"),
            None,
            None,
        )
        .await
        .unwrap();
        suppliers::upsert_supplier(
            &db,
            &suppliers::NewSupplier {
                id: suppliers::SUPPLIER_MOCK,
                display_name: "Mock Supplier",
                capabilities_json: "[]",
                requirements_json: "[]",
                supported_regions_json: "[]",
                terms_url: None,
                needs_network: false,
                needs_browser: false,
            },
        )
        .await
        .unwrap();

        let rule = create_rule(
            &db,
            household.id,
            user.id,
            &NewReplenishmentRule {
                product_id: product.id,
                location_id: None,
                minimum_quantity: "500",
                target_quantity: "1500",
                unit: "g",
                preferred_supplier_id: Some(suppliers::SUPPLIER_MOCK),
                preferred_supplier_item_id: Some("mock-rice-1kg"),
                preferred_package_quantity: Some("1000"),
                preferred_package_unit: Some("g"),
                automation_level: AUTOMATION_CONFIRM_TO_SUBMIT,
                expiry_suppression_days: Some(3),
                spend_cap_amount: Some("20.00"),
                spend_cap_currency: Some("USD"),
            },
        )
        .await
        .unwrap();
        assert_eq!(rule.product_id, product.id);
        assert_eq!(rule.automation_level, AUTOMATION_CONFIRM_TO_SUBMIT);

        let paused = set_rule_paused(&db, household.id, rule.id, user.id, true, Some("vacation"))
            .await
            .unwrap()
            .unwrap();
        assert!(paused.paused_at.is_some());
        assert_eq!(paused.pause_reason.as_deref(), Some("vacation"));

        let settings = upsert_settings(
            &db,
            household.id,
            user.id,
            &UpsertReplenishmentSettings {
                global_disabled: true,
                default_spend_cap_amount: Some("75.00"),
                default_spend_cap_currency: Some("USD"),
                notification_lead_minutes: 30,
                quiet_hours_start: Some("22:00"),
                quiet_hours_end: Some("07:00"),
            },
        )
        .await
        .unwrap();
        assert!(settings.global_disabled);

        let policy = upsert_supplier_policy(
            &db,
            household.id,
            user.id,
            &UpsertSupplierPolicy {
                supplier_id: suppliers::SUPPLIER_MOCK,
                disabled: true,
                spend_cap_amount: Some("50.00"),
                spend_cap_currency: Some("USD"),
                quiet_hours_start: None,
                quiet_hours_end: None,
            },
        )
        .await
        .unwrap();
        assert!(policy.disabled);

        let signal = create_demand_signal(
            &db,
            household.id,
            user.id,
            &NewDemandSignal {
                product_id: product.id,
                location_id: None,
                signal_type: DEMAND_SIGNAL_MANUAL_SHOPPING,
                quantity: "250",
                unit: "g",
                recipe_id: None,
                recipe_version_id: None,
                desired_on: None,
                supplier_id: None,
                supplier_item_id: None,
                note: Some("shopping list"),
                metadata_json: "{}",
            },
        )
        .await
        .unwrap();
        assert_eq!(signal.status, DEMAND_SIGNAL_ACTIVE);
        assert_eq!(
            list_demand_signals(&db, household.id, true)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
