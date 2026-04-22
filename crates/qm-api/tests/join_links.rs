mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use support::TestApp;

#[tokio::test]
async fn join_landing_renders_invite_and_server() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;

    let (status, headers, raw) = app
        .raw(
            Method::GET,
            "/join?invite=ABCD1234&server=https%3A%2F%2Fexample.com",
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(raw.contains("ABCD1234"));
    assert!(raw.contains("https://example.com"));
    assert!(raw.contains("quartermaster://join"));
}
