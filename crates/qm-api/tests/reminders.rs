mod support;

use axum::http::{Method, StatusCode};
use jiff::Timestamp;
use qm_api::ApiConfig;
use qm_db::reminders::ExpiryReminderPolicy;
use serde_json::json;
use support::TestApp;

fn enabled_policy() -> ExpiryReminderPolicy {
    ExpiryReminderPolicy {
        enabled: true,
        ..ExpiryReminderPolicy::default()
    }
}

#[tokio::test]
async fn list_returns_due_reminders_for_active_household_only() {
    let app = TestApp::start(ApiConfig {
        expiry_reminder_policy: enabled_policy(),
        ..ApiConfig::default()
    })
    .await;

    let (household_a, user_id) = app.seed_household_admin("alice").await;
    let household_b = qm_db::households::create(&app.db, "Cabin", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, household_b.id)
        .await
        .unwrap();
    qm_db::memberships::insert(&app.db, household_b.id, user_id, "admin")
        .await
        .unwrap();

    let pantry_a = qm_db::locations::list_for_household(&app.db, household_a)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.kind == "pantry")
        .unwrap()
        .id;
    let pantry_b = qm_db::locations::list_for_household(&app.db, household_b.id)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.kind == "pantry")
        .unwrap()
        .id;
    let product_a = qm_db::products::create_manual(
        &app.db,
        household_a,
        "Milk",
        None,
        "volume",
        Some("ml"),
        None,
        None,
    )
    .await
    .unwrap();
    let product_b = qm_db::products::create_manual(
        &app.db,
        household_b.id,
        "Yogurt",
        None,
        "count",
        Some("piece"),
        None,
        None,
    )
    .await
    .unwrap();

    let batch_a = qm_db::stock::create(
        &app.db,
        household_a,
        product_a.id,
        pantry_a,
        "1000",
        "ml",
        Some("2999-01-03"),
        None,
        None,
        user_id,
        Some(&enabled_policy()),
    )
    .await
    .unwrap();
    let batch_b = qm_db::stock::create(
        &app.db,
        household_b.id,
        product_b.id,
        pantry_b,
        "2",
        "piece",
        Some("2999-01-03"),
        None,
        None,
        user_id,
        Some(&enabled_policy()),
    )
    .await
    .unwrap();

    sqlx::query(
        "UPDATE stock_reminder SET fire_at = '2000-01-01T00:00:00.000Z' WHERE batch_id = ?",
    )
    .bind(batch_a.id.to_string())
    .execute(&app.db.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE stock_reminder SET fire_at = '2000-01-01T00:00:00.000Z' WHERE batch_id = ?",
    )
    .bind(batch_b.id.to_string())
    .execute(&app.db.pool)
    .await
    .unwrap();

    let alice = app.login("alice").await;
    let switched = app
        .send(
            Method::POST,
            "/auth/switch-household",
            Some(json!({ "household_id": household_a })),
            Some(&alice),
        )
        .await;
    assert_eq!(switched.0, StatusCode::OK);

    let (status, body) = app
        .send(Method::GET, "/reminders", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["items"][0]["batch_id"].as_str().unwrap(),
        batch_a.id.to_string()
    );

    let switched = app
        .send(
            Method::POST,
            "/auth/switch-household",
            Some(json!({ "household_id": household_b.id })),
            Some(&alice),
        )
        .await;
    assert_eq!(switched.0, StatusCode::OK);

    let (status, body) = app
        .send(Method::GET, "/reminders", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["items"][0]["batch_id"].as_str().unwrap(),
        batch_b.id.to_string()
    );
}

#[tokio::test]
async fn present_open_and_ack_are_device_aware() {
    let app = TestApp::start(ApiConfig {
        expiry_reminder_policy: enabled_policy(),
        ..ApiConfig::default()
    })
    .await;
    let (household_id, user_id) = app.seed_household_admin("alice").await;
    let pantry = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.kind == "pantry")
        .unwrap()
        .id;
    let product = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Butter",
        None,
        "count",
        Some("piece"),
        None,
        None,
    )
    .await
    .unwrap();
    let batch = qm_db::stock::create(
        &app.db,
        household_id,
        product.id,
        pantry,
        "1",
        "piece",
        Some("2999-01-03"),
        None,
        None,
        user_id,
        Some(&enabled_policy()),
    )
    .await
    .unwrap();

    sqlx::query("UPDATE stock_reminder SET fire_at = ? WHERE batch_id = ?")
        .bind(qm_db::time::format_timestamp(Timestamp::now()))
        .bind(batch.id.to_string())
        .execute(&app.db.pool)
        .await
        .unwrap();

    let alice = app.login("alice").await;
    let (status, _) = app
        .send(
            Method::POST,
            "/devices/register",
            Some(json!({
                "device_id": "ios-main",
                "platform": "ios",
                "push_authorization": "authorized",
                "push_token": "token-1",
                "app_version": "0.1",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = app
        .send(Method::GET, "/reminders", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    let reminder_id = body["items"][0]["id"].as_str().unwrap().to_owned();
    assert!(body["items"][0]["presented_on_device_at"].is_null());
    assert!(body["items"][0]["opened_on_device_at"].is_null());
    assert!(body["items"][0].get("acked_at").is_none());

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/reminders/{reminder_id}/present"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    let (status, body) = app
        .send(Method::GET, "/reminders", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"][0]["presented_on_device_at"].is_string());
    assert!(body["items"][0]["opened_on_device_at"].is_null());

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/reminders/{reminder_id}/open"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    let (status, body) = app
        .send(Method::GET, "/reminders", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"][0]["opened_on_device_at"].is_string());

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/reminders/{reminder_id}/ack"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());

    let (status, _) = app
        .send(
            Method::POST,
            &format!("/reminders/{reminder_id}/ack"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = app
        .send(Method::GET, "/reminders", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().unwrap().is_empty());
}
