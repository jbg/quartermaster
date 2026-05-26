use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::{audited_sql, now_utc_rfc3339, Database};

pub const SUPPLIER_MOCK: &str = "mock";

pub const ACCOUNT_STATUS_ACTIVE: &str = "active";
pub const ACCOUNT_STATUS_NEEDS_CONFIGURATION: &str = "needs_configuration";
pub const ACCOUNT_STATUS_DISABLED: &str = "disabled";

pub const CART_STATUS_DRAFT: &str = "draft";
pub const CART_STATUS_NEEDS_REVIEW: &str = "needs_review";
pub const CART_STATUS_READY: &str = "ready";
pub const CART_STATUS_SUBMITTED: &str = "submitted";
pub const CART_STATUS_CANCELLED: &str = "cancelled";

pub const ORDER_STATUS_SUBMITTED: &str = "submitted";
pub const ORDER_STATUS_DELIVERED: &str = "delivered";

const SUPPLIER_COLS: &str = "id, display_name, capabilities_json, requirements_json, \
                             supported_regions_json, terms_url, needs_network, needs_browser, \
                             enabled, created_at, updated_at";
const ACCOUNT_COLS: &str = "id, household_id, supplier_id, display_name, status, region_json, \
                            config_json, consent_accepted_at, created_by, updated_by, \
                            created_at, updated_at";
const SECRET_COLS: &str =
    "account_id, secret_name, secret_kind, redacted_hint, created_at, updated_at";
const CATALOG_COLS: &str =
    "id, supplier_id, supplier_item_id, name, brand, image_url, detail_url, \
                            availability, price_amount, price_currency, pack_quantity, pack_unit, \
                            lead_time_min_days, lead_time_max_days, minimum_order_quantity, \
                            minimum_order_unit, metadata_json, fetched_at";
const MAPPING_COLS: &str =
    "id, household_id, product_id, supplier_id, supplier_item_id, confidence, \
                            confirmed_at, substitute_policy_json, created_by, updated_by, \
                            created_at, updated_at";
const DRAFT_COLS: &str = "id, household_id, account_id, supplier_id, status, source, \
                           intervention_state, review_notes, created_by, updated_by, \
                           created_at, updated_at";
const LINE_COLS: &str =
    "id, draft_id, household_id, product_id, supplier_item_id, quantity, unit, \
                          note, sort_order, created_at";
const ORDER_COLS: &str = "id, household_id, draft_id, account_id, supplier_id, supplier_order_id, \
                           status, review_url, redacted_summary_json, submitted_at, delivered_at, \
                           created_by, created_at, updated_at";

#[derive(Debug, Clone, Serialize)]
pub struct SupplierRow {
    pub id: String,
    pub display_name: String,
    pub capabilities_json: String,
    pub requirements_json: String,
    pub supported_regions_json: String,
    pub terms_url: Option<String>,
    pub needs_network: bool,
    pub needs_browser: bool,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplierAccountRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub supplier_id: String,
    pub display_name: String,
    pub status: String,
    pub region_json: Option<String>,
    pub config_json: String,
    pub consent_accepted_at: Option<String>,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplierAccountSecretRow {
    pub account_id: Uuid,
    pub secret_name: String,
    pub secret_kind: String,
    pub redacted_hint: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplierCatalogItemRow {
    pub id: Uuid,
    pub supplier_id: String,
    pub supplier_item_id: String,
    pub name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub detail_url: Option<String>,
    pub availability: String,
    pub price_amount: Option<String>,
    pub price_currency: Option<String>,
    pub pack_quantity: Option<String>,
    pub pack_unit: Option<String>,
    pub lead_time_min_days: Option<i64>,
    pub lead_time_max_days: Option<i64>,
    pub minimum_order_quantity: Option<String>,
    pub minimum_order_unit: Option<String>,
    pub metadata_json: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductSupplierMappingRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub product_id: Uuid,
    pub supplier_id: String,
    pub supplier_item_id: String,
    pub confidence: String,
    pub confirmed_at: Option<String>,
    pub substitute_policy_json: String,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplierCartDraftRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub account_id: Option<Uuid>,
    pub supplier_id: String,
    pub status: String,
    pub source: String,
    pub intervention_state: String,
    pub review_notes: Option<String>,
    pub created_by: Option<Uuid>,
    pub updated_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplierCartLineRow {
    pub id: Uuid,
    pub draft_id: Uuid,
    pub household_id: Uuid,
    pub product_id: Option<Uuid>,
    pub supplier_item_id: String,
    pub quantity: String,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupplierOrderRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub draft_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub supplier_id: String,
    pub supplier_order_id: Option<String>,
    pub status: String,
    pub review_url: Option<String>,
    pub redacted_summary_json: String,
    pub submitted_at: Option<String>,
    pub delivered_at: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct NewSupplier<'a> {
    pub id: &'a str,
    pub display_name: &'a str,
    pub capabilities_json: &'a str,
    pub requirements_json: &'a str,
    pub supported_regions_json: &'a str,
    pub terms_url: Option<&'a str>,
    pub needs_network: bool,
    pub needs_browser: bool,
}

#[derive(Debug, Clone)]
pub struct NewSupplierAccount<'a> {
    pub supplier_id: &'a str,
    pub display_name: &'a str,
    pub status: &'a str,
    pub region_json: Option<&'a str>,
    pub config_json: &'a str,
    pub consent_accepted_at: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewCatalogItem<'a> {
    pub supplier_id: &'a str,
    pub supplier_item_id: &'a str,
    pub name: &'a str,
    pub brand: Option<&'a str>,
    pub image_url: Option<&'a str>,
    pub detail_url: Option<&'a str>,
    pub availability: &'a str,
    pub price_amount: Option<&'a str>,
    pub price_currency: Option<&'a str>,
    pub pack_quantity: Option<&'a str>,
    pub pack_unit: Option<&'a str>,
    pub lead_time_min_days: Option<i64>,
    pub lead_time_max_days: Option<i64>,
    pub minimum_order_quantity: Option<&'a str>,
    pub minimum_order_unit: Option<&'a str>,
    pub metadata_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewMapping<'a> {
    pub product_id: Uuid,
    pub supplier_id: &'a str,
    pub supplier_item_id: &'a str,
    pub confidence: &'a str,
    pub confirmed_at: Option<&'a str>,
    pub substitute_policy_json: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewCartDraft<'a> {
    pub account_id: Option<Uuid>,
    pub supplier_id: &'a str,
    pub status: &'a str,
    pub source: &'a str,
    pub intervention_state: &'a str,
    pub review_notes: Option<&'a str>,
    pub lines: Vec<NewCartLine<'a>>,
}

#[derive(Debug, Clone)]
pub struct NewCartLine<'a> {
    pub product_id: Option<Uuid>,
    pub supplier_item_id: &'a str,
    pub quantity: &'a str,
    pub unit: Option<&'a str>,
    pub note: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct NewOrder<'a> {
    pub draft_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub supplier_id: &'a str,
    pub supplier_order_id: Option<&'a str>,
    pub status: &'a str,
    pub review_url: Option<&'a str>,
    pub redacted_summary_json: &'a str,
    pub submitted_at: Option<&'a str>,
}

pub async fn upsert_supplier(
    db: &Database,
    supplier: &NewSupplier<'_>,
) -> Result<SupplierRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO supplier \
         (id, display_name, capabilities_json, requirements_json, supported_regions_json, \
          terms_url, needs_network, needs_browser, enabled, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
             display_name = excluded.display_name, \
             capabilities_json = excluded.capabilities_json, \
             requirements_json = excluded.requirements_json, \
             supported_regions_json = excluded.supported_regions_json, \
             terms_url = excluded.terms_url, \
             needs_network = excluded.needs_network, \
             needs_browser = excluded.needs_browser, \
             enabled = 1, \
             updated_at = excluded.updated_at",
    )
    .bind(supplier.id)
    .bind(supplier.display_name)
    .bind(supplier.capabilities_json)
    .bind(supplier.requirements_json)
    .bind(supplier.supported_regions_json)
    .bind(supplier.terms_url)
    .bind(if supplier.needs_network { 1 } else { 0 })
    .bind(if supplier.needs_browser { 1 } else { 0 })
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_supplier(db, supplier.id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_suppliers(db: &Database) -> Result<Vec<SupplierRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(format!(
        "SELECT {SUPPLIER_COLS} FROM supplier WHERE enabled = 1 ORDER BY display_name ASC"
    )))
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_supplier).collect()
}

pub async fn find_supplier(db: &Database, id: &str) -> Result<Option<SupplierRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {SUPPLIER_COLS} FROM supplier WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_supplier).transpose()
}

pub async fn create_account(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    new: &NewSupplierAccount<'_>,
) -> Result<SupplierAccountRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO supplier_account \
         (id, household_id, supplier_id, display_name, status, region_json, config_json, \
          consent_accepted_at, created_by, updated_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.supplier_id)
    .bind(new.display_name)
    .bind(new.status)
    .bind(new.region_json)
    .bind(new.config_json)
    .bind(new.consent_accepted_at)
    .bind(actor_id.to_string())
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_account(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_accounts(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<SupplierAccountRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(format!(
        "SELECT {ACCOUNT_COLS} FROM supplier_account \
         WHERE household_id = ? ORDER BY created_at DESC, id DESC"
    )))
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_account).collect()
}

pub async fn find_account(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<SupplierAccountRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {ACCOUNT_COLS} FROM supplier_account WHERE household_id = ? AND id = ?"
    )))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_account).transpose()
}

pub async fn update_account(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    actor_id: Uuid,
    display_name: &str,
    status: &str,
    region_json: Option<&str>,
    config_json: &str,
    consent_accepted_at: Option<&str>,
) -> Result<Option<SupplierAccountRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let updated = sqlx::query(
        "UPDATE supplier_account \
         SET display_name = ?, status = ?, region_json = ?, config_json = ?, \
             consent_accepted_at = ?, updated_by = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(display_name)
    .bind(status)
    .bind(region_json)
    .bind(config_json)
    .bind(consent_accepted_at)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if updated.rows_affected() == 0 {
        Ok(None)
    } else {
        find_account(db, household_id, id).await
    }
}

pub async fn delete_account(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM supplier_account WHERE household_id = ? AND id = ?")
        .bind(household_id.to_string())
        .bind(id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn upsert_account_secret(
    db: &Database,
    household_id: Uuid,
    account_id: Uuid,
    secret_name: &str,
    secret_kind: &str,
    encrypted_value: &str,
    redacted_hint: Option<&str>,
) -> Result<Option<SupplierAccountSecretRow>, sqlx::Error> {
    if find_account(db, household_id, account_id).await?.is_none() {
        return Ok(None);
    }
    let now = now_utc_rfc3339();
    sqlx::query(
        "INSERT INTO supplier_account_secret \
         (account_id, secret_name, secret_kind, encrypted_value, redacted_hint, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(account_id, secret_name) DO UPDATE SET \
             secret_kind = excluded.secret_kind, encrypted_value = excluded.encrypted_value, \
             redacted_hint = excluded.redacted_hint, updated_at = excluded.updated_at",
    )
    .bind(account_id.to_string())
    .bind(secret_name)
    .bind(secret_kind)
    .bind(encrypted_value)
    .bind(redacted_hint)
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_account_secret(db, account_id, secret_name).await
}

pub async fn list_account_secrets(
    db: &Database,
    account_id: Uuid,
) -> Result<Vec<SupplierAccountSecretRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(format!(
        "SELECT {SECRET_COLS} FROM supplier_account_secret \
         WHERE account_id = ? ORDER BY secret_name ASC"
    )))
    .bind(account_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_secret).collect()
}

pub async fn find_account_secret(
    db: &Database,
    account_id: Uuid,
    secret_name: &str,
) -> Result<Option<SupplierAccountSecretRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {SECRET_COLS} FROM supplier_account_secret WHERE account_id = ? AND secret_name = ?"
    )))
    .bind(account_id.to_string())
    .bind(secret_name)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_secret).transpose()
}

pub async fn delete_account_secret(
    db: &Database,
    household_id: Uuid,
    account_id: Uuid,
    secret_name: &str,
) -> Result<bool, sqlx::Error> {
    if find_account(db, household_id, account_id).await?.is_none() {
        return Ok(false);
    }
    let result =
        sqlx::query("DELETE FROM supplier_account_secret WHERE account_id = ? AND secret_name = ?")
            .bind(account_id.to_string())
            .bind(secret_name)
            .execute(&db.pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn upsert_catalog_item(
    db: &Database,
    item: &NewCatalogItem<'_>,
) -> Result<SupplierCatalogItemRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    let existing = sqlx::query(
        "SELECT id FROM supplier_catalog_item WHERE supplier_id = ? AND supplier_item_id = ?",
    )
    .bind(item.supplier_id)
    .bind(item.supplier_item_id)
    .fetch_optional(&db.pool)
    .await?;
    let id = existing
        .as_ref()
        .map(|row| uuid_from(row, "id"))
        .transpose()?
        .unwrap_or_else(Uuid::now_v7);
    sqlx::query(
        "INSERT INTO supplier_catalog_item \
         (id, supplier_id, supplier_item_id, name, brand, image_url, detail_url, availability, \
          price_amount, price_currency, pack_quantity, pack_unit, lead_time_min_days, \
          lead_time_max_days, minimum_order_quantity, minimum_order_unit, metadata_json, fetched_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(supplier_id, supplier_item_id) DO UPDATE SET \
             name = excluded.name, brand = excluded.brand, image_url = excluded.image_url, \
             detail_url = excluded.detail_url, availability = excluded.availability, \
             price_amount = excluded.price_amount, price_currency = excluded.price_currency, \
             pack_quantity = excluded.pack_quantity, pack_unit = excluded.pack_unit, \
             lead_time_min_days = excluded.lead_time_min_days, \
             lead_time_max_days = excluded.lead_time_max_days, \
             minimum_order_quantity = excluded.minimum_order_quantity, \
             minimum_order_unit = excluded.minimum_order_unit, \
             metadata_json = excluded.metadata_json, fetched_at = excluded.fetched_at",
    )
    .bind(id.to_string())
    .bind(item.supplier_id)
    .bind(item.supplier_item_id)
    .bind(item.name)
    .bind(item.brand)
    .bind(item.image_url)
    .bind(item.detail_url)
    .bind(item.availability)
    .bind(item.price_amount)
    .bind(item.price_currency)
    .bind(item.pack_quantity)
    .bind(item.pack_unit)
    .bind(item.lead_time_min_days)
    .bind(item.lead_time_max_days)
    .bind(item.minimum_order_quantity)
    .bind(item.minimum_order_unit)
    .bind(item.metadata_json)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_catalog_item(db, item.supplier_id, item.supplier_item_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_catalog_item(
    db: &Database,
    supplier_id: &str,
    supplier_item_id: &str,
) -> Result<Option<SupplierCatalogItemRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {CATALOG_COLS} FROM supplier_catalog_item \
         WHERE supplier_id = ? AND supplier_item_id = ?"
    )))
    .bind(supplier_id)
    .bind(supplier_item_id)
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_catalog_item).transpose()
}

pub async fn upsert_mapping(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    new: &NewMapping<'_>,
) -> Result<ProductSupplierMappingRow, sqlx::Error> {
    let now = now_utc_rfc3339();
    let existing = sqlx::query(
        "SELECT id FROM product_supplier_mapping \
         WHERE household_id = ? AND product_id = ? AND supplier_id = ? AND supplier_item_id = ?",
    )
    .bind(household_id.to_string())
    .bind(new.product_id.to_string())
    .bind(new.supplier_id)
    .bind(new.supplier_item_id)
    .fetch_optional(&db.pool)
    .await?;
    let id = existing
        .as_ref()
        .map(|row| uuid_from(row, "id"))
        .transpose()?
        .unwrap_or_else(Uuid::now_v7);
    sqlx::query(
        "INSERT INTO product_supplier_mapping \
         (id, household_id, product_id, supplier_id, supplier_item_id, confidence, confirmed_at, \
          substitute_policy_json, created_by, updated_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
             confidence = excluded.confidence, confirmed_at = excluded.confirmed_at, \
             substitute_policy_json = excluded.substitute_policy_json, \
             updated_by = excluded.updated_by, updated_at = excluded.updated_at",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.product_id.to_string())
    .bind(new.supplier_id)
    .bind(new.supplier_item_id)
    .bind(new.confidence)
    .bind(new.confirmed_at)
    .bind(new.substitute_policy_json)
    .bind(actor_id.to_string())
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&db.pool)
    .await?;
    find_mapping(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_mappings_for_product(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
) -> Result<Vec<ProductSupplierMappingRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(format!(
        "SELECT {MAPPING_COLS} FROM product_supplier_mapping \
         WHERE household_id = ? AND product_id = ? ORDER BY confirmed_at DESC, updated_at DESC"
    )))
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_mapping).collect()
}

pub async fn find_mapping(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<ProductSupplierMappingRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {MAPPING_COLS} FROM product_supplier_mapping WHERE household_id = ? AND id = ?"
    )))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_mapping).transpose()
}

pub async fn delete_mapping(
    db: &Database,
    household_id: Uuid,
    product_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM product_supplier_mapping WHERE household_id = ? AND product_id = ? AND id = ?",
    )
    .bind(household_id.to_string())
    .bind(product_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn create_cart_draft(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    new: &NewCartDraft<'_>,
) -> Result<(SupplierCartDraftRow, Vec<SupplierCartLineRow>), sqlx::Error> {
    let draft_id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;
    sqlx::query(
        "INSERT INTO supplier_cart_draft \
         (id, household_id, account_id, supplier_id, status, source, intervention_state, \
          review_notes, created_by, updated_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(draft_id.to_string())
    .bind(household_id.to_string())
    .bind(new.account_id.map(|id| id.to_string()))
    .bind(new.supplier_id)
    .bind(new.status)
    .bind(new.source)
    .bind(new.intervention_state)
    .bind(new.review_notes)
    .bind(actor_id.to_string())
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    for (idx, line) in new.lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO supplier_cart_line \
             (id, draft_id, household_id, product_id, supplier_item_id, quantity, unit, note, sort_order, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(draft_id.to_string())
        .bind(household_id.to_string())
        .bind(line.product_id.map(|id| id.to_string()))
        .bind(line.supplier_item_id)
        .bind(line.quantity)
        .bind(line.unit)
        .bind(line.note)
        .bind(i64::try_from(idx).unwrap_or(i64::MAX))
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    find_cart_draft(db, household_id, draft_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_cart_draft(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<(SupplierCartDraftRow, Vec<SupplierCartLineRow>)>, sqlx::Error> {
    let Some(draft) = sqlx::query(audited_sql(format!(
        "SELECT {DRAFT_COLS} FROM supplier_cart_draft WHERE household_id = ? AND id = ?"
    )))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?
    .map(row_to_draft)
    .transpose()?
    else {
        return Ok(None);
    };
    let lines = list_cart_lines(db, household_id, id).await?;
    Ok(Some((draft, lines)))
}

pub async fn update_cart_status(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
    actor_id: Uuid,
    status: &str,
    intervention_state: &str,
    review_notes: Option<&str>,
) -> Result<Option<(SupplierCartDraftRow, Vec<SupplierCartLineRow>)>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let updated = sqlx::query(
        "UPDATE supplier_cart_draft \
         SET status = ?, intervention_state = ?, review_notes = ?, updated_by = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(status)
    .bind(intervention_state)
    .bind(review_notes)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(household_id.to_string())
    .bind(id.to_string())
    .execute(&db.pool)
    .await?;
    if updated.rows_affected() == 0 {
        Ok(None)
    } else {
        find_cart_draft(db, household_id, id).await
    }
}

pub async fn create_order(
    db: &Database,
    household_id: Uuid,
    actor_id: Uuid,
    new: &NewOrder<'_>,
) -> Result<SupplierOrderRow, sqlx::Error> {
    let id = Uuid::now_v7();
    let now = now_utc_rfc3339();
    let mut tx = db.pool.begin().await?;
    sqlx::query(
        "INSERT INTO supplier_order \
         (id, household_id, draft_id, account_id, supplier_id, supplier_order_id, status, \
          review_url, redacted_summary_json, submitted_at, delivered_at, created_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind(new.draft_id.map(|id| id.to_string()))
    .bind(new.account_id.map(|id| id.to_string()))
    .bind(new.supplier_id)
    .bind(new.supplier_order_id)
    .bind(new.status)
    .bind(new.review_url)
    .bind(new.redacted_summary_json)
    .bind(new.submitted_at)
    .bind(actor_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO supplier_order_event \
         (id, order_id, household_id, event_type, status, redacted_payload_json, created_at, created_by) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(id.to_string())
    .bind(household_id.to_string())
    .bind("created")
    .bind(new.status)
    .bind(new.redacted_summary_json)
    .bind(&now)
    .bind(actor_id.to_string())
    .execute(&mut *tx)
    .await?;
    if let Some(draft_id) = new.draft_id {
        sqlx::query(
            "UPDATE supplier_cart_draft SET status = ?, updated_by = ?, updated_at = ? \
             WHERE household_id = ? AND id = ?",
        )
        .bind(CART_STATUS_SUBMITTED)
        .bind(actor_id.to_string())
        .bind(&now)
        .bind(household_id.to_string())
        .bind(draft_id.to_string())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    find_order(db, household_id, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_orders(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<SupplierOrderRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(format!(
        "SELECT {ORDER_COLS} FROM supplier_order WHERE household_id = ? ORDER BY created_at DESC"
    )))
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_order).collect()
}

pub async fn find_order(
    db: &Database,
    household_id: Uuid,
    id: Uuid,
) -> Result<Option<SupplierOrderRow>, sqlx::Error> {
    let row = sqlx::query(audited_sql(format!(
        "SELECT {ORDER_COLS} FROM supplier_order WHERE household_id = ? AND id = ?"
    )))
    .bind(household_id.to_string())
    .bind(id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(row_to_order).transpose()
}

pub async fn mark_order_delivered(
    db: &Database,
    household_id: Uuid,
    order_id: Uuid,
    actor_id: Uuid,
) -> Result<Option<SupplierOrderRow>, sqlx::Error> {
    let now = now_utc_rfc3339();
    let updated = sqlx::query(
        "UPDATE supplier_order SET status = ?, delivered_at = ?, updated_at = ? \
         WHERE household_id = ? AND id = ?",
    )
    .bind(ORDER_STATUS_DELIVERED)
    .bind(&now)
    .bind(&now)
    .bind(household_id.to_string())
    .bind(order_id.to_string())
    .execute(&db.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(None);
    }
    sqlx::query(
        "INSERT INTO supplier_order_event \
         (id, order_id, household_id, event_type, status, redacted_payload_json, created_at, created_by) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(order_id.to_string())
    .bind(household_id.to_string())
    .bind("received")
    .bind(ORDER_STATUS_DELIVERED)
    .bind("{}")
    .bind(&now)
    .bind(actor_id.to_string())
    .execute(&db.pool)
    .await?;
    find_order(db, household_id, order_id).await
}

async fn list_cart_lines(
    db: &Database,
    household_id: Uuid,
    draft_id: Uuid,
) -> Result<Vec<SupplierCartLineRow>, sqlx::Error> {
    let rows = sqlx::query(audited_sql(format!(
        "SELECT {LINE_COLS} FROM supplier_cart_line \
         WHERE household_id = ? AND draft_id = ? ORDER BY sort_order ASC"
    )))
    .bind(household_id.to_string())
    .bind(draft_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter().map(row_to_line).collect()
}

fn row_to_supplier(row: sqlx::any::AnyRow) -> Result<SupplierRow, sqlx::Error> {
    Ok(SupplierRow {
        id: row.try_get("id")?,
        display_name: row.try_get("display_name")?,
        capabilities_json: row.try_get("capabilities_json")?,
        requirements_json: row.try_get("requirements_json")?,
        supported_regions_json: row.try_get("supported_regions_json")?,
        terms_url: row.try_get("terms_url")?,
        needs_network: row_bool(&row, "needs_network")?,
        needs_browser: row_bool(&row, "needs_browser")?,
        enabled: row_bool(&row, "enabled")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_account(row: sqlx::any::AnyRow) -> Result<SupplierAccountRow, sqlx::Error> {
    Ok(SupplierAccountRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        supplier_id: row.try_get("supplier_id")?,
        display_name: row.try_get("display_name")?,
        status: row.try_get("status")?,
        region_json: row.try_get("region_json")?,
        config_json: row.try_get("config_json")?,
        consent_accepted_at: row.try_get("consent_accepted_at")?,
        created_by: optional_uuid_from(&row, "created_by")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_secret(row: sqlx::any::AnyRow) -> Result<SupplierAccountSecretRow, sqlx::Error> {
    Ok(SupplierAccountSecretRow {
        account_id: uuid_from(&row, "account_id")?,
        secret_name: row.try_get("secret_name")?,
        secret_kind: row.try_get("secret_kind")?,
        redacted_hint: row.try_get("redacted_hint")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_catalog_item(row: sqlx::any::AnyRow) -> Result<SupplierCatalogItemRow, sqlx::Error> {
    Ok(SupplierCatalogItemRow {
        id: uuid_from(&row, "id")?,
        supplier_id: row.try_get("supplier_id")?,
        supplier_item_id: row.try_get("supplier_item_id")?,
        name: row.try_get("name")?,
        brand: row.try_get("brand")?,
        image_url: row.try_get("image_url")?,
        detail_url: row.try_get("detail_url")?,
        availability: row.try_get("availability")?,
        price_amount: row.try_get("price_amount")?,
        price_currency: row.try_get("price_currency")?,
        pack_quantity: row.try_get("pack_quantity")?,
        pack_unit: row.try_get("pack_unit")?,
        lead_time_min_days: row.try_get("lead_time_min_days")?,
        lead_time_max_days: row.try_get("lead_time_max_days")?,
        minimum_order_quantity: row.try_get("minimum_order_quantity")?,
        minimum_order_unit: row.try_get("minimum_order_unit")?,
        metadata_json: row.try_get("metadata_json")?,
        fetched_at: row.try_get("fetched_at")?,
    })
}

fn row_to_mapping(row: sqlx::any::AnyRow) -> Result<ProductSupplierMappingRow, sqlx::Error> {
    Ok(ProductSupplierMappingRow {
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

fn row_to_draft(row: sqlx::any::AnyRow) -> Result<SupplierCartDraftRow, sqlx::Error> {
    Ok(SupplierCartDraftRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        account_id: optional_uuid_from(&row, "account_id")?,
        supplier_id: row.try_get("supplier_id")?,
        status: row.try_get("status")?,
        source: row.try_get("source")?,
        intervention_state: row.try_get("intervention_state")?,
        review_notes: row.try_get("review_notes")?,
        created_by: optional_uuid_from(&row, "created_by")?,
        updated_by: optional_uuid_from(&row, "updated_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_line(row: sqlx::any::AnyRow) -> Result<SupplierCartLineRow, sqlx::Error> {
    Ok(SupplierCartLineRow {
        id: uuid_from(&row, "id")?,
        draft_id: uuid_from(&row, "draft_id")?,
        household_id: uuid_from(&row, "household_id")?,
        product_id: optional_uuid_from(&row, "product_id")?,
        supplier_item_id: row.try_get("supplier_item_id")?,
        quantity: row.try_get("quantity")?,
        unit: row.try_get("unit")?,
        note: row.try_get("note")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_order(row: sqlx::any::AnyRow) -> Result<SupplierOrderRow, sqlx::Error> {
    Ok(SupplierOrderRow {
        id: uuid_from(&row, "id")?,
        household_id: uuid_from(&row, "household_id")?,
        draft_id: optional_uuid_from(&row, "draft_id")?,
        account_id: optional_uuid_from(&row, "account_id")?,
        supplier_id: row.try_get("supplier_id")?,
        supplier_order_id: row.try_get("supplier_order_id")?,
        status: row.try_get("status")?,
        review_url: row.try_get("review_url")?,
        redacted_summary_json: row.try_get("redacted_summary_json")?,
        submitted_at: row.try_get("submitted_at")?,
        delivered_at: row.try_get("delivered_at")?,
        created_by: optional_uuid_from(&row, "created_by")?,
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
    use crate::{households, products, test_support, users};

    #[tokio::test]
    async fn supplier_account_secrets_do_not_expose_ciphertext() {
        let db = test_support::sqlite().await.into_db();
        let user = users::create(&db, "supplier@example.com", "Supplier", "hash")
            .await
            .unwrap();
        let household = households::create(&db, "Kitchen", "UTC").await.unwrap();
        upsert_supplier(
            &db,
            &NewSupplier {
                id: SUPPLIER_MOCK,
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
        let account = create_account(
            &db,
            household.id,
            user.id,
            &NewSupplierAccount {
                supplier_id: SUPPLIER_MOCK,
                display_name: "Mock",
                status: ACCOUNT_STATUS_ACTIVE,
                region_json: None,
                config_json: "{}",
                consent_accepted_at: None,
            },
        )
        .await
        .unwrap();

        let secret = upsert_account_secret(
            &db,
            household.id,
            account.id,
            "api_token",
            "password",
            "encrypted-value",
            Some("tok...123"),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(secret.secret_name, "api_token");
        assert_eq!(secret.redacted_hint.as_deref(), Some("tok...123"));
    }

    #[tokio::test]
    async fn mappings_are_household_and_product_scoped() {
        let db = test_support::sqlite().await.into_db();
        let user = users::create(&db, "mapping@example.com", "Mapping", "hash")
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
        upsert_supplier(
            &db,
            &NewSupplier {
                id: SUPPLIER_MOCK,
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

        let mapping = upsert_mapping(
            &db,
            household.id,
            user.id,
            &NewMapping {
                product_id: product.id,
                supplier_id: SUPPLIER_MOCK,
                supplier_item_id: "mock-rice-1kg",
                confidence: "confirmed",
                confirmed_at: Some("2026-05-26T00:00:00Z"),
                substitute_policy_json: "{}",
            },
        )
        .await
        .unwrap();

        let mappings = list_mappings_for_product(&db, household.id, product.id)
            .await
            .unwrap();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].id, mapping.id);
    }
}
