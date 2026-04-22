mod support;

use axum::http::{HeaderMap, Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::TestApp;

#[tokio::test]
async fn login_is_rate_limited_per_client_ip() {
    let app = TestApp::start(ApiConfig {
        trust_proxy_headers: true,
        rate_limit_auth: qm_api::RateLimitConfig {
            requests_per_minute: 60,
            burst: 1,
        },
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);

    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "198.51.100.10".parse().unwrap());

    let first = app
        .send_with_headers(
            Method::POST,
            "/auth/login",
            Some(json!({"username": "alice", "password": "password123"})),
            None,
            headers.clone(),
        )
        .await;
    let second = app
        .send_with_headers(
            Method::POST,
            "/auth/login",
            Some(json!({"username": "alice", "password": "password123"})),
            None,
            headers,
        )
        .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(second.1["code"], "rate_limited");
}

#[tokio::test]
async fn stock_history_is_rate_limited_per_client_ip() {
    let app = TestApp::start(ApiConfig {
        trust_proxy_headers: true,
        rate_limit_history: qm_api::RateLimitConfig {
            requests_per_minute: 60,
            burst: 1,
        },
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "198.51.100.20".parse().unwrap());

    let first = app
        .send_with_headers(
            Method::GET,
            "/stock/events",
            None,
            Some(&alice),
            headers.clone(),
        )
        .await;
    let second = app
        .send_with_headers(Method::GET, "/stock/events", None, Some(&alice), headers)
        .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::TOO_MANY_REQUESTS);
}
