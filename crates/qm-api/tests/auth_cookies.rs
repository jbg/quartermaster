mod support;

use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::json;
use support::TestApp;

#[tokio::test]
async fn register_login_and_refresh_set_browser_cookies() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;

    let (status, headers, register_body) = app
        .send_with_request_id(
            Method::POST,
            "/api/v1/onboarding/create-household",
            Some(json!({
                "username": "alice",
                "password": "password123",
                "household_name": "Alice's Household",
                "timezone": "UTC",
            })),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(register_body["access_token"].as_str().is_some());
    assert_cookie_names(&headers);

    let (status, headers, login_body) = app
        .send_with_request_id(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({
                "username": "alice",
                "password": "password123",
            })),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(login_body["refresh_token"].as_str().is_some());
    assert_cookie_names(&headers);

    let cookie = cookie_header(&headers);
    let csrf = cookie_value(&headers, qm_api::auth::CSRF_COOKIE);
    let mut cookie_headers = HeaderMap::new();
    cookie_headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    let (status, _) = app
        .send_with_headers(
            Method::POST,
            "/api/v1/auth/refresh",
            Some(json!({})),
            None,
            cookie_headers.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    cookie_headers.insert(
        HeaderName::from_static(qm_api::auth::CSRF_HEADER),
        HeaderValue::from_str(&csrf).unwrap(),
    );
    let (status, refresh_headers, refresh_body) = app
        .send_with_request_id_and_headers(
            Method::POST,
            "/api/v1/auth/refresh",
            Some(json!({})),
            None,
            None,
            cookie_headers,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(refresh_body["access_token"].as_str().is_some());
    assert_cookie_names(&refresh_headers);
}

#[tokio::test]
async fn cookie_auth_requires_csrf_for_unsafe_requests_and_keeps_bearer_working() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (home, user) = app.seed_household_admin("alice").await;
    let cabin = qm_db::households::create(&app.db, "Cabin", "UTC")
        .await
        .unwrap();
    qm_db::memberships::insert(&app.db, cabin.id, user, "admin")
        .await
        .unwrap();

    let (status, headers, body) = app
        .send_with_request_id(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({
                "username": "alice",
                "password": "password123",
            })),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let bearer = body["access_token"].as_str().unwrap().to_owned();
    let cookie = cookie_header(&headers);
    let csrf = cookie_value(&headers, qm_api::auth::CSRF_COOKIE);

    let mut cookie_headers = HeaderMap::new();
    cookie_headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
    let (status, _) = app
        .send_with_headers(
            Method::GET,
            "/api/v1/auth/me",
            None,
            None,
            cookie_headers.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .send_with_headers(
            Method::POST,
            "/api/v1/auth/switch-household",
            Some(json!({ "household_id": cabin.id })),
            None,
            cookie_headers.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    cookie_headers.insert(
        HeaderName::from_static(qm_api::auth::CSRF_HEADER),
        HeaderValue::from_str(&csrf).unwrap(),
    );
    let (status, _) = app
        .send_with_headers(
            Method::POST,
            "/api/v1/auth/switch-household",
            Some(json!({ "household_id": cabin.id })),
            None,
            cookie_headers,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/auth/switch-household",
            Some(json!({ "household_id": home })),
            Some(&bearer),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn logout_clears_browser_cookies() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    app.seed_household_admin("alice").await;
    let (status, headers, _) = app
        .send_with_request_id(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({
                "username": "alice",
                "password": "password123",
            })),
            None,
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let mut cookie_headers = HeaderMap::new();
    cookie_headers.insert(
        header::COOKIE,
        HeaderValue::from_str(&cookie_header(&headers)).unwrap(),
    );
    cookie_headers.insert(
        HeaderName::from_static(qm_api::auth::CSRF_HEADER),
        HeaderValue::from_str(&cookie_value(&headers, qm_api::auth::CSRF_COOKIE)).unwrap(),
    );
    let (status, logout_headers, body) = app
        .send_with_request_id_and_headers(
            Method::POST,
            "/api/v1/auth/logout",
            None,
            None,
            None,
            cookie_headers,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_null());
    for value in set_cookies(&logout_headers) {
        assert!(value.contains("Max-Age=0"));
    }
}

#[tokio::test]
async fn credentialed_cors_allows_configured_web_auth_origin() {
    let app = TestApp::start(ApiConfig {
        web_auth_allowed_origins: vec!["https://web.example.com".into()],
        ..ApiConfig::default()
    })
    .await;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_static("https://web.example.com"),
    );
    headers.insert(
        header::ACCESS_CONTROL_REQUEST_METHOD,
        HeaderValue::from_static("POST"),
    );
    headers.insert(
        header::ACCESS_CONTROL_REQUEST_HEADERS,
        HeaderValue::from_static("content-type,x-qm-csrf"),
    );

    let (status, headers, _) = app
        .raw_with_headers(Method::OPTIONS, "/api/v1/auth/me", headers)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "https://web.example.com"
    );
    assert_eq!(
        headers
            .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .unwrap(),
        "true"
    );
}

fn set_cookies(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap().to_owned())
        .collect()
}

fn assert_cookie_names(headers: &HeaderMap) {
    let cookies = set_cookies(headers);
    assert!(cookies.iter().any(|value| value.starts_with("qm_access=")));
    assert!(cookies.iter().any(|value| value.starts_with("qm_refresh=")));
    assert!(cookies.iter().any(|value| value.starts_with("qm_csrf=")));
    assert!(cookies
        .iter()
        .filter(|value| value.starts_with("qm_access=") || value.starts_with("qm_refresh="))
        .all(|value| value.contains("HttpOnly") && value.contains("Path=/")));
    assert!(cookies
        .iter()
        .find(|value| value.starts_with("qm_csrf="))
        .is_some_and(|value| !value.contains("HttpOnly") && value.contains("Path=/")));
}

#[tokio::test]
async fn public_base_url_does_not_force_secure_browser_cookies() {
    let app = TestApp::start(ApiConfig {
        public_base_url: Some("https://quartermaster.example.com".into()),
        ..ApiConfig::default()
    })
    .await;
    app.seed_household_admin("alice").await;

    let (status, headers, _) = app
        .send_with_request_id(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({
                "username": "alice",
                "password": "password123",
            })),
            None,
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(set_cookies(&headers)
        .iter()
        .all(|value| !value.contains("Secure")));
}

#[tokio::test]
async fn cross_origin_web_auth_uses_secure_browser_cookies() {
    let app = TestApp::start(ApiConfig {
        web_auth_allowed_origins: vec!["https://web.example.com".into()],
        ..ApiConfig::default()
    })
    .await;
    app.seed_household_admin("alice").await;

    let (status, headers, _) = app
        .send_with_request_id(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({
                "username": "alice",
                "password": "password123",
            })),
            None,
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(set_cookies(&headers)
        .iter()
        .all(|value| value.contains("Secure")));
}

fn cookie_header(headers: &HeaderMap) -> String {
    set_cookies(headers)
        .into_iter()
        .map(|cookie| cookie.split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>()
        .join("; ")
}

fn cookie_value(headers: &HeaderMap, name: &str) -> String {
    set_cookies(headers)
        .into_iter()
        .find_map(|cookie| {
            let first = cookie.split(';').next()?;
            let (cookie_name, value) = first.split_once('=')?;
            (cookie_name == name).then(|| value.to_owned())
        })
        .unwrap()
}
