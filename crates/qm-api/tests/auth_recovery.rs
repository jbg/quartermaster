mod support;

use axum::http::{Method, StatusCode};
use jiff::{SignedDuration, Timestamp};
use qm_api::ApiConfig;
use serde_json::json;
use support::TestApp;

#[tokio::test]
async fn request_recovery_email_exposes_pending_state_on_me() {
    let app = TestApp::start(ApiConfig::default()).await;
    app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/auth/email-verification",
            Some(json!({ "email": " Alice@Example.COM " })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pending_email"], "alice@example.com");
    assert!(body["expires_at"].as_str().is_some());

    let me = app.me(&alice).await;
    assert_eq!(me["user"]["email"], serde_json::Value::Null);
    assert_eq!(me["user"]["email_verified_at"], serde_json::Value::Null);
    assert_eq!(me["user"]["pending_email"], "alice@example.com");
    assert!(me["user"]["pending_email_verification_expires_at"]
        .as_str()
        .is_some());
}

#[tokio::test]
async fn confirm_recovery_email_consumes_matching_pending_code() {
    let app = TestApp::start(ApiConfig::default()).await;
    let (_, user_id) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let expires_at = qm_db::time::format_timestamp(
        Timestamp::now()
            .checked_add(SignedDuration::from_mins(30))
            .unwrap(),
    );
    qm_db::users::create_email_verification(
        &app.db,
        user_id,
        "alice@example.com",
        &qm_api::auth::sha256_hex("ABC1234567"),
        &expires_at,
    )
    .await
    .unwrap();

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/auth/email-verification/confirm",
            Some(json!({ "code": "abc-123-4567" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], "alice@example.com");
    assert!(body["user"]["email_verified_at"].as_str().is_some());
    assert_eq!(body["user"]["pending_email"], serde_json::Value::Null);
}

#[tokio::test]
async fn confirm_recovery_email_rejects_wrong_or_expired_codes() {
    let app = TestApp::start(ApiConfig::default()).await;
    let (_, user_id) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let expires_at = qm_db::time::format_timestamp(
        Timestamp::now()
            .checked_add(SignedDuration::from_mins(-1))
            .unwrap(),
    );
    qm_db::users::create_email_verification(
        &app.db,
        user_id,
        "alice@example.com",
        &qm_api::auth::sha256_hex("ABC1234567"),
        &expires_at,
    )
    .await
    .unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/email-verification/confirm",
            Some(json!({ "code": "ABC1234567" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let future = qm_db::time::format_timestamp(
        Timestamp::now()
            .checked_add(SignedDuration::from_mins(30))
            .unwrap(),
    );
    qm_db::users::create_email_verification(
        &app.db,
        user_id,
        "alice@example.com",
        &qm_api::auth::sha256_hex("RIGHTCODE1"),
        &future,
    )
    .await
    .unwrap();
    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/email-verification/confirm",
            Some(json!({ "code": "WRONGCODE1" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn clear_recovery_email_removes_verified_and_pending_email() {
    let app = TestApp::start(ApiConfig::default()).await;
    let (_, user_id) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let expires_at = qm_db::time::format_timestamp(
        Timestamp::now()
            .checked_add(SignedDuration::from_mins(30))
            .unwrap(),
    );
    qm_db::users::create_email_verification(
        &app.db,
        user_id,
        "alice@example.com",
        &qm_api::auth::sha256_hex("ABC1234567"),
        &expires_at,
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/auth/email-verification/confirm",
            Some(json!({ "code": "ABC1234567" })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/auth/email-verification",
            Some(json!({ "email": "new@example.com" })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::OK
    );

    let (status, body) = app
        .send(Method::DELETE, "/api/v1/auth/email", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], serde_json::Value::Null);
    assert_eq!(body["user"]["email_verified_at"], serde_json::Value::Null);
    assert_eq!(body["user"]["pending_email"], serde_json::Value::Null);
}
