mod support;

use std::time::Duration;

use axum::http::StatusCode;
use qm_api::ApiConfig;
use support::off_http::MockOffServer;
use support::TestApp;

#[tokio::test]
async fn barcode_lookup_retries_transient_off_failures_then_succeeds() {
    let mock = MockOffServer::start().await;
    let app = TestApp::start(ApiConfig {
        off_api_base_url: mock.base_url(),
        off_max_retries: 2,
        off_retry_base_delay: Duration::from_millis(5),
        off_timeout: Duration::from_millis(50),
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let (status, body) = app
        .send(
            axum::http::Method::GET,
            "/api/v1/products/by-barcode/1111111111111",
            None,
            Some(&alice),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["source"], "openfoodfacts");
    assert_eq!(mock.hit_count("1111111111111").await, 3);
}

#[tokio::test]
async fn barcode_lookup_404_writes_negative_cache_entry() {
    let mock = MockOffServer::start().await;
    let app = TestApp::start(ApiConfig {
        off_api_base_url: mock.base_url(),
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let (status, _) = app
        .send(
            axum::http::Method::GET,
            "/api/v1/products/by-barcode/2222222222222",
            None,
            Some(&alice),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    let cached = qm_db::barcode_cache::get(&app.db, "2222222222222")
        .await
        .unwrap()
        .unwrap();
    assert!(cached.miss);
}

#[tokio::test]
async fn breaker_open_failures_do_not_write_cache_misses_and_fail_fast() {
    let mock = MockOffServer::start().await;
    let app = TestApp::start(ApiConfig {
        off_api_base_url: mock.base_url(),
        off_max_retries: 0,
        off_retry_base_delay: Duration::from_millis(5),
        off_timeout: Duration::from_millis(20),
        off_circuit_breaker_failure_threshold: 1,
        off_circuit_breaker_open_for: Duration::from_secs(60),
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let first = app
        .send(
            axum::http::Method::GET,
            "/api/v1/products/by-barcode/3333333333333",
            None,
            Some(&alice),
        )
        .await;
    let second = app
        .send(
            axum::http::Method::GET,
            "/api/v1/products/by-barcode/3333333333333",
            None,
            Some(&alice),
        )
        .await;

    assert_eq!(first.0, StatusCode::BAD_GATEWAY);
    assert_eq!(second.0, StatusCode::BAD_GATEWAY);
    assert_eq!(mock.hit_count("3333333333333").await, 1);
    assert!(qm_db::barcode_cache::get(&app.db, "3333333333333")
        .await
        .unwrap()
        .is_none());
}
