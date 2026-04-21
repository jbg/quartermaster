mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::TestApp;
use uuid::Uuid;

#[tokio::test]
async fn product_stock_history_lifecycle_flows_through_api() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me["household_id"].as_str().unwrap()).unwrap();
    let pantry_id = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;

    let (status, product) = app
        .send(
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

    let (status, batch) = app
        .send(
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

    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/stock/{batch_id}"),
            Some(json!({ "quantity": "450" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["quantity"], "450");

    let (status, consumed) = app
        .send(
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
        app.send(Method::DELETE, &format!("/stock/{batch_id}"), None, Some(&alice))
            .await
            .0,
        StatusCode::NO_CONTENT
    );

    let (status, restored) = app
        .send(
            Method::POST,
            &format!("/stock/{batch_id}/restore"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["quantity"], "250");

    let (status, history) = app
        .send(Method::GET, "/stock/events?limit=20", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = history["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["event_type"] == "add"));
    assert!(items.iter().any(|item| item["event_type"] == "adjust"));
    assert!(items.iter().any(|item| item["event_type"] == "consume"));
    assert!(items.iter().any(|item| item["event_type"] == "discard"));
    assert!(items.iter().any(|item| item["event_type"] == "restore"));
}

#[tokio::test]
async fn request_ids_are_generated_and_propagated() {
    let app = TestApp::start(ApiConfig::default()).await;

    let (status, headers, _) = app
        .send_with_request_id(Method::GET, "/healthz", None, None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let generated = headers.get("x-request-id").unwrap().to_str().unwrap();
    assert!(!generated.is_empty());

    let (status, headers, _) = app
        .send_with_request_id(
            Method::GET,
            "/healthz",
            None,
            None,
            Some("test-request-id"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("x-request-id").unwrap(),
        "test-request-id"
    );
}
