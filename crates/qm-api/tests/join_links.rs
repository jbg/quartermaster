mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, IosReleaseIdentity, RegistrationMode};
use serde_json::Value;
use support::TestApp;
use uuid::Uuid;

#[tokio::test]
async fn join_landing_is_served_from_web_dist_when_configured() {
    let web_dist = test_web_dist();
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        web_dist_dir: Some(web_dist.clone()),
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
    assert!(raw.contains("quartermaster-web-shell"));

    let _ = std::fs::remove_dir_all(web_dist);
}

#[tokio::test]
async fn api_routes_win_over_web_fallback() {
    let web_dist = test_web_dist();
    let app = TestApp::start(ApiConfig {
        web_dist_dir: Some(web_dist.clone()),
        ..ApiConfig::default()
    })
    .await;

    let (status, body) = app.send(Method::GET, "/api/v1/auth/me", None, None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthorized");

    let _ = std::fs::remove_dir_all(web_dist);
}

#[tokio::test]
async fn root_api_like_paths_belong_to_the_web_app() {
    let web_dist = test_web_dist();
    let app = TestApp::start(ApiConfig {
        web_dist_dir: Some(web_dist.clone()),
        ..ApiConfig::default()
    })
    .await;

    let (status, headers, raw) = app.raw(Method::GET, "/auth/me").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert!(raw.contains("quartermaster-web-shell"));

    let _ = std::fs::remove_dir_all(web_dist);
}

#[tokio::test]
async fn web_brand_assets_are_served_from_web_dist() {
    let web_dist = test_web_dist();
    let app = TestApp::start(ApiConfig {
        web_dist_dir: Some(web_dist.clone()),
        ..ApiConfig::default()
    })
    .await;

    let (status, headers, raw) = app.raw(Method::GET, "/brand/quartermaster-mark.svg").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "image/svg+xml");
    assert!(raw.contains("Quartermaster mark"));

    let _ = std::fs::remove_dir_all(web_dist);
}

#[tokio::test]
async fn missing_web_dist_does_not_break_api_routes() {
    let missing = std::env::temp_dir().join(format!("qm-missing-web-{}", Uuid::now_v7()));
    let app = TestApp::start(ApiConfig {
        web_dist_dir: Some(missing),
        ..ApiConfig::default()
    })
    .await;

    let (status, _, raw) = app.raw(Method::GET, "/healthz").await;

    assert_eq!(status, StatusCode::OK);
    assert!(raw.contains("ok"));
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
        vec!["/join", "/join*", "/batches/*"]
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

fn test_web_dist() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("qm-web-{}", Uuid::now_v7()));
    std::fs::create_dir_all(dir.join("_app")).unwrap();
    std::fs::create_dir_all(dir.join("brand")).unwrap();
    std::fs::write(
        dir.join("brand").join("quartermaster-mark.svg"),
        r#"<svg xmlns="http://www.w3.org/2000/svg"><title>Quartermaster mark</title></svg>"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>Quartermaster</title><main>quartermaster-web-shell</main>",
    )
    .unwrap();
    std::fs::write(
        dir.join("join.html"),
        "<!doctype html><title>Quartermaster</title><main>quartermaster-web-shell</main>",
    )
    .unwrap();
    std::fs::write(
        dir.join("200.html"),
        "<!doctype html><title>Quartermaster</title><main>quartermaster-web-shell</main>",
    )
    .unwrap();
    dir
}
