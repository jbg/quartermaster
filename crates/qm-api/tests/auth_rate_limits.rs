mod support;

use axum::http::{HeaderMap, Method, StatusCode};
use qm_api::{rate_limit::ClientIpMode, ApiConfig};
use serde_json::json;
use std::{net::SocketAddr, str::FromStr};
use support::TestApp;

#[tokio::test]
async fn login_is_rate_limited_per_client_ip() {
    let app = TestApp::start(ApiConfig {
        rate_limit_client_ip_mode: ClientIpMode::XForwardedFor,
        rate_limit_trusted_proxy_cidrs: vec![qm_api::rate_limit::TrustedProxyNet::from_str(
            "127.0.0.0/8",
        )
        .unwrap()],
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
        rate_limit_client_ip_mode: ClientIpMode::XForwardedFor,
        rate_limit_trusted_proxy_cidrs: vec![qm_api::rate_limit::TrustedProxyNet::from_str(
            "127.0.0.0/8",
        )
        .unwrap()],
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

#[tokio::test]
async fn socket_mode_ignores_forwarded_headers() {
    let app = TestApp::start(ApiConfig {
        rate_limit_client_ip_mode: ClientIpMode::Socket,
        rate_limit_auth: qm_api::RateLimitConfig {
            requests_per_minute: 60,
            burst: 1,
        },
        ..ApiConfig::default()
    })
    .await;
    app.seed_user_without_household("alice").await;

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
}

#[tokio::test]
async fn forwarded_mode_falls_back_when_header_is_blank() {
    let app = TestApp::start(ApiConfig {
        rate_limit_client_ip_mode: ClientIpMode::XForwardedFor,
        rate_limit_trusted_proxy_cidrs: vec![qm_api::rate_limit::TrustedProxyNet::from_str(
            "127.0.0.0/8",
        )
        .unwrap()],
        rate_limit_auth: qm_api::RateLimitConfig {
            requests_per_minute: 60,
            burst: 1,
        },
        ..ApiConfig::default()
    })
    .await;
    app.seed_user_without_household("alice").await;

    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", " , ".parse().unwrap());

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
}

#[tokio::test]
async fn forwarded_mode_ignores_header_from_untrusted_peer() {
    let app = TestApp::start(ApiConfig {
        rate_limit_client_ip_mode: ClientIpMode::XForwardedFor,
        rate_limit_trusted_proxy_cidrs: vec![qm_api::rate_limit::TrustedProxyNet::from_str(
            "127.0.0.0/8",
        )
        .unwrap()],
        rate_limit_auth: qm_api::RateLimitConfig {
            requests_per_minute: 60,
            burst: 1,
        },
        ..ApiConfig::default()
    })
    .await;
    app.seed_user_without_household("alice").await;

    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "198.51.100.30".parse().unwrap());
    let peer_addr = SocketAddr::from_str("10.0.0.2:3000").unwrap();

    let first = app
        .send_with_peer_and_request_id_and_headers(
            Method::POST,
            "/auth/login",
            Some(json!({"username": "alice", "password": "password123"})),
            None,
            None,
            headers.clone(),
            peer_addr,
        )
        .await;
    let second = app
        .send_with_peer_and_request_id_and_headers(
            Method::POST,
            "/auth/login",
            Some(json!({"username": "alice", "password": "password123"})),
            None,
            None,
            headers,
            peer_addr,
        )
        .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::TOO_MANY_REQUESTS);
}
