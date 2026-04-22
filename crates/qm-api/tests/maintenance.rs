mod support;

use axum::http::{HeaderMap, Method, StatusCode};
use chrono::{Duration, Utc};
use qm_api::ApiConfig;
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
        Utc::now() - Duration::minutes(5),
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
}
