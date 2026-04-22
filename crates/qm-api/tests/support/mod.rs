use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Method, Request, StatusCode},
    Router,
};
use qm_api::{ApiConfig, AppState};
use qm_db::Database;
use serde_json::{json, Value};
use sqlx::Connection;
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
            .send_with_request_id_and_headers(method, path, body, bearer, None, extra_headers)
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
        let req = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();
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
        let household = qm_db::households::create(&self.db, "Home").await.unwrap();
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
    if postgres_test_enabled() {
        postgres_db().await
    } else {
        let db = Database::connect(&temp_sqlite_db_url()).await.unwrap();
        db.migrate().await.unwrap();
        db
    }
}

async fn postgres_db() -> Database {
    let admin_url = std::env::var("QM_POSTGRES_TEST_URL")
        .expect("QM_POSTGRES_TEST_URL must be set when Postgres tests are enabled");
    let db_name = format!("qm_api_test_{}", Uuid::now_v7().simple());
    let pool = reqwest::Url::parse(&admin_url).expect("valid Postgres URL");
    let admin_db_name = pool
        .path_segments()
        .and_then(|segments| segments.last())
        .filter(|segment| !segment.is_empty())
        .expect("postgres url should include database")
        .to_owned();

    let mut admin = sqlx::postgres::PgConnection::connect(&admin_url)
        .await
        .expect("connect postgres admin");
    sqlx::query(format!(r#"CREATE DATABASE "{db_name}""#).as_str())
        .execute(&mut admin)
        .await
        .expect("create isolated postgres database");
    admin.close().await.expect("close postgres admin");

    let db_url = admin_url.replacen(&format!("/{admin_db_name}"), &format!("/{db_name}"), 1);
    let db = Database::connect(&db_url).await.unwrap();
    db.migrate().await.unwrap();
    db
}

fn postgres_test_enabled() -> bool {
    matches!(
        std::env::var("QM_REQUIRE_POSTGRES_TESTS").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    ) || std::env::var("QM_POSTGRES_TEST_URL").is_ok()
}

fn temp_sqlite_db_url() -> String {
    format!("sqlite:///tmp/qm-api-{}.db?mode=rwc", Uuid::now_v7())
}
