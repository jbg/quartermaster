mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn deleted_manual_products_stay_visible_in_history_but_reject_new_stock() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let pantry = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;

    let (_, product) = app
        .send(
            Method::POST,
            "/products",
            Some(json!({
                "name": "Spice Mix",
                "brand": null,
                "family": "mass",
                "preferred_unit": "g",
                "barcode": null,
                "image_url": null,
            })),
            Some(&alice),
        )
        .await;
    let product_id = product["id"].as_str().unwrap();
    let (_, batch) = app
        .send(
            Method::POST,
            "/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry,
                "quantity": "80",
                "unit": "g",
            })),
            Some(&alice),
        )
        .await;
    let batch_id = batch["id"].as_str().unwrap();
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/stock/{batch_id}"),
            None,
            Some(&alice)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/products/{product_id}"),
            None,
            Some(&alice)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let (status, history) = app
        .send(Method::GET, "/stock/events?limit=20", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(history["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["product"]["id"] == product_id));

    let (status, _) = app
        .send(
            Method::POST,
            "/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry,
                "quantity": "10",
                "unit": "g",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, batch_detail) = app
        .send(
            Method::GET,
            &format!("/stock/{batch_id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(batch_detail["code"], "not_found");
}
