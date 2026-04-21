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

fn temp_db_url() -> String {
    format!("sqlite:///tmp/qm-api-lifecycle-{}.db?mode=rwc", Uuid::now_v7())
}

async fn start_app(config: ApiConfig) -> (Router, Database) {
    let db = Database::connect(&temp_db_url()).await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        http: reqwest::Client::new(),
    };
    (qm_api::router(state), db)
}

async fn send(
    app: &Router,
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
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

async fn register(app: &Router, username: &str) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        "/auth/register",
        Some(json!({
            "username": username,
            "password": "password123",
            "email": format!("{username}@example.com"),
        })),
        None,
    )
    .await
}

async fn login(app: &Router, username: &str) -> String {
    let (status, body) = send(
        app,
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

#[tokio::test]
async fn product_stock_history_lifecycle_flows_through_api() {
    let (app, db) = start_app(ApiConfig::default()).await;
    assert_eq!(register(&app, "alice").await.0, StatusCode::CREATED);
    let alice = login(&app, "alice").await;

    let me = send(&app, Method::GET, "/auth/me", None, Some(&alice)).await.1;
    let household_id = Uuid::parse_str(me["household_id"].as_str().unwrap()).unwrap();
    let pantry_id = qm_db::locations::list_for_household(&db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;

    let (status, product) = send(
        &app,
        Method::POST,
        "/products",
        Some(json!({
            "name": "Rice",
            "brand": "Acme",
            "family": "mass",
            "preferred_unit": "g",
            "barcode": null,
            "image_url": null,
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id = product["id"].as_str().unwrap();

    let (status, batch) = send(
        &app,
        Method::POST,
        "/stock",
        Some(json!({
            "product_id": product_id,
            "location_id": pantry_id,
            "quantity": "500",
            "unit": "g",
            "expires_on": "2026-06-01",
            "opened_on": null,
            "note": "bag",
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let batch_id = batch["id"].as_str().unwrap();

    let (status, updated) = send(
        &app,
        Method::PATCH,
        &format!("/stock/{batch_id}"),
        Some(json!({ "quantity": "450" })),
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["quantity"], "450");

    let (status, consumed) = send(
        &app,
        Method::POST,
        "/stock/consume",
        Some(json!({
            "product_id": product_id,
            "location_id": pantry_id,
            "quantity": "200",
            "unit": "g",
        })),
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(consumed["consumed"].as_array().unwrap().len(), 1);

    assert_eq!(
        send(&app, Method::DELETE, &format!("/stock/{batch_id}"), None, Some(&alice))
            .await
            .0,
        StatusCode::NO_CONTENT
    );

    let (status, restored) = send(
        &app,
        Method::POST,
        &format!("/stock/{batch_id}/restore"),
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["quantity"], "250");

    let (status, history) = send(&app, Method::GET, "/stock/events?limit=20", None, Some(&alice)).await;
    assert_eq!(status, StatusCode::OK);
    let items = history["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["event_type"] == "add"));
    assert!(items.iter().any(|item| item["event_type"] == "adjust"));
    assert!(items.iter().any(|item| item["event_type"] == "consume"));
    assert!(items.iter().any(|item| item["event_type"] == "discard"));
    assert!(items.iter().any(|item| item["event_type"] == "restore"));
}
