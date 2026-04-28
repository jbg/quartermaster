mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::json;
use support::{me_current_household_id, TestApp};

fn create_household_body(username: &str) -> serde_json::Value {
    json!({
        "username": username,
        "password": "password123",
        "household_name": format!("{username}'s Kitchen"),
        "timezone": "UTC",
    })
}

fn invite_body(max_uses: i64) -> serde_json::Value {
    json!({
        "expires_at": "2999-01-01T00:00:00.000Z",
        "max_uses": max_uses,
        "role_granted": "member",
    })
}

#[tokio::test]
async fn status_reports_initial_setup_until_first_household_exists() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::FirstRunOnly,
        ..ApiConfig::default()
    })
    .await;

    let (status, body) = app
        .send(Method::GET, "/api/v1/onboarding/status", None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["server_state"], "needs_initial_setup");
    assert_eq!(body["household_signup"], "enabled");
    assert_eq!(body["invite_join"], "disabled");
    assert_eq!(body["auth_methods"], json!(["password"]));

    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/onboarding/create-household",
            Some(create_household_body("alice")),
            None,
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let (_, body) = app
        .send(Method::GET, "/api/v1/onboarding/status", None, None)
        .await;
    assert_eq!(body["server_state"], "ready");
    assert_eq!(body["household_signup"], "disabled");
    assert_eq!(body["invite_join"], "enabled");
}

#[tokio::test]
async fn create_household_supports_first_run_and_open_servers() {
    let first_run = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::FirstRunOnly,
        ..ApiConfig::default()
    })
    .await;
    let (status, body) = first_run
        .send(
            Method::POST,
            "/api/v1/onboarding/create-household",
            Some(create_household_body("alice")),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let alice = body["access_token"].as_str().unwrap();
    let me = first_run.me(alice).await;
    assert_eq!(me["user"]["username"], "alice");
    assert!(me_current_household_id(&me).is_some());

    assert_eq!(
        first_run
            .send(
                Method::POST,
                "/api/v1/onboarding/create-household",
                Some(create_household_body("bob")),
                None,
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let open = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(
        open.send(
            Method::POST,
            "/api/v1/onboarding/create-household",
            Some(create_household_body("alice")),
            None,
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        open.send(
            Method::POST,
            "/api/v1/onboarding/create-household",
            Some(create_household_body("bob")),
            None,
        )
        .await
        .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn create_household_rejects_invite_only_and_username_conflicts() {
    let invite_only = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(
        invite_only
            .send(
                Method::POST,
                "/api/v1/onboarding/create-household",
                Some(create_household_body("alice")),
                None,
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let open = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(open.register("alice", None).await.0, StatusCode::CREATED);
    let (status, _) = open
        .send(
            Method::POST,
            "/api/v1/onboarding/create-household",
            Some(create_household_body("alice")),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn join_invite_is_transactional_and_logs_user_in() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let (_, invite) = app
        .send(
            Method::POST,
            "/api/v1/households/current/invites",
            Some(invite_body(1)),
            Some(&alice),
        )
        .await;
    let code = invite["code"].as_str().unwrap();

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/onboarding/join-invite",
            Some(json!({
                "username": "bob",
                "password": "password123",
                "invite_code": code,
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let bob = body["access_token"].as_str().unwrap();
    let me = app.me(bob).await;
    assert_eq!(
        me_current_household_id(&me).unwrap(),
        household_id.to_string()
    );

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/onboarding/join-invite",
            Some(json!({
                "username": "carol",
                "password": "password123",
                "invite_code": code,
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invalid_invite_join_does_not_create_orphaned_user() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/onboarding/join-invite",
            Some(json!({
                "username": "bob",
                "password": "password123",
                "invite_code": "NOPE",
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(qm_db::users::count(&app.db).await.unwrap(), 0);
}
