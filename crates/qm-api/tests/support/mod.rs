use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use qm_api::{ApiConfig, AppState};
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
        let db = Database::connect(&temp_db_url()).await.unwrap();
        db.migrate().await.unwrap();
        let state = AppState {
            db: db.clone(),
            config: Arc::new(config),
            http: reqwest::Client::new(),
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
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if let Some(token) = bearer {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        let req = req
            .body(match body {
                Some(value) => Body::from(serde_json::to_vec(&value).unwrap()),
                None => Body::empty(),
            })
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, json)
    }

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

    pub async fn me(&self, bearer: &str) -> Value {
        self.send(Method::GET, "/auth/me", None, Some(bearer)).await.1
    }

    #[allow(dead_code)]
    pub async fn seed_household_admin(&self, username: &str) -> (Uuid, Uuid) {
        let household = qm_db::households::create(&self.db, "Home").await.unwrap();
        qm_db::locations::seed_defaults(&self.db, household.id).await.unwrap();
        let hash = qm_api::auth::hash_password("password123").unwrap();
        let user =
            qm_db::users::create(&self.db, username, Some(&format!("{username}@example.com")), &hash)
                .await
                .unwrap();
        qm_db::memberships::insert(&self.db, household.id, user.id, "admin")
            .await
            .unwrap();
        (household.id, user.id)
    }
}

fn temp_db_url() -> String {
    format!("sqlite:///tmp/qm-api-{}.db?mode=rwc", Uuid::now_v7())
}
