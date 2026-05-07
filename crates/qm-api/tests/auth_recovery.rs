mod support;

use std::{future::Future, pin::Pin, sync::Arc};

use axum::http::{Method, StatusCode};
use jiff::{SignedDuration, Timestamp};
use qm_api::{
    email::{EmailDeliveryError, EmailMessage, EmailTransport},
    ApiConfig,
};
use serde_json::json;
use support::TestApp;
use tokio::sync::Mutex;

#[derive(Debug, Default)]
struct CaptureEmailTransport {
    messages: Mutex<Vec<EmailMessage>>,
}

impl CaptureEmailTransport {
    async fn messages(&self) -> Vec<EmailMessage> {
        self.messages.lock().await.clone()
    }
}

impl EmailTransport for CaptureEmailTransport {
    fn send<'a>(
        &'a self,
        message: EmailMessage,
    ) -> Pin<Box<dyn Future<Output = Result<(), EmailDeliveryError>> + Send + 'a>> {
        Box::pin(async move {
            self.messages.lock().await.push(message);
            Ok(())
        })
    }
}

#[tokio::test]
async fn request_recovery_email_exposes_pending_state_on_me() {
    let email = Arc::new(CaptureEmailTransport::default());
    let app = TestApp::start_with_email(ApiConfig::default(), email.clone()).await;
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
    let messages = email.messages().await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].to.email, "alice@example.com");
    assert!(messages[0].text_body.contains("code"));
}

#[tokio::test]
async fn request_recovery_email_requires_configured_transport() {
    let app = TestApp::start(ApiConfig::default()).await;
    app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/auth/email-verification",
            Some(json!({ "email": "alice@example.com" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "service_unavailable");
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
    let email = Arc::new(CaptureEmailTransport::default());
    let app = TestApp::start_with_email(ApiConfig::default(), email).await;
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

#[tokio::test]
async fn password_reset_request_is_generic_and_sends_when_recoverable() {
    let email = Arc::new(CaptureEmailTransport::default());
    let app = TestApp::start_with_email(
        ApiConfig {
            public_base_url: Some("https://quartermaster.example".into()),
            ..ApiConfig::default()
        },
        email.clone(),
    )
    .await;
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

    for username in ["alice", "missing"] {
        let (status, body) = app
            .send(
                Method::POST,
                "/api/v1/auth/password-reset/request",
                Some(json!({ "username": username })),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert_eq!(body["status"], "accepted");
    }

    let messages = email.messages().await;
    assert_eq!(messages.len(), 1);
    assert!(messages[0]
        .text_body
        .contains("/reset-password?username=alice&token="));
}

#[tokio::test]
async fn password_reset_confirm_updates_password_and_revokes_sessions() {
    let email = Arc::new(CaptureEmailTransport::default());
    let app = TestApp::start_with_email(ApiConfig::default(), email.clone()).await;
    let (_, user_id) = app.seed_household_admin("alice").await;
    let old_token = app.login("alice").await;
    let expires_at = qm_db::time::format_timestamp(
        Timestamp::now()
            .checked_add(SignedDuration::from_mins(30))
            .unwrap(),
    );
    qm_db::users::create_password_reset(
        &app.db,
        user_id,
        &qm_api::auth::sha256_hex("RESET12345"),
        &qm_api::auth::sha256_hex("reset-token"),
        &expires_at,
    )
    .await
    .unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/password-reset/confirm",
            Some(json!({
                "username": "alice",
                "new_password": "newpassword123",
                "code": "reset-12345"
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    assert_eq!(
        app.login_with_password("alice", "password123").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(app
        .login_with_password("alice", "newpassword123")
        .await
        .0
        .is_success());
    assert_eq!(
        app.send(Method::GET, "/api/v1/auth/me", None, Some(&old_token))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn password_reset_confirm_rejects_bad_or_consumed_secret() {
    let app = TestApp::start(ApiConfig::default()).await;
    let (_, user_id) = app.seed_household_admin("alice").await;
    let expires_at = qm_db::time::format_timestamp(
        Timestamp::now()
            .checked_add(SignedDuration::from_mins(30))
            .unwrap(),
    );
    qm_db::users::create_password_reset(
        &app.db,
        user_id,
        &qm_api::auth::sha256_hex("RIGHTCODE1"),
        &qm_api::auth::sha256_hex("right-token"),
        &expires_at,
    )
    .await
    .unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/password-reset/confirm",
            Some(json!({
                "username": "alice",
                "new_password": "newpassword123",
                "code": "WRONGCODE1"
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/password-reset/confirm",
            Some(json!({
                "username": "alice",
                "new_password": "newpassword123",
                "token": "right-token"
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/password-reset/confirm",
            Some(json!({
                "username": "alice",
                "new_password": "anotherpassword123",
                "token": "right-token"
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
