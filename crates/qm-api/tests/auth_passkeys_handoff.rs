mod support;

use axum::http::{Method, StatusCode};
use jiff::{SignedDuration, Timestamp};
use qm_api::{ApiConfig, PasskeyConfig};
use serde_json::json;
use uuid::Uuid;

use support::TestApp;

#[tokio::test]
async fn onboarding_reports_passkeys_only_when_configured() {
    let app = TestApp::start(ApiConfig::default()).await;
    let (status, body) = app
        .send(Method::GET, "/api/v1/onboarding/status", None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let passkey = body["auth_methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|method| method["method"] == "passkey")
        .unwrap();
    assert_eq!(passkey["availability"], "unavailable");
    assert_eq!(passkey["unavailable_reason"], "not_configured");

    let app = TestApp::start(ApiConfig {
        passkeys: PasskeyConfig {
            enabled: true,
            rp_id: Some("localhost".into()),
            origin: Some("http://localhost".into()),
            rp_name: "Quartermaster".into(),
        },
        ..ApiConfig::default()
    })
    .await;
    let (status, body) = app
        .send(Method::GET, "/api/v1/onboarding/status", None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let passkey = body["auth_methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|method| method["method"] == "passkey")
        .unwrap();
    assert_eq!(passkey["availability"], "enabled");
    assert!(passkey["unavailable_reason"].is_null());
}

#[tokio::test]
async fn handoff_preview_and_accept_issue_session_once() {
    let app = TestApp::start(ApiConfig {
        public_base_url: Some("https://quartermaster.example.com".into()),
        ..ApiConfig::default()
    })
    .await;
    let _ = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs",
            Some(json!({"target_device_label": "Alice's iPad"})),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let handoff_url = body["handoff_url"].as_str().unwrap();
    assert!(handoff_url.starts_with("quartermaster://handoff?"));
    assert!(handoff_url.contains("server=https%3A%2F%2Fquartermaster%2Eexample%2Ecom"));

    let (id, token) = handoff_parts(handoff_url);
    let (status, preview) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs/preview",
            Some(json!({"id": id, "token": token})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["source_email"], "alice@example.com");
    assert_eq!(preview["source_display_name"], "alice");
    assert_eq!(preview["target_device_label"], "Alice's iPad");

    let (status, accepted) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs/accept",
            Some(json!({"id": id, "token": token, "device_label": "Accepted iPad"})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let accepted_access = accepted["access_token"].as_str().unwrap();
    let me = app.me(accepted_access).await;
    assert_eq!(me["user"]["email"], "alice@example.com");
    assert!(me["current_household"]["id"].as_str().is_some());

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs/accept",
            Some(json!({"id": id, "token": token})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handoff_cancel_and_wrong_token_fail_closed() {
    let app = TestApp::start(ApiConfig::default()).await;
    let _ = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs",
            Some(json!({
                "target_device_label": "Phone",
                "server_url": "http://localhost:8080"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (id, token) = handoff_parts(body["handoff_url"].as_str().unwrap());

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs/preview",
            Some(json!({"id": id, "token": "wrong"})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = app
        .send(
            Method::DELETE,
            &format!("/api/v1/auth/handoffs/{id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs/preview",
            Some(json!({"id": id, "token": token})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handoff_expiry_fails_closed() {
    let app = TestApp::start(ApiConfig::default()).await;
    let (household_id, user_id) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let session = qm_db::tokens::find_active_by_hash(&app.db, &qm_api::auth::sha256_hex(&alice))
        .await
        .unwrap()
        .unwrap()
        .session_id;
    let token = "expired-secret";
    let handoff = qm_db::auth_handoff::create(
        &app.db,
        user_id,
        session,
        Some(household_id),
        Some("Phone"),
        &qm_api::auth::sha256_hex(token),
        Timestamp::now()
            .checked_sub(SignedDuration::from_mins(1))
            .unwrap(),
    )
    .await
    .unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/handoffs/accept",
            Some(json!({"id": handoff.id, "token": token})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

fn handoff_parts(url: &str) -> (Uuid, String) {
    let url = reqwest::Url::parse(url).unwrap();
    let query = url.query_pairs().collect::<Vec<_>>();
    let id = query
        .iter()
        .find(|(key, _)| key == "id")
        .map(|(_, value)| Uuid::parse_str(value).unwrap());
    let token = query
        .iter()
        .find(|(key, _)| key == "token")
        .map(|(_, value)| value.to_string());
    (id.unwrap(), token.unwrap())
}
