mod support;

use axum::http::{HeaderMap, Method, StatusCode};
use jiff::{SignedDuration, Timestamp};
use qm_api::ApiConfig;
use sqlx::Row;
use support::TestApp;
use uuid::Uuid;

#[tokio::test]
async fn sweep_auth_sessions_requires_shared_secret() {
    let app = TestApp::start(ApiConfig {
        auth_session_sweep_trigger_secret: Some("secret-token".into()),
        ..ApiConfig::default()
    })
    .await;

    let (status, body) = app
        .send(
            Method::POST,
            "/internal/maintenance/sweep-auth-sessions",
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthorized");
}

#[tokio::test]
async fn sweep_auth_sessions_deletes_stale_rows_with_valid_secret() {
    let app = TestApp::start(ApiConfig {
        auth_session_sweep_trigger_secret: Some("secret-token".into()),
        ..ApiConfig::default()
    })
    .await;
    let user_id = app.seed_user_without_household("alice").await;
    let session_id = Uuid::now_v7();

    qm_db::auth_sessions::upsert(&app.db, session_id, user_id, None)
        .await
        .unwrap();
    qm_db::tokens::create(
        &app.db,
        user_id,
        session_id,
        "expired-hash",
        qm_db::tokens::KIND_ACCESS,
        Some("iPhone"),
        Timestamp::now()
            .checked_sub(SignedDuration::from_mins(5))
            .unwrap(),
    )
    .await
    .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        qm_api::routes::maintenance::MAINTENANCE_TOKEN_HEADER,
        "secret-token".parse().unwrap(),
    );

    let (status, body) = app
        .send_with_headers(
            Method::POST,
            "/internal/maintenance/sweep-auth-sessions",
            None,
            None,
            headers,
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted_sessions"], 1);
    assert!(qm_db::auth_sessions::find(&app.db, session_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn maintenance_route_is_unmounted_without_secret() {
    let app = TestApp::start(ApiConfig::default()).await;
    let (status, _) = app
        .send(
            Method::POST,
            "/internal/maintenance/sweep-auth-sessions",
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .send(
            Method::POST,
            "/internal/maintenance/sweep-expiry-reminders",
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .send(
            Method::POST,
            "/internal/maintenance/seed-android-smoke",
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .send(Method::POST, "/internal/maintenance/seed-smoke", None, None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sweep_expiry_reminders_reconciles_rows_with_valid_secret() {
    let app = TestApp::start(ApiConfig {
        expiry_reminder_policy: qm_db::reminders::ExpiryReminderPolicy {
            enabled: true,
            ..Default::default()
        },
        expiry_reminder_trigger_secret: Some("reminder-secret".into()),
        ..ApiConfig::default()
    })
    .await;
    let household = qm_db::households::create(&app.db, "Home", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, household.id)
        .await
        .unwrap();
    let pantry = qm_db::locations::list_for_household(&app.db, household.id)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.kind == "pantry")
        .unwrap()
        .id;
    let user = qm_db::users::create(&app.db, "alice", None, "hash")
        .await
        .unwrap();
    qm_db::memberships::insert(&app.db, household.id, user.id, "admin")
        .await
        .unwrap();
    let product = qm_db::products::create_manual(
        &app.db,
        household.id,
        "Milk",
        None,
        "volume",
        Some("ml"),
        None,
        None,
    )
    .await
    .unwrap();
    qm_db::stock::create(
        &app.db,
        household.id,
        product.id,
        pantry,
        "1000",
        "ml",
        Some("2999-01-03"),
        None,
        None,
        user.id,
        None,
    )
    .await
    .unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(
        qm_api::routes::maintenance::MAINTENANCE_TOKEN_HEADER,
        "reminder-secret".parse().unwrap(),
    );

    let (status, body) = app
        .send_with_headers(
            Method::POST,
            "/internal/maintenance/sweep-expiry-reminders",
            None,
            None,
            headers,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["inserted"], 1);
    assert_eq!(body["deleted"], 0);
}

#[tokio::test]
async fn seed_android_smoke_requires_shared_secret() {
    let app = TestApp::start(ApiConfig {
        android_smoke_seed_trigger_secret: Some("smoke-secret".into()),
        ..ApiConfig::default()
    })
    .await;

    let (status, body) = app
        .send(
            Method::POST,
            "/internal/maintenance/seed-android-smoke",
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthorized");

    let (status, body) = app
        .send(Method::POST, "/internal/maintenance/seed-smoke", None, None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthorized");
}

#[tokio::test]
async fn seed_android_smoke_returns_deterministic_fixture() {
    let app = TestApp::start(ApiConfig {
        android_smoke_seed_trigger_secret: Some("smoke-secret".into()),
        expiry_reminder_policy: qm_db::reminders::ExpiryReminderPolicy {
            enabled: false,
            ..Default::default()
        },
        ..ApiConfig::default()
    })
    .await;

    let mut headers = HeaderMap::new();
    headers.insert(
        qm_api::routes::maintenance::MAINTENANCE_TOKEN_HEADER,
        "smoke-secret".parse().unwrap(),
    );

    let (status, body) = app
        .send_with_headers(
            Method::POST,
            "/internal/maintenance/seed-android-smoke",
            None,
            None,
            headers.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "android_smoke_18423");
    assert_eq!(body["password"], "quartermaster-smoke-18423");
    assert_eq!(body["reminders"].as_array().unwrap().len(), 2);
    assert_eq!(
        smoke_batch_count(&app, body["household_id"].as_str().unwrap()).await,
        2
    );
    assert_eq!(
        smoke_reminder_count(&app, body["household_id"].as_str().unwrap()).await,
        2
    );

    let household_id = Uuid::parse_str(body["household_id"].as_str().unwrap()).unwrap();
    let location_id = Uuid::parse_str(body["location_id"].as_str().unwrap()).unwrap();
    let user = qm_db::users::find_by_username(&app.db, "android_smoke_18423")
        .await
        .unwrap()
        .unwrap();
    let leftover_product = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Android Smoke Product leftover",
        None,
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();
    let leftover_location = qm_db::locations::create(
        &app.db,
        household_id,
        "Android Smoke Shelf leftover",
        "pantry",
        99,
    )
    .await
    .unwrap();
    qm_db::stock::create(
        &app.db,
        household_id,
        leftover_product.id,
        location_id,
        "125",
        "g",
        Some("2999-01-03"),
        None,
        Some("leftover product stock"),
        user.id,
        None,
    )
    .await
    .unwrap();
    qm_db::stock::create(
        &app.db,
        household_id,
        leftover_product.id,
        leftover_location.id,
        "250",
        "g",
        Some("2999-01-03"),
        None,
        Some("leftover location stock"),
        user.id,
        None,
    )
    .await
    .unwrap();

    let (status, body_again) = app
        .send_with_headers(
            Method::POST,
            "/internal/maintenance/seed-smoke",
            None,
            None,
            headers,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body_again["username"], body["username"]);
    assert_eq!(body_again["invite_code"], body["invite_code"]);
    assert_eq!(body_again["reminders"].as_array().unwrap().len(), 2);
    assert_eq!(
        smoke_batch_count(&app, body_again["household_id"].as_str().unwrap()).await,
        2
    );
    assert_eq!(
        smoke_reminder_count(&app, body_again["household_id"].as_str().unwrap()).await,
        2
    );
    assert_eq!(
        leftover_product_count(&app, body_again["household_id"].as_str().unwrap()).await,
        0
    );
    assert_eq!(
        leftover_location_count(&app, body_again["household_id"].as_str().unwrap()).await,
        0
    );

    let user = qm_db::users::find_by_username(&app.db, "android_smoke_18423")
        .await
        .unwrap()
        .unwrap();
    let memberships = qm_db::memberships::list_for_user(&app.db, user.id)
        .await
        .unwrap();
    assert_eq!(memberships.len(), 1);
}

async fn smoke_batch_count(app: &TestApp, household_id: &str) -> i64 {
    sqlx::query(
        "SELECT COUNT(*) AS n \
         FROM stock_batch b \
         INNER JOIN product p ON p.id = b.product_id \
         WHERE b.household_id = ? AND (p.name = 'Smoke Rice' OR p.name = 'Smoke Beans')",
    )
    .bind(household_id)
    .fetch_one(&app.db.pool)
    .await
    .unwrap()
    .try_get("n")
    .unwrap()
}

async fn smoke_reminder_count(app: &TestApp, household_id: &str) -> i64 {
    sqlx::query(
        "SELECT COUNT(*) AS n \
         FROM stock_reminder r \
         INNER JOIN product p ON p.id = r.product_id \
         WHERE r.household_id = ? AND (p.name = 'Smoke Rice' OR p.name = 'Smoke Beans')",
    )
    .bind(household_id)
    .fetch_one(&app.db.pool)
    .await
    .unwrap()
    .try_get("n")
    .unwrap()
}

async fn leftover_product_count(app: &TestApp, household_id: &str) -> i64 {
    sqlx::query(
        "SELECT COUNT(*) AS n \
         FROM product \
         WHERE created_by_household_id = ? AND name LIKE 'Android Smoke Product %'",
    )
    .bind(household_id)
    .fetch_one(&app.db.pool)
    .await
    .unwrap()
    .try_get("n")
    .unwrap()
}

async fn leftover_location_count(app: &TestApp, household_id: &str) -> i64 {
    sqlx::query(
        "SELECT COUNT(*) AS n \
         FROM location \
         WHERE household_id = ? AND name LIKE 'Android Smoke Shelf %'",
    )
    .bind(household_id)
    .fetch_one(&app.db.pool)
    .await
    .unwrap()
    .try_get("n")
    .unwrap()
}
