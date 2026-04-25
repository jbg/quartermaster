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
            "/api/v1/products",
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
            "/api/v1/stock",
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
            &format!("/api/v1/stock/{batch_id}"),
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
            &format!("/api/v1/products/{product_id}"),
            None,
            Some(&alice)
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let (status, history) = app
        .send(
            Method::GET,
            "/api/v1/stock/events?limit=20",
            None,
            Some(&alice),
        )
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
            "/api/v1/stock",
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
            &format!("/api/v1/stock/{batch_id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(batch_detail["code"], "not_found");
}

#[tokio::test]
async fn product_catalogue_lists_visible_products_and_deleted_when_requested() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let alice_household_id =
        Uuid::parse_str(me_current_household_id(&app.me(&alice).await).unwrap()).unwrap();
    let bob_household_id = qm_db::households::create(&app.db, "Bob home", "UTC")
        .await
        .unwrap()
        .id;

    let active = qm_db::products::create_manual(
        &app.db,
        alice_household_id,
        "Catalogue Rice",
        Some("House"),
        "mass",
        Some("kg"),
        None,
        None,
    )
    .await
    .unwrap();
    let deleted = qm_db::products::create_manual(
        &app.db,
        alice_household_id,
        "Catalogue Retired",
        None,
        "count",
        None,
        None,
        None,
    )
    .await
    .unwrap();
    qm_db::products::soft_delete(&app.db, deleted.id)
        .await
        .unwrap();
    qm_db::products::create_manual(
        &app.db,
        bob_household_id,
        "Catalogue Bob Private",
        None,
        "count",
        None,
        None,
        None,
    )
    .await
    .unwrap();
    qm_db::products::upsert_from_off(
        &app.db,
        "5449000000996",
        "Catalogue Cola",
        Some("Open"),
        "volume",
        Some("ml"),
        None,
    )
    .await
    .unwrap();

    let (status, catalogue) = app
        .send(Method::GET, "/api/v1/products", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<_> = catalogue["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Catalogue Cola", "Catalogue Rice"]);

    let (status, catalogue) = app
        .send(
            Method::GET,
            "/api/v1/products?include_deleted=true",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = catalogue["items"].as_array().unwrap();
    assert!(items
        .iter()
        .any(|item| item["id"].as_str() == Some(&active.id.to_string())));
    let deleted_item = items
        .iter()
        .find(|item| item["id"].as_str() == Some(&deleted.id.to_string()))
        .unwrap();
    assert!(deleted_item["deleted_at"].as_str().is_some());
    assert!(!items
        .iter()
        .any(|item| item["name"].as_str() == Some("Catalogue Bob Private")));

    let (status, filtered) = app
        .send(
            Method::GET,
            "/api/v1/products?q=cola&include_deleted=true",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let filtered_items = filtered["items"].as_array().unwrap();
    assert_eq!(filtered_items.len(), 1);
    assert_eq!(filtered_items[0]["name"], "Catalogue Cola");
}

#[tokio::test]
async fn product_patch_uses_json_patch_replace_and_remove() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let (status, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Patch Flour",
                "brand": "Mill",
                "family": "mass",
                "preferred_unit": "g",
                "barcode": null,
                "image_url": "https://example.com/flour.png",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id = product["id"].as_str().unwrap();

    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/products/{product_id}"),
            Some(json!([
                { "op": "replace", "path": "/name", "value": "Patch Bread Flour" },
                { "op": "remove", "path": "/brand" },
                { "op": "replace", "path": "/preferred_unit", "value": "kg" },
            ])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Patch Bread Flour");
    assert!(updated["brand"].is_null());
    assert_eq!(updated["preferred_unit"], "kg");
    assert_eq!(updated["image_url"], "https://example.com/flour.png");

    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/products/{product_id}"),
            Some(json!([
                { "op": "replace", "path": "/brand", "value": "New Mill" },
                { "op": "remove", "path": "/image_url" },
            ])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["brand"], "New Mill");
    assert!(updated["image_url"].is_null());

    for body in [
        json!([{ "op": "replace", "path": "/brand" }]),
        json!([{ "op": "remove", "path": "/name" }]),
        json!([{ "op": "add", "path": "/brand", "value": "x" }]),
    ] {
        let (status, _) = app
            .send(
                Method::PATCH,
                &format!("/api/v1/products/{product_id}"),
                Some(body),
                Some(&alice),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
