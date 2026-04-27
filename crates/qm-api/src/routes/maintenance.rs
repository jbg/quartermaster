use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use jiff::Timestamp;
use metrics::counter;
use serde::Serialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AppState};

pub const MAINTENANCE_TOKEN_HEADER: &str = "x-qm-maintenance-token";
const SMOKE_USERNAME: &str = "quartermaster_smoke_18423";
const SMOKE_PASSWORD: &str = "quartermaster-smoke-18423";
const SMOKE_EMAIL: &str = "quartermaster-smoke@example.com";
const SMOKE_HOUSEHOLD_NAME: &str = "Quartermaster Smoke Household";
const SMOKE_TIMEZONE: &str = "UTC";
const SMOKE_INVITE_EXPIRES_AT: &str = "2999-01-01T00:00:00Z";
const SMOKE_REMINDER_FIRE_AT: &str = "2000-01-01T00:00:00.000Z";
const SMOKE_SERVER_URL: &str = "http://127.0.0.1:8080";
const SMOKE_PRODUCT_PREFIX: &str = "Smoke Product %";
const SMOKE_LOCATION_PREFIX: &str = "Smoke Shelf %";
const SMOKE_BARCODE: &str = "1111111111111";
const SMOKE_PRODUCTS: [(&str, &str); 2] = [
    ("Smoke Rice", "Smoke fixture seed 1"),
    ("Smoke Beans", "Smoke fixture seed 2"),
];

#[derive(Debug, Serialize)]
pub struct SweepAuthSessionsResponse {
    pub deleted_sessions: u64,
}

#[derive(Debug, Serialize)]
pub struct SweepExpiryRemindersResponse {
    pub inserted: u64,
    pub deleted: u64,
}

#[derive(Debug, Serialize)]
pub struct SmokeReminderSeed {
    pub reminder_id: Uuid,
    pub batch_id: Uuid,
    pub product_id: Uuid,
    pub location_id: Uuid,
    pub kind: String,
    pub product_name: String,
    pub location_name: String,
    pub quantity: String,
    pub unit: String,
    pub expires_on: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SeedSmokeResponse {
    pub username: String,
    pub password: String,
    pub invite_code: String,
    pub server_url: String,
    pub household_id: Uuid,
    pub location_id: Uuid,
    pub barcode: String,
    pub reminders: Vec<SmokeReminderSeed>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/maintenance/sweep-auth-sessions",
            post(sweep_auth_sessions),
        )
        .route(
            "/internal/maintenance/sweep-expiry-reminders",
            post(sweep_expiry_reminders),
        )
        .route("/internal/maintenance/seed-smoke", post(seed_smoke))
}

async fn sweep_auth_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<SweepAuthSessionsResponse>)> {
    let provided = headers
        .get(MAINTENANCE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    let expected = state
        .config
        .auth_session_sweep_trigger_secret
        .as_deref()
        .ok_or(ApiError::NotFound)?;

    if provided != Some(expected) {
        return Err(ApiError::Unauthorized);
    }

    let deleted_sessions = match qm_db::auth_sessions::delete_stale_sessions(
        &state.db,
        &qm_db::now_utc_rfc3339(),
        qm_db::auth_sessions::STALE_SESSION_SWEEP_BATCH_SIZE,
    )
    .await
    {
        Ok(deleted_sessions) => deleted_sessions,
        Err(err) => {
            counter!("qm_auth_session_sweeps_total", "surface" => "manual", "outcome" => "failure")
                .increment(1);
            return Err(err.into());
        }
    };
    counter!("qm_auth_session_sweeps_total", "surface" => "manual", "outcome" => "success")
        .increment(1);
    counter!("qm_auth_session_swept_sessions_total", "surface" => "manual")
        .increment(deleted_sessions);

    Ok((
        StatusCode::OK,
        Json(SweepAuthSessionsResponse { deleted_sessions }),
    ))
}

async fn sweep_expiry_reminders(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<SweepExpiryRemindersResponse>)> {
    require_maintenance_secret(
        &headers,
        state.config.expiry_reminder_trigger_secret.as_deref(),
    )?;

    let stats = match qm_db::reminders::reconcile_all(
        &state.db,
        &state.config.expiry_reminder_policy,
    )
    .await
    {
        Ok(stats) => stats,
        Err(err) => {
            counter!("qm_expiry_reminder_sweeps_total", "surface" => "manual", "outcome" => "failure")
                .increment(1);
            return Err(err.into());
        }
    };
    counter!("qm_expiry_reminder_sweeps_total", "surface" => "manual", "outcome" => "success")
        .increment(1);
    counter!("qm_expiry_reminder_sweep_inserted_total", "surface" => "manual")
        .increment(stats.inserted);
    counter!("qm_expiry_reminder_sweep_deleted_total", "surface" => "manual")
        .increment(stats.deleted);
    Ok((
        StatusCode::OK,
        Json(SweepExpiryRemindersResponse {
            inserted: stats.inserted,
            deleted: stats.deleted,
        }),
    ))
}

async fn seed_smoke(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<(StatusCode, Json<SeedSmokeResponse>)> {
    require_maintenance_secret(&headers, state.config.smoke_seed_trigger_secret.as_deref())?;

    let payload = build_smoke_fixture(&state).await?;
    Ok((StatusCode::OK, Json(payload)))
}

fn require_maintenance_secret(headers: &HeaderMap, expected: Option<&str>) -> ApiResult<()> {
    let provided = headers
        .get(MAINTENANCE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    let expected = expected.ok_or(ApiError::NotFound)?;
    if provided != Some(expected) {
        return Err(ApiError::Unauthorized);
    }
    Ok(())
}

async fn build_smoke_fixture(state: &AppState) -> Result<SeedSmokeResponse, ApiError> {
    let user = find_or_create_smoke_user(&state.db).await?;
    let household_id = find_or_create_smoke_household(&state.db, user.id).await?;
    let pantry = ensure_pantry_location(&state.db, household_id).await?;
    ensure_smoke_barcode_product(&state.db).await?;
    reset_smoke_fixture_artifacts(&state.db, household_id).await?;

    let policy = smoke_reminder_policy(&state.config.expiry_reminder_policy);
    let mut reminders = Vec::with_capacity(SMOKE_PRODUCTS.len());
    for (product_name, note) in SMOKE_PRODUCTS {
        let product = find_or_create_smoke_product(&state.db, household_id, product_name).await?;
        let batch = qm_db::stock::create(
            &state.db,
            household_id,
            product.id,
            pantry.id,
            "500",
            "g",
            Some("2999-01-03"),
            None,
            Some(note),
            user.id,
            Some(&policy),
        )
        .await?;
        let reminder =
            qm_db::reminders::force_due_for_batch(&state.db, batch.id, SMOKE_REMINDER_FIRE_AT)
                .await?
                .ok_or_else(|| {
                    ApiError::BadRequest("smoke reminder fixture was not created".into())
                })?;
        reminders.push(SmokeReminderSeed {
            reminder_id: reminder.id,
            batch_id: reminder.batch_id,
            product_id: reminder.product_id,
            location_id: reminder.location_id,
            kind: reminder.kind,
            product_name: reminder.product_name,
            location_name: reminder.location_name,
            quantity: reminder.quantity,
            unit: reminder.unit,
            expires_on: reminder.expires_on,
        });
    }

    let invite = find_or_create_smoke_invite(&state.db, household_id, user.id).await?;

    Ok(SeedSmokeResponse {
        username: SMOKE_USERNAME.into(),
        password: SMOKE_PASSWORD.into(),
        invite_code: invite.code,
        server_url: state
            .config
            .public_base_url
            .clone()
            .unwrap_or_else(|| SMOKE_SERVER_URL.into()),
        household_id,
        location_id: pantry.id,
        barcode: SMOKE_BARCODE.into(),
        reminders,
    })
}

async fn find_or_create_smoke_user(
    db: &qm_db::Database,
) -> Result<qm_db::users::UserRow, ApiError> {
    if let Some(existing) = qm_db::users::find_by_username(db, SMOKE_USERNAME).await? {
        return Ok(existing);
    }

    let password_hash = crate::auth::hash_password(SMOKE_PASSWORD)
        .map_err(|err| ApiError::BadRequest(format!("failed to hash smoke password: {err}")))?;
    qm_db::users::create(db, SMOKE_USERNAME, Some(SMOKE_EMAIL), &password_hash)
        .await
        .map_err(Into::into)
}

async fn find_or_create_smoke_household(
    db: &qm_db::Database,
    user_id: Uuid,
) -> Result<Uuid, ApiError> {
    if let Some(existing) = qm_db::memberships::list_for_user(db, user_id)
        .await?
        .into_iter()
        .next()
    {
        return Ok(existing.membership.household_id);
    }

    let household = qm_db::households::create(db, SMOKE_HOUSEHOLD_NAME, SMOKE_TIMEZONE).await?;
    qm_db::locations::seed_defaults(db, household.id).await?;
    qm_db::memberships::insert(db, household.id, user_id, "admin").await?;
    Ok(household.id)
}

async fn reset_smoke_fixture_artifacts(
    db: &qm_db::Database,
    household_id: Uuid,
) -> Result<(), ApiError> {
    let mut tx = db.pool.begin().await?;
    let household_id = household_id.to_string();

    sqlx::query(
        "DELETE FROM reminder_delivery \
         WHERE reminder_id IN ( \
           SELECT r.id \
           FROM stock_reminder r \
           INNER JOIN stock_batch b ON b.id = r.batch_id \
           INNER JOIN product p ON p.id = b.product_id \
           LEFT JOIN location l ON l.id = b.location_id \
           WHERE b.household_id = ? \
             AND (p.name = ? OR p.name = ? OR p.name LIKE ? OR l.name LIKE ?) \
         )",
    )
    .bind(&household_id)
    .bind(SMOKE_PRODUCTS[0].0)
    .bind(SMOKE_PRODUCTS[1].0)
    .bind(SMOKE_PRODUCT_PREFIX)
    .bind(SMOKE_LOCATION_PREFIX)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM reminder_device_state \
         WHERE reminder_id IN ( \
           SELECT r.id \
           FROM stock_reminder r \
           INNER JOIN stock_batch b ON b.id = r.batch_id \
           INNER JOIN product p ON p.id = b.product_id \
           LEFT JOIN location l ON l.id = b.location_id \
           WHERE b.household_id = ? \
             AND (p.name = ? OR p.name = ? OR p.name LIKE ? OR l.name LIKE ?) \
         )",
    )
    .bind(&household_id)
    .bind(SMOKE_PRODUCTS[0].0)
    .bind(SMOKE_PRODUCTS[1].0)
    .bind(SMOKE_PRODUCT_PREFIX)
    .bind(SMOKE_LOCATION_PREFIX)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM stock_reminder \
         WHERE id IN ( \
           SELECT r.id \
           FROM stock_reminder r \
           INNER JOIN stock_batch b ON b.id = r.batch_id \
           INNER JOIN product p ON p.id = b.product_id \
           LEFT JOIN location l ON l.id = b.location_id \
           WHERE b.household_id = ? \
             AND (p.name = ? OR p.name = ? OR p.name LIKE ? OR l.name LIKE ?) \
         )",
    )
    .bind(&household_id)
    .bind(SMOKE_PRODUCTS[0].0)
    .bind(SMOKE_PRODUCTS[1].0)
    .bind(SMOKE_PRODUCT_PREFIX)
    .bind(SMOKE_LOCATION_PREFIX)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM stock_event \
         WHERE batch_id IN ( \
           SELECT b.id \
           FROM stock_batch b \
           INNER JOIN product p ON p.id = b.product_id \
           LEFT JOIN location l ON l.id = b.location_id \
           WHERE b.household_id = ? \
             AND (p.name = ? OR p.name = ? OR p.name LIKE ? OR l.name LIKE ?) \
         )",
    )
    .bind(&household_id)
    .bind(SMOKE_PRODUCTS[0].0)
    .bind(SMOKE_PRODUCTS[1].0)
    .bind(SMOKE_PRODUCT_PREFIX)
    .bind(SMOKE_LOCATION_PREFIX)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM stock_batch \
         WHERE id IN ( \
           SELECT b.id \
           FROM stock_batch b \
           INNER JOIN product p ON p.id = b.product_id \
           LEFT JOIN location l ON l.id = b.location_id \
           WHERE b.household_id = ? \
             AND (p.name = ? OR p.name = ? OR p.name LIKE ? OR l.name LIKE ?) \
         )",
    )
    .bind(&household_id)
    .bind(SMOKE_PRODUCTS[0].0)
    .bind(SMOKE_PRODUCTS[1].0)
    .bind(SMOKE_PRODUCT_PREFIX)
    .bind(SMOKE_LOCATION_PREFIX)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM product \
         WHERE created_by_household_id = ? \
           AND (name = ? OR name = ? OR name LIKE ?)",
    )
    .bind(&household_id)
    .bind(SMOKE_PRODUCTS[0].0)
    .bind(SMOKE_PRODUCTS[1].0)
    .bind(SMOKE_PRODUCT_PREFIX)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM location WHERE household_id = ? AND name LIKE ?")
        .bind(&household_id)
        .bind(SMOKE_LOCATION_PREFIX)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

async fn ensure_pantry_location(
    db: &qm_db::Database,
    household_id: Uuid,
) -> Result<qm_db::locations::LocationRow, ApiError> {
    if let Some(existing) = qm_db::locations::list_for_household(db, household_id)
        .await?
        .into_iter()
        .find(|location| location.kind == "pantry")
    {
        return Ok(existing);
    }

    qm_db::locations::create(db, household_id, "Pantry", "pantry", 0)
        .await
        .map_err(Into::into)
}

async fn find_or_create_smoke_product(
    db: &qm_db::Database,
    household_id: Uuid,
    product_name: &str,
) -> Result<qm_db::products::ProductRow, ApiError> {
    if let Some(existing) =
        qm_db::products::search_with_deleted(db, household_id, product_name, 20, true)
            .await?
            .into_iter()
            .find(|product| {
                product.name == product_name
                    && product.created_by_household_id == Some(household_id)
            })
    {
        if existing.deleted_at.is_none() {
            return Ok(existing);
        }
    }

    qm_db::products::create_manual(
        db,
        household_id,
        product_name,
        Some("Quartermaster"),
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .map_err(Into::into)
}

async fn ensure_smoke_barcode_product(
    db: &qm_db::Database,
) -> Result<qm_db::products::ProductRow, ApiError> {
    let product = qm_db::products::upsert_from_off(
        db,
        SMOKE_BARCODE,
        "Retry Beans",
        Some("Acme"),
        "mass",
        Some("g"),
        None,
    )
    .await?;
    qm_db::barcode_cache::put_hit(db, SMOKE_BARCODE, product.id).await?;
    Ok(product)
}

async fn find_or_create_smoke_invite(
    db: &qm_db::Database,
    household_id: Uuid,
    created_by: Uuid,
) -> Result<qm_db::invites::InviteRow, ApiError> {
    let now = Timestamp::now();
    if let Some(existing) = qm_db::invites::list_for_household(db, household_id)
        .await?
        .into_iter()
        .find(|invite| {
            invite.use_count < invite.max_uses
                && invite
                    .expires_at
                    .parse::<Timestamp>()
                    .map(|expires_at| expires_at > now)
                    .unwrap_or(false)
        })
    {
        return Ok(existing);
    }

    let code = Uuid::now_v7().simple().to_string()[..12].to_ascii_uppercase();
    qm_db::invites::create(
        db,
        household_id,
        &code,
        created_by,
        SMOKE_INVITE_EXPIRES_AT,
        2,
        "member",
    )
    .await
    .map_err(Into::into)
}

fn smoke_reminder_policy(
    configured: &qm_db::reminders::ExpiryReminderPolicy,
) -> qm_db::reminders::ExpiryReminderPolicy {
    let mut policy = configured.clone();
    policy.enabled = true;
    policy
}
