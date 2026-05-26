mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn suppliers_setup_catalog_mapping_and_order_lifecycle() {
    let app = TestApp::start(ApiConfig {
        supplier_credential_encryption_key: Some("test supplier key".into()),
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let pantry_id = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|location| location.kind == "pantry")
        .unwrap()
        .id;
    let product = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Rice",
        None,
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();

    let (status, capabilities) = app
        .send(
            Method::GET,
            "/api/v1/suppliers/capabilities",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(capabilities["suppliers"][0]["id"], "mock");

    let (status, account) = app
        .send(
            Method::POST,
            "/api/v1/suppliers/accounts",
            Some(json!({
                "supplier_id": "mock",
                "display_name": "Mock account",
                "status": "active",
                "config": { "mode": "test" }
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let account_id = account["id"].as_str().unwrap();

    let (status, secret) = app
        .send(
            Method::PUT,
            &format!("/api/v1/suppliers/accounts/{account_id}/secrets/api_token"),
            Some(json!({
                "secret_kind": "api_token",
                "value": "super-secret-token"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(secret["secret_kind"], "api_token");
    assert_ne!(secret["redacted_hint"], "super-secret-token");

    let (status, catalog) = app
        .send(
            Method::GET,
            "/api/v1/suppliers/catalog/search?q=rice",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalog["items"][0]["supplier_item_id"], "mock-rice-1kg");

    let (status, mapping) = app
        .send(
            Method::PUT,
            &format!("/api/v1/products/{}/supplier-mappings", product.id),
            Some(json!({
                "supplier_id": "mock",
                "supplier_item_id": "mock-rice-1kg",
                "confidence": "confirmed",
                "substitute_policy": { "allow_substitutes": false }
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mapping["confidence"], "confirmed");

    let (status, draft) = app
        .send(
            Method::POST,
            "/api/v1/suppliers/cart-drafts",
            Some(json!({
                "account_id": account_id,
                "supplier_id": "mock",
                "lines": [{
                    "product_id": product.id,
                    "supplier_item_id": "mock-rice-1kg",
                    "quantity": "1",
                    "unit": "piece"
                }]
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(draft["status"], "ready");
    let draft_id = draft["id"].as_str().unwrap();

    let (status, order) = app
        .send(
            Method::POST,
            &format!("/api/v1/suppliers/cart-drafts/{draft_id}/submit"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(order["status"], "submitted");
    let order_id = order["id"].as_str().unwrap();

    let (status, received) = app
        .send(
            Method::POST,
            &format!("/api/v1/suppliers/orders/{order_id}/receive"),
            Some(json!({
                "lines": [{
                    "product_id": product.id,
                    "location_id": pantry_id,
                    "quantity": "1000",
                    "unit": "g",
                    "expires_on": "2026-06-30",
                    "note": "received from mock supplier"
                }]
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(received["status"], "delivered");

    let batches = qm_db::stock::list(&app.db, household_id, &qm_db::stock::StockFilter::default())
        .await
        .unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].batch.quantity, "1000");
}

#[tokio::test]
async fn suppliers_secrets_require_server_encryption_key() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let account = app
        .send(
            Method::POST,
            "/api/v1/suppliers/accounts",
            Some(json!({
                "supplier_id": "mock",
                "display_name": "Mock account",
                "status": "active"
            })),
            Some(&alice),
        )
        .await
        .1;
    let account_id = account["id"].as_str().unwrap();

    let (status, body) = app
        .send(
            Method::PUT,
            &format!("/api/v1/suppliers/accounts/{account_id}/secrets/api_token"),
            Some(json!({
                "secret_kind": "api_token",
                "value": "super-secret-token"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "service_unavailable");
}
