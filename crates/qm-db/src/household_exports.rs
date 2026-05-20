use std::collections::{HashMap, HashSet};

use qm_core::units::{MeasurementSystem, UnitFamily};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{jobs, now_utc_rfc3339, Database};

pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HouseholdExportDocument {
    pub schema_version: i64,
    pub exported_at: String,
    pub household: ExportHousehold,
    pub locations: Vec<ExportLocation>,
    pub storage_vessels: Vec<ExportStorageVessel>,
    pub label_printers: Vec<ExportLabelPrinter>,
    pub products: Vec<ExportProduct>,
    pub barcode_cache: Vec<ExportBarcodeCacheEntry>,
    pub stock_batches: Vec<ExportStockBatch>,
    pub stock_events: Vec<ExportStockEvent>,
    pub stock_reminders: Vec<ExportStockReminder>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportHousehold {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub measurement_system: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportLocation {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub sort_order: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportStorageVessel {
    pub id: Uuid,
    pub name: String,
    pub tare_weight: String,
    pub tare_unit: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportLabelPrinter {
    pub id: Uuid,
    pub name: String,
    pub driver: String,
    pub address: String,
    pub port: i64,
    pub media: String,
    #[serde(default = "default_label_printer_delivery")]
    pub delivery: String,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportProduct {
    pub id: Uuid,
    pub source: String,
    pub off_barcode: Option<String>,
    pub name: String,
    pub brand: Option<String>,
    pub family: String,
    pub default_unit: String,
    pub image_url: Option<String>,
    pub package_quantity: Option<String>,
    pub package_unit: Option<String>,
    pub fetched_at: Option<String>,
    pub created_at: String,
    pub deleted_at: Option<String>,
    pub max_open_days: Option<i64>,
    pub package_size_local_override: bool,
    pub off_name: Option<String>,
    pub off_brand: Option<String>,
    pub off_package_quantity: Option<String>,
    pub off_package_unit: Option<String>,
    pub name_local_override: bool,
    pub brand_local_override: bool,
    pub family_local_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportBarcodeCacheEntry {
    pub barcode: String,
    pub product_id: Option<Uuid>,
    pub raw_off_json: Option<String>,
    pub fetched_at: String,
    pub miss: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportStockBatch {
    pub id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub storage_vessel_id: Option<Uuid>,
    pub source_batch_id: Option<Uuid>,
    pub source_operation_id: Option<Uuid>,
    pub initial_quantity: String,
    pub quantity: String,
    pub unit: String,
    pub package_quantity: Option<String>,
    pub package_unit: Option<String>,
    pub produced_on: Option<String>,
    pub expires_on: Option<String>,
    pub opened_on: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub depleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportStockEvent {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub event_type: String,
    pub quantity_delta: String,
    pub package_quantity: Option<String>,
    pub package_unit: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub consume_request_id: Option<Uuid>,
    pub operation_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportStockReminder {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub kind: String,
    pub fire_at: String,
    pub title: String,
    pub body: String,
    pub household_timezone: String,
    pub household_fire_local_at: String,
    pub expires_on: Option<String>,
    pub acked_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ImportOutcome {
    pub household_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct DeleteRequestOutcome {
    pub purge_job_id: Uuid,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported household export schema_version: {0}")]
    UnsupportedSchemaVersion(i64),
    #[error("duplicate id in {0}")]
    DuplicateId(&'static str),
    #[error("dangling reference: {0}")]
    DanglingReference(String),
    #[error("invalid value: {0}")]
    InvalidValue(String),
    #[error("database error")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
pub enum DeleteRequestError {
    #[error("household not found")]
    NotFound,
    #[error("confirmation name did not match")]
    ConfirmationMismatch,
    #[error("database error")]
    Database(#[from] sqlx::Error),
}

pub async fn export_household(
    db: &Database,
    household_id: Uuid,
) -> Result<Option<HouseholdExportDocument>, sqlx::Error> {
    let Some(household) = export_household_row(db, household_id).await? else {
        return Ok(None);
    };

    Ok(Some(HouseholdExportDocument {
        schema_version: SCHEMA_VERSION,
        exported_at: now_utc_rfc3339(),
        household,
        locations: export_locations(db, household_id).await?,
        storage_vessels: export_storage_vessels(db, household_id).await?,
        label_printers: export_label_printers(db, household_id).await?,
        products: export_products(db, household_id).await?,
        barcode_cache: export_barcode_cache(db, household_id).await?,
        stock_batches: export_stock_batches(db, household_id).await?,
        stock_events: export_stock_events(db, household_id).await?,
        stock_reminders: export_stock_reminders(db, household_id).await?,
    }))
}

pub async fn import_household(
    db: &Database,
    document: &HouseholdExportDocument,
    actor_user_id: Uuid,
) -> Result<ImportOutcome, ImportError> {
    validate_document(document)?;

    let mut location_ids = IdMap::new();
    let mut vessel_ids = IdMap::new();
    let mut printer_ids = IdMap::new();
    let mut product_ids = IdMap::new();
    let mut batch_ids = IdMap::new();
    let mut event_ids = IdMap::new();
    let mut reminder_ids = IdMap::new();
    let mut consume_request_ids = HashMap::new();
    let mut operation_ids = HashMap::new();

    for row in &document.locations {
        location_ids.insert(row.id, Uuid::now_v7());
    }
    for row in &document.storage_vessels {
        vessel_ids.insert(row.id, Uuid::now_v7());
    }
    for row in &document.label_printers {
        printer_ids.insert(row.id, Uuid::now_v7());
    }
    for row in &document.products {
        product_ids.insert(row.id, Uuid::now_v7());
    }
    for row in &document.stock_batches {
        batch_ids.insert(row.id, Uuid::now_v7());
    }
    for row in &document.stock_events {
        event_ids.insert(row.id, Uuid::now_v7());
        if let Some(id) = row.consume_request_id {
            consume_request_ids.entry(id).or_insert_with(Uuid::now_v7);
        }
        if let Some(id) = row.operation_id {
            operation_ids.entry(id).or_insert_with(Uuid::now_v7);
        }
    }
    for row in &document.stock_batches {
        if let Some(id) = row.source_operation_id {
            operation_ids.entry(id).or_insert_with(Uuid::now_v7);
        }
    }
    for row in &document.stock_reminders {
        reminder_ids.insert(row.id, Uuid::now_v7());
    }

    let household_id = Uuid::now_v7();
    let mut tx = db.pool.begin().await?;
    sqlx::query(
        "INSERT INTO household (id, name, timezone, created_at, measurement_system) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(household_id.to_string())
    .bind(&document.household.name)
    .bind(&document.household.timezone)
    .bind(&document.household.created_at)
    .bind(&document.household.measurement_system)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO membership (household_id, user_id, role, joined_at) VALUES (?, ?, ?, ?)",
    )
    .bind(household_id.to_string())
    .bind(actor_user_id.to_string())
    .bind("admin")
    .bind(now_utc_rfc3339())
    .execute(&mut *tx)
    .await?;

    for row in &document.locations {
        sqlx::query(
            "INSERT INTO location (id, household_id, name, kind, sort_order, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(location_ids.get(row.id).to_string())
        .bind(household_id.to_string())
        .bind(&row.name)
        .bind(&row.kind)
        .bind(row.sort_order)
        .bind(&row.created_at)
        .execute(&mut *tx)
        .await?;
    }

    for row in &document.storage_vessels {
        sqlx::query(
            "INSERT INTO storage_vessel \
             (id, household_id, name, tare_weight, tare_unit, sort_order, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(vessel_ids.get(row.id).to_string())
        .bind(household_id.to_string())
        .bind(&row.name)
        .bind(&row.tare_weight)
        .bind(&row.tare_unit)
        .bind(row.sort_order)
        .bind(&row.created_at)
        .bind(&row.updated_at)
        .execute(&mut *tx)
        .await?;
    }

    for row in &document.label_printers {
        sqlx::query(
            "INSERT INTO label_printer \
             (id, household_id, name, driver, address, port, media, delivery, enabled, is_default, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(printer_ids.get(row.id).to_string())
        .bind(household_id.to_string())
        .bind(&row.name)
        .bind(&row.driver)
        .bind(&row.address)
        .bind(row.port)
        .bind(&row.media)
        .bind(&row.delivery)
        .bind(bool_int(row.enabled))
        .bind(bool_int(row.is_default))
        .bind(&row.created_at)
        .bind(&row.updated_at)
        .execute(&mut *tx)
        .await?;
    }

    for row in &document.products {
        sqlx::query(
            "INSERT INTO product \
             (id, source, off_barcode, name, brand, family, default_unit, image_url, \
              package_quantity, package_unit, fetched_at, created_by_household_id, created_at, \
              deleted_at, max_open_days, package_size_local_override, off_name, off_brand, \
              off_package_quantity, off_package_unit, name_local_override, brand_local_override, \
              family_local_override) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(product_ids.get(row.id).to_string())
        .bind(&row.source)
        .bind(&row.off_barcode)
        .bind(&row.name)
        .bind(&row.brand)
        .bind(&row.family)
        .bind(&row.default_unit)
        .bind(&row.image_url)
        .bind(&row.package_quantity)
        .bind(&row.package_unit)
        .bind(&row.fetched_at)
        .bind(household_id.to_string())
        .bind(&row.created_at)
        .bind(&row.deleted_at)
        .bind(row.max_open_days)
        .bind(bool_int(row.package_size_local_override))
        .bind(&row.off_name)
        .bind(&row.off_brand)
        .bind(&row.off_package_quantity)
        .bind(&row.off_package_unit)
        .bind(bool_int(row.name_local_override))
        .bind(bool_int(row.brand_local_override))
        .bind(bool_int(row.family_local_override))
        .execute(&mut *tx)
        .await?;
    }

    for row in &document.stock_batches {
        sqlx::query(
            "INSERT INTO stock_batch \
             (id, household_id, product_id, location_id, storage_vessel_id, source_batch_id, \
              source_operation_id, initial_quantity, quantity, unit, package_quantity, package_unit, \
              produced_on, expires_on, opened_on, note, created_at, created_by, depleted_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(batch_ids.get(row.id).to_string())
        .bind(household_id.to_string())
        .bind(product_ids.get(row.product_id).to_string())
        .bind(location_ids.get(row.location_id).to_string())
        .bind(
            row.storage_vessel_id
                .map(|id| vessel_ids.get(id).to_string()),
        )
        .bind(row.source_batch_id.map(|id| batch_ids.get(id).to_string()))
        .bind(
            row.source_operation_id
                .map(|id| operation_ids[&id].to_string()),
        )
        .bind(&row.initial_quantity)
        .bind(&row.quantity)
        .bind(&row.unit)
        .bind(&row.package_quantity)
        .bind(&row.package_unit)
        .bind(&row.produced_on)
        .bind(&row.expires_on)
        .bind(&row.opened_on)
        .bind(&row.note)
        .bind(&row.created_at)
        .bind(actor_user_id.to_string())
        .bind(&row.depleted_at)
        .execute(&mut *tx)
        .await?;
    }

    for row in &document.stock_events {
        sqlx::query(
            "INSERT INTO stock_event \
             (id, household_id, batch_id, event_type, quantity_delta, package_quantity, \
              package_unit, note, created_at, created_by, consume_request_id, operation_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event_ids.get(row.id).to_string())
        .bind(household_id.to_string())
        .bind(batch_ids.get(row.batch_id).to_string())
        .bind(&row.event_type)
        .bind(&row.quantity_delta)
        .bind(&row.package_quantity)
        .bind(&row.package_unit)
        .bind(&row.note)
        .bind(&row.created_at)
        .bind(actor_user_id.to_string())
        .bind(
            row.consume_request_id
                .map(|id| consume_request_ids[&id].to_string()),
        )
        .bind(row.operation_id.map(|id| operation_ids[&id].to_string()))
        .execute(&mut *tx)
        .await?;
    }

    for row in &document.barcode_cache {
        sqlx::query(
            "INSERT INTO barcode_cache (household_id, barcode, product_id, raw_off_json, fetched_at, miss) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(household_id.to_string())
        .bind(&row.barcode)
        .bind(row.product_id.map(|id| product_ids.get(id).to_string()))
        .bind(&row.raw_off_json)
        .bind(&row.fetched_at)
        .bind(bool_int(row.miss))
        .execute(&mut *tx)
        .await?;
    }

    for row in &document.stock_reminders {
        sqlx::query(
            "INSERT INTO stock_reminder \
             (id, household_id, batch_id, product_id, location_id, kind, fire_at, title, body, \
              created_at, household_timezone, expires_on, household_fire_local_at, acked_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(reminder_ids.get(row.id).to_string())
        .bind(household_id.to_string())
        .bind(batch_ids.get(row.batch_id).to_string())
        .bind(product_ids.get(row.product_id).to_string())
        .bind(location_ids.get(row.location_id).to_string())
        .bind(&row.kind)
        .bind(&row.fire_at)
        .bind(&row.title)
        .bind(&row.body)
        .bind(&row.created_at)
        .bind(&row.household_timezone)
        .bind(&row.expires_on)
        .bind(&row.household_fire_local_at)
        .bind(&row.acked_at)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(ImportOutcome { household_id })
}

pub async fn request_household_deletion(
    db: &Database,
    household_id: Uuid,
    actor_user_id: Uuid,
    confirmation_name: &str,
) -> Result<DeleteRequestOutcome, DeleteRequestError> {
    let mut tx = db.pool.begin().await?;
    let row =
        sqlx::query("SELECT name FROM household WHERE id = ? AND deletion_requested_at IS NULL")
            .bind(household_id.to_string())
            .fetch_optional(&mut *tx)
            .await?;
    let Some(row) = row else {
        return Err(DeleteRequestError::NotFound);
    };
    let name: String = row.try_get("name")?;
    if confirmation_name.trim() != name {
        return Err(DeleteRequestError::ConfirmationMismatch);
    }

    let now = now_utc_rfc3339();
    sqlx::query(
        "UPDATE household SET deletion_requested_at = ?, deletion_requested_by = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(actor_user_id.to_string())
    .bind(household_id.to_string())
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM invite WHERE household_id = ?")
        .bind(household_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM membership WHERE household_id = ?")
        .bind(household_id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE auth_session SET active_household_id = NULL, updated_at = ? WHERE active_household_id = ?")
        .bind(&now)
        .bind(household_id.to_string())
        .execute(&mut *tx)
        .await?;

    let payload_json = format!(r#"{{"household_id":"{household_id}"}}"#);
    let purge_job_id = enqueue_purge_job_tx(&mut tx, household_id, &payload_json, &now).await?;
    tx.commit().await?;
    Ok(DeleteRequestOutcome { purge_job_id })
}

pub async fn purge_household(db: &Database, household_id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = db.pool.begin().await?;
    purge_household_tx(&mut tx, household_id).await?;
    tx.commit().await
}

async fn purge_household_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
) -> Result<(), sqlx::Error> {
    let household = household_id.to_string();
    sqlx::query(
        "DELETE FROM reminder_device_state \
         WHERE reminder_id IN (SELECT id FROM stock_reminder WHERE household_id = ?)",
    )
    .bind(&household)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM reminder_delivery \
         WHERE reminder_id IN (SELECT id FROM stock_reminder WHERE household_id = ?)",
    )
    .bind(&household)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM stock_reminder WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM stock_event WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM stock_batch WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM barcode_cache WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM storage_vessel WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM label_printer WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM location WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM invite WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM product WHERE created_by_household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM membership WHERE household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE auth_session SET active_household_id = NULL WHERE active_household_id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM household WHERE id = ?")
        .bind(&household)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn enqueue_purge_job_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    household_id: Uuid,
    payload_json: &str,
    now: &str,
) -> Result<Uuid, sqlx::Error> {
    let existing = sqlx::query(
        "SELECT id FROM background_job \
         WHERE kind = ? AND dedupe_key = ? AND status IN (?, ?, ?) \
         LIMIT 1",
    )
    .bind(jobs::KIND_HOUSEHOLD_PURGE)
    .bind(household_id.to_string())
    .bind(jobs::STATUS_PENDING)
    .bind(jobs::STATUS_LEASED)
    .bind(jobs::STATUS_RETRYABLE)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = existing {
        return uuid_from(&row, "id");
    }

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO background_job \
         (id, kind, dedupe_key, payload_json, status, run_at, lease_owner, lease_until, \
          attempt_count, max_attempts, last_error, created_at, updated_at, finished_at) \
         VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, 0, ?, NULL, ?, ?, NULL)",
    )
    .bind(id.to_string())
    .bind(jobs::KIND_HOUSEHOLD_PURGE)
    .bind(household_id.to_string())
    .bind(payload_json)
    .bind(jobs::STATUS_PENDING)
    .bind(now)
    .bind(5_i64)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

fn validate_document(document: &HouseholdExportDocument) -> Result<(), ImportError> {
    if document.schema_version != SCHEMA_VERSION {
        return Err(ImportError::UnsupportedSchemaVersion(
            document.schema_version,
        ));
    }
    if MeasurementSystem::from_str_ci(&document.household.measurement_system).is_none() {
        return Err(ImportError::InvalidValue(
            "household.measurement_system is invalid".into(),
        ));
    }

    let location_ids = unique_ids("locations", document.locations.iter().map(|row| row.id))?;
    let vessel_ids = unique_ids(
        "storage_vessels",
        document.storage_vessels.iter().map(|row| row.id),
    )?;
    let product_ids = unique_ids("products", document.products.iter().map(|row| row.id))?;
    let batch_ids = unique_ids(
        "stock_batches",
        document.stock_batches.iter().map(|row| row.id),
    )?;
    unique_ids(
        "label_printers",
        document.label_printers.iter().map(|row| row.id),
    )?;
    unique_ids(
        "stock_events",
        document.stock_events.iter().map(|row| row.id),
    )?;
    unique_ids(
        "stock_reminders",
        document.stock_reminders.iter().map(|row| row.id),
    )?;

    for product in &document.products {
        let family = UnitFamily::from_str_ci(&product.family).ok_or_else(|| {
            ImportError::InvalidValue(format!("invalid product family {}", product.family))
        })?;
        let unit = qm_core::units::lookup(&product.default_unit).map_err(|_| {
            ImportError::InvalidValue(format!("unknown product unit {}", product.default_unit))
        })?;
        if unit.family != family {
            return Err(ImportError::InvalidValue(format!(
                "product {} default_unit does not match family",
                product.id
            )));
        }
    }

    for batch in &document.stock_batches {
        require_ref(&product_ids, batch.product_id, "batch.product_id")?;
        require_ref(&location_ids, batch.location_id, "batch.location_id")?;
        if let Some(id) = batch.storage_vessel_id {
            require_ref(&vessel_ids, id, "batch.storage_vessel_id")?;
        }
        if let Some(id) = batch.source_batch_id {
            require_ref(&batch_ids, id, "batch.source_batch_id")?;
        }
        let product = document
            .products
            .iter()
            .find(|row| row.id == batch.product_id)
            .expect("validated product reference");
        let family = UnitFamily::from_str_ci(&product.family).expect("validated product family");
        let unit = qm_core::units::lookup(&batch.unit)
            .map_err(|_| ImportError::InvalidValue(format!("unknown batch unit {}", batch.unit)))?;
        if unit.family != family {
            return Err(ImportError::InvalidValue(format!(
                "batch {} unit does not match product family",
                batch.id
            )));
        }
    }

    let mut event_sums: HashMap<Uuid, Decimal> = HashMap::new();
    for event in &document.stock_events {
        require_ref(&batch_ids, event.batch_id, "event.batch_id")?;
        let delta = event.quantity_delta.parse::<Decimal>().map_err(|_| {
            ImportError::InvalidValue(format!("event {} quantity_delta is invalid", event.id))
        })?;
        *event_sums.entry(event.batch_id).or_default() += delta;
    }
    for batch in &document.stock_batches {
        let cached = batch.quantity.parse::<Decimal>().map_err(|_| {
            ImportError::InvalidValue(format!("batch {} quantity is invalid", batch.id))
        })?;
        let sum = event_sums.get(&batch.id).copied().unwrap_or_default();
        if cached != sum {
            return Err(ImportError::InvalidValue(format!(
                "batch {} quantity does not match stock_event sum",
                batch.id
            )));
        }
    }

    for cache in &document.barcode_cache {
        if let Some(id) = cache.product_id {
            require_ref(&product_ids, id, "barcode_cache.product_id")?;
        }
    }
    for reminder in &document.stock_reminders {
        require_ref(&batch_ids, reminder.batch_id, "reminder.batch_id")?;
        require_ref(&product_ids, reminder.product_id, "reminder.product_id")?;
        require_ref(&location_ids, reminder.location_id, "reminder.location_id")?;
    }

    Ok(())
}

fn unique_ids<I>(label: &'static str, ids: I) -> Result<HashSet<Uuid>, ImportError>
where
    I: IntoIterator<Item = Uuid>,
{
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(ImportError::DuplicateId(label));
        }
    }
    Ok(seen)
}

fn require_ref(ids: &HashSet<Uuid>, id: Uuid, label: &str) -> Result<(), ImportError> {
    if ids.contains(&id) {
        Ok(())
    } else {
        Err(ImportError::DanglingReference(format!("{label} {id}")))
    }
}

fn default_label_printer_delivery() -> String {
    "server".to_owned()
}

struct IdMap(HashMap<Uuid, Uuid>);

impl IdMap {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn insert(&mut self, old: Uuid, new: Uuid) {
        self.0.insert(old, new);
    }

    fn get(&self, old: Uuid) -> Uuid {
        self.0[&old]
    }
}

async fn export_household_row(
    db: &Database,
    household_id: Uuid,
) -> Result<Option<ExportHousehold>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, name, timezone, measurement_system, created_at \
         FROM household WHERE id = ? AND deletion_requested_at IS NULL",
    )
    .bind(household_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    row.map(|row| {
        Ok(ExportHousehold {
            id: uuid_from(&row, "id")?,
            name: row.try_get("name")?,
            timezone: row.try_get("timezone")?,
            measurement_system: row.try_get("measurement_system")?,
            created_at: row.try_get("created_at")?,
        })
    })
    .transpose()
}

async fn export_locations(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportLocation>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, kind, sort_order, created_at \
         FROM location WHERE household_id = ? ORDER BY sort_order ASC, name ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportLocation {
                id: uuid_from(&row, "id")?,
                name: row.try_get("name")?,
                kind: row.try_get("kind")?,
                sort_order: row.try_get("sort_order")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

async fn export_storage_vessels(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportStorageVessel>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, tare_weight, tare_unit, sort_order, created_at, updated_at \
         FROM storage_vessel WHERE household_id = ? ORDER BY sort_order ASC, name ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportStorageVessel {
                id: uuid_from(&row, "id")?,
                name: row.try_get("name")?,
                tare_weight: row.try_get("tare_weight")?,
                tare_unit: row.try_get("tare_unit")?,
                sort_order: row.try_get("sort_order")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

async fn export_label_printers(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportLabelPrinter>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, driver, address, port, media, delivery, enabled, is_default, created_at, updated_at \
         FROM label_printer WHERE household_id = ? ORDER BY is_default DESC, name ASC, created_at ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportLabelPrinter {
                id: uuid_from(&row, "id")?,
                name: row.try_get("name")?,
                driver: row.try_get("driver")?,
                address: row.try_get("address")?,
                port: row.try_get("port")?,
                media: row.try_get("media")?,
                delivery: row.try_get("delivery")?,
                enabled: row.try_get::<i64, _>("enabled")? != 0,
                is_default: row.try_get::<i64, _>("is_default")? != 0,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

async fn export_products(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportProduct>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, source, off_barcode, name, brand, family, default_unit, image_url, \
                package_quantity, package_unit, fetched_at, created_at, deleted_at, max_open_days, \
                package_size_local_override, off_name, off_brand, off_package_quantity, \
                off_package_unit, name_local_override, brand_local_override, family_local_override \
         FROM product WHERE created_by_household_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportProduct {
                id: uuid_from(&row, "id")?,
                source: row.try_get("source")?,
                off_barcode: row.try_get("off_barcode")?,
                name: row.try_get("name")?,
                brand: row.try_get("brand")?,
                family: row.try_get("family")?,
                default_unit: row.try_get("default_unit")?,
                image_url: row.try_get("image_url")?,
                package_quantity: row.try_get("package_quantity")?,
                package_unit: row.try_get("package_unit")?,
                fetched_at: row.try_get("fetched_at")?,
                created_at: row.try_get("created_at")?,
                deleted_at: row.try_get("deleted_at")?,
                max_open_days: row.try_get("max_open_days")?,
                package_size_local_override: row
                    .try_get::<i64, _>("package_size_local_override")?
                    != 0,
                off_name: row.try_get("off_name")?,
                off_brand: row.try_get("off_brand")?,
                off_package_quantity: row.try_get("off_package_quantity")?,
                off_package_unit: row.try_get("off_package_unit")?,
                name_local_override: row.try_get::<i64, _>("name_local_override")? != 0,
                brand_local_override: row.try_get::<i64, _>("brand_local_override")? != 0,
                family_local_override: row.try_get::<i64, _>("family_local_override")? != 0,
            })
        })
        .collect()
}

async fn export_barcode_cache(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportBarcodeCacheEntry>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT barcode, product_id, raw_off_json, fetched_at, miss \
         FROM barcode_cache WHERE household_id = ? ORDER BY barcode ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportBarcodeCacheEntry {
                barcode: row.try_get("barcode")?,
                product_id: optional_uuid_from(&row, "product_id")?,
                raw_off_json: row.try_get("raw_off_json")?,
                fetched_at: row.try_get("fetched_at")?,
                miss: row.try_get::<i64, _>("miss")? != 0,
            })
        })
        .collect()
}

async fn export_stock_batches(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportStockBatch>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, product_id, location_id, storage_vessel_id, source_batch_id, \
                source_operation_id, initial_quantity, quantity, unit, \
                package_quantity, package_unit, produced_on, expires_on, opened_on, note, \
                created_at, depleted_at \
         FROM stock_batch WHERE household_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportStockBatch {
                id: uuid_from(&row, "id")?,
                product_id: uuid_from(&row, "product_id")?,
                location_id: uuid_from(&row, "location_id")?,
                storage_vessel_id: optional_uuid_from(&row, "storage_vessel_id")?,
                source_batch_id: optional_uuid_from(&row, "source_batch_id")?,
                source_operation_id: optional_uuid_from(&row, "source_operation_id")?,
                initial_quantity: row.try_get("initial_quantity")?,
                quantity: row.try_get("quantity")?,
                unit: row.try_get("unit")?,
                package_quantity: row.try_get("package_quantity")?,
                package_unit: row.try_get("package_unit")?,
                produced_on: row.try_get("produced_on")?,
                expires_on: row.try_get("expires_on")?,
                opened_on: row.try_get("opened_on")?,
                note: row.try_get("note")?,
                created_at: row.try_get("created_at")?,
                depleted_at: row.try_get("depleted_at")?,
            })
        })
        .collect()
}

async fn export_stock_events(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportStockEvent>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, batch_id, event_type, quantity_delta, package_quantity, package_unit, \
                note, created_at, consume_request_id, operation_id \
         FROM stock_event WHERE household_id = ? ORDER BY created_at ASC, id ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportStockEvent {
                id: uuid_from(&row, "id")?,
                batch_id: uuid_from(&row, "batch_id")?,
                event_type: row.try_get("event_type")?,
                quantity_delta: row.try_get("quantity_delta")?,
                package_quantity: row.try_get("package_quantity")?,
                package_unit: row.try_get("package_unit")?,
                note: row.try_get("note")?,
                created_at: row.try_get("created_at")?,
                consume_request_id: optional_uuid_from(&row, "consume_request_id")?,
                operation_id: optional_uuid_from(&row, "operation_id")?,
            })
        })
        .collect()
}

async fn export_stock_reminders(
    db: &Database,
    household_id: Uuid,
) -> Result<Vec<ExportStockReminder>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, batch_id, product_id, location_id, kind, fire_at, title, body, household_timezone, \
                household_fire_local_at, expires_on, acked_at, created_at \
         FROM stock_reminder WHERE household_id = ? ORDER BY fire_at ASC, id ASC",
    )
    .bind(household_id.to_string())
    .fetch_all(&db.pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ExportStockReminder {
                id: uuid_from(&row, "id")?,
                batch_id: uuid_from(&row, "batch_id")?,
                product_id: uuid_from(&row, "product_id")?,
                location_id: uuid_from(&row, "location_id")?,
                kind: row.try_get("kind")?,
                fire_at: row.try_get("fire_at")?,
                title: row.try_get("title")?,
                body: row.try_get("body")?,
                household_timezone: row.try_get("household_timezone")?,
                household_fire_local_at: row.try_get("household_fire_local_at")?,
                expires_on: row.try_get("expires_on")?,
                acked_at: row.try_get("acked_at")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect()
}

fn uuid_from(row: &sqlx::any::AnyRow, column: &str) -> Result<Uuid, sqlx::Error> {
    let raw: String = row.try_get(column)?;
    Uuid::parse_str(&raw).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

fn optional_uuid_from(row: &sqlx::any::AnyRow, column: &str) -> Result<Option<Uuid>, sqlx::Error> {
    let raw: Option<String> = row.try_get(column)?;
    raw.map(|s| Uuid::parse_str(&s))
        .transpose()
        .map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

fn bool_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{households, locations, memberships, products, stock, users};

    #[tokio::test]
    async fn purge_household_removes_tenant_rows_and_is_idempotent() {
        let db = crate::test_db().await;
        let household = households::create(&db, "Home", "UTC").await.unwrap();
        locations::seed_defaults(&db, household.id).await.unwrap();
        let user = users::create(&db, "alice@example.com", "Alice", "hash")
            .await
            .unwrap();
        memberships::insert(&db, household.id, user.id, "admin")
            .await
            .unwrap();
        let pantry = locations::list_for_household(&db, household.id)
            .await
            .unwrap()
            .into_iter()
            .find(|loc| loc.kind == "pantry")
            .unwrap();
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
        stock::create(
            &db,
            household.id,
            product.id,
            pantry.id,
            "100",
            "g",
            None,
            None,
            None,
            None,
            user.id,
            None,
        )
        .await
        .unwrap();

        purge_household(&db, household.id).await.unwrap();
        purge_household(&db, household.id).await.unwrap();

        let household_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM household WHERE id = ?")
                .bind(household.id.to_string())
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let batch_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM stock_batch WHERE household_id = ?")
                .bind(household.id.to_string())
                .fetch_one(&db.pool)
                .await
                .unwrap();
        let event_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM stock_event WHERE household_id = ?")
                .bind(household.id.to_string())
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(household_count, 0);
        assert_eq!(batch_count, 0);
        assert_eq!(event_count, 0);
    }
}
