mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, IosReleaseIdentity, RegistrationMode};
use serde_json::Value;
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
    assert!(raw.contains("Open in Quartermaster"));
}

#[tokio::test]
async fn apple_app_site_association_is_served_from_well_known_path_when_ios_identity_is_present() {
    let app = TestApp::start(ApiConfig {
        ios_release_identity: Some(
            IosReleaseIdentity::new("42J2SSX5SM".into(), "com.example.quartermaster".into())
                .unwrap(),
        ),
        ..ApiConfig::default()
    })
    .await;

    let (status, headers, raw) = app
        .raw(Method::GET, "/.well-known/apple-app-site-association")
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "application/json");
    let body: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        body["applinks"]["details"][0]["appID"].as_str().unwrap(),
        "42J2SSX5SM.com.example.quartermaster"
    );
    assert_eq!(
        body["applinks"]["details"][0]["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["/join", "/join*"]
    );
}

#[tokio::test]
async fn apple_app_site_association_is_not_served_without_ios_identity() {
    let app = TestApp::start(ApiConfig::default()).await;

    let (status, _, raw) = app
        .raw(Method::GET, "/.well-known/apple-app-site-association")
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(raw.is_empty());
}
