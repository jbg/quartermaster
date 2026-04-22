use std::{net::SocketAddr, str::FromStr, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Method, Request, StatusCode},
    Router,
};
use qm_api::{ApiConfig, AppState};
use qm_db::test_support;
use qm_db::Database;
use serde_json::{json, Value};
use tower::util::ServiceExt;
use uuid::Uuid;

pub struct TestApp {
    app: Router,
    pub db: Database,
}

impl TestApp {
    pub async fn start(config: ApiConfig) -> Self {
        Self::start_with_http(config, reqwest::Client::new()).await
    }

    pub async fn start_with_http(config: ApiConfig, http: reqwest::Client) -> Self {
        let db = test_db().await;
        let config = Arc::new(config);
        let state = AppState {
            db: db.clone(),
            off_breaker: Arc::new(qm_api::openfoodfacts::OffCircuitBreaker::default()),
            rate_limiters: Arc::new(qm_api::rate_limit::RateLimiters::new(&config)),
            config,
            http,
        };
        Self {
            app: qm_api::router(state),
            db,
        }
    }

    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
    ) -> (StatusCode, Value) {
        self.send_with_headers(method, path, body, bearer, HeaderMap::new())
            .await
    }

    pub async fn send_with_headers(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        extra_headers: HeaderMap,
    ) -> (StatusCode, Value) {
        let (status, _, json) = self
            .send_with_peer_and_request_id_and_headers(
                method,
                path,
                body,
                bearer,
                None,
                extra_headers,
                SocketAddr::from_str("127.0.0.1:3000").unwrap(),
            )
            .await;
        (status, json)
    }

    #[allow(dead_code)]
    pub async fn send_with_request_id(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        request_id: Option<&str>,
    ) -> (StatusCode, HeaderMap, Value) {
        self.send_with_request_id_and_headers(
            method,
            path,
            body,
            bearer,
            request_id,
            HeaderMap::new(),
        )
        .await
    }

    pub async fn send_with_request_id_and_headers(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        request_id: Option<&str>,
        extra_headers: HeaderMap,
    ) -> (StatusCode, HeaderMap, Value) {
        self.send_with_peer_and_request_id_and_headers(
            method,
            path,
            body,
            bearer,
            request_id,
            extra_headers,
            SocketAddr::from_str("127.0.0.1:3000").unwrap(),
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn send_with_peer_and_request_id_and_headers(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        bearer: Option<&str>,
        request_id: Option<&str>,
        extra_headers: HeaderMap,
        peer_addr: SocketAddr,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if let Some(token) = bearer {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        if let Some(request_id) = request_id {
            req = req.header("x-request-id", request_id);
        }
        let mut req = req
            .body(match body {
                Some(value) => Body::from(serde_json::to_vec(&value).unwrap()),
                None => Body::empty(),
            })
            .unwrap();
        req.headers_mut().extend(extra_headers);
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(peer_addr));
        let res = self.app.clone().oneshot(req).await.unwrap();
        let headers = res.headers().clone();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, headers, json)
    }

    #[allow(dead_code)]
    pub async fn raw(&self, method: Method, path: &str) -> (StatusCode, HeaderMap, String) {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            SocketAddr::from_str("127.0.0.1:3000").unwrap(),
        ));
        let res = self.app.clone().oneshot(req).await.unwrap();
        let headers = res.headers().clone();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[allow(dead_code)]
    pub async fn register(&self, username: &str, invite_code: Option<&str>) -> (StatusCode, Value) {
        self.send(
            Method::POST,
            "/auth/register",
            Some(json!({
                "username": username,
                "password": "password123",
                "email": format!("{username}@example.com"),
                "invite_code": invite_code,
            })),
            None,
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn login(&self, username: &str) -> String {
        let (status, body) = self
            .send(
                Method::POST,
                "/auth/login",
                Some(json!({
                    "username": username,
                    "password": "password123",
                })),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        body["access_token"].as_str().unwrap().to_owned()
    }

    #[allow(dead_code)]
    pub async fn me(&self, bearer: &str) -> Value {
        self.send(Method::GET, "/auth/me", None, Some(bearer))
            .await
            .1
    }

    #[allow(dead_code)]
    pub async fn seed_household_admin(&self, username: &str) -> (Uuid, Uuid) {
        let household = qm_db::households::create(&self.db, "Home", "UTC")
            .await
            .unwrap();
        qm_db::locations::seed_defaults(&self.db, household.id)
            .await
            .unwrap();
        let hash = qm_api::auth::hash_password("password123").unwrap();
        let user = qm_db::users::create(
            &self.db,
            username,
            Some(&format!("{username}@example.com")),
            &hash,
        )
        .await
        .unwrap();
        qm_db::memberships::insert(&self.db, household.id, user.id, "admin")
            .await
            .unwrap();
        (household.id, user.id)
    }

    #[allow(dead_code)]
    pub async fn seed_user_without_household(&self, username: &str) -> Uuid {
        let hash = qm_api::auth::hash_password("password123").unwrap();
        qm_db::users::create(
            &self.db,
            username,
            Some(&format!("{username}@example.com")),
            &hash,
        )
        .await
        .unwrap()
        .id
    }
}

async fn test_db() -> Database {
    test_support::default_test_database().await.into_db()
}
