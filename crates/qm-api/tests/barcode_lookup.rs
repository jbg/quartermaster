mod support;

use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use qm_api::ApiConfig;
use serde_json::{json, Value};
use support::TestApp;
use tokio::sync::Mutex;

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
            "/products/by-barcode/1111111111111",
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
            "/products/by-barcode/2222222222222",
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
            "/products/by-barcode/3333333333333",
            None,
            Some(&alice),
        )
        .await;
    let second = app
        .send(
            axum::http::Method::GET,
            "/products/by-barcode/3333333333333",
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

struct MockOffServer {
    addr: std::net::SocketAddr,
    hits: Arc<Mutex<HashMap<String, usize>>>,
}

impl MockOffServer {
    async fn start() -> Self {
        let hits = Arc::new(Mutex::new(HashMap::new()));
        let state = MockOffState { hits: hits.clone() };
        let app = Router::new()
            .route("/api/v2/product/{barcode}", get(mock_off_product))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { addr, hits }
    }

    fn base_url(&self) -> String {
        format!("http://{}/api/v2/product", self.addr)
    }

    async fn hit_count(&self, barcode: &str) -> usize {
        self.hits
            .lock()
            .await
            .get(barcode)
            .copied()
            .unwrap_or_default()
    }
}

#[derive(Clone)]
struct MockOffState {
    hits: Arc<Mutex<HashMap<String, usize>>>,
}

async fn mock_off_product(
    State(state): State<MockOffState>,
    Path(barcode): Path<String>,
) -> impl IntoResponse {
    let barcode = barcode.trim_end_matches(".json").to_owned();
    let attempt = {
        let mut hits = state.hits.lock().await;
        let count = hits.entry(barcode.clone()).or_insert(0);
        *count += 1;
        *count
    };

    match barcode.as_str() {
        "1111111111111" if attempt < 3 => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": 0, "status_verbose": "temporary"})),
        )
            .into_response(),
        "1111111111111" => (
            StatusCode::OK,
            Json(json!({
                "code": barcode,
                "status": 1,
                "product": {
                    "product_name": "Retry Beans",
                    "brands": "Acme",
                    "image_front_url": Value::Null,
                    "quantity": "500 g",
                }
            })),
        )
            .into_response(),
        "2222222222222" => (
            StatusCode::NOT_FOUND,
            Json(json!({"status": 0, "status_verbose": "not found"})),
        )
            .into_response(),
        "3333333333333" => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": 0, "status_verbose": "down"})),
        )
            .into_response(),
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({"status": 0, "status_verbose": "not found"})),
        )
            .into_response(),
    }
}
