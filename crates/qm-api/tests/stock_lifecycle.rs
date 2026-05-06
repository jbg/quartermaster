mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn product_stock_history_lifecycle_flows_through_api() {
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
        .unwrap();
    let pantry_id = pantry.id;

    let (status, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
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
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry_id,
                "quantity": "500",
                "unit": "g",
                "produced_on": "2026-05-20",
                "expires_on": "2026-06-01",
                "opened_on": null,
                "note": "bag",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let batch_id = batch["id"].as_str().unwrap();
    assert_eq!(batch["location_id"], pantry_id.to_string());
    assert_eq!(batch["location_name"].as_str().unwrap(), pantry.name);
    assert_eq!(batch["produced_on"], "2026-05-20");
    assert!(batch["depleted_at"].is_null());

    let (status, listed) = app
        .send(
            Method::GET,
            "/api/v1/stock?include_depleted=true",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        listed["items"][0]["location_name"].as_str().unwrap(),
        pantry.name
    );
    assert!(listed["items"][0]["depleted_at"].is_null());

    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/stock/{batch_id}"),
            Some(json!([{ "op": "replace", "path": "/quantity", "value": "450" }])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["quantity"], "450");

    let (status, consumed) = app
        .send(
            Method::POST,
            "/api/v1/stock/consume",
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

    let (status, depleted) = app
        .send(
            Method::GET,
            &format!("/api/v1/stock/{batch_id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(depleted["location_name"].as_str().unwrap(), pantry.name);
    assert!(depleted["depleted_at"].as_str().is_some());

    let (status, restored) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/restore"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["quantity"], "250");
    assert_eq!(restored["location_name"].as_str().unwrap(), pantry.name);
    assert!(restored["depleted_at"].is_null());

    let (status, history) = app
        .send(
            Method::GET,
            "/api/v1/stock/events?limit=20",
            None,
            Some(&alice),
        )
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
async fn consume_and_store_depletes_source_and_creates_open_remainder() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let locations = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap();
    let pantry = locations.iter().find(|l| l.kind == "pantry").unwrap().id;
    let fridge = locations.iter().find(|l| l.kind == "fridge").unwrap().id;

    let (status, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Open Tomatoes",
                "brand": null,
                "family": "volume",
                "preferred_unit": "ml",
                "barcode": null,
                "image_url": null,
                "max_open_days": 3,
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id = product["id"].as_str().unwrap();
    let (_, batch) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry,
                "quantity": "400",
                "unit": "ml",
                "expires_on": "2026-12-31",
            })),
            Some(&alice),
        )
        .await;
    let batch_id = batch["id"].as_str().unwrap();

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/consume-and-store"),
            Some(json!({
                "used_quantity": "150",
                "remainder_location_id": fridge,
                "opened_on": "2026-05-01",
                "note": "leftover sauce",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["source"]["id"], batch_id);
    assert_eq!(body["source"]["quantity"], "0");
    assert!(body["source"]["depleted_at"].as_str().is_some());
    assert_eq!(body["remainder"]["location_id"], fridge.to_string());
    assert_eq!(body["remainder"]["quantity"], "250");
    assert_eq!(body["remainder"]["unit"], "ml");
    assert_eq!(body["remainder"]["opened_on"], "2026-05-01");
    assert_eq!(body["remainder"]["expires_on"], "2026-05-04");
    assert_eq!(body["remainder"]["note"], "leftover sauce");
    let request_id = body["consume_request_id"].as_str().unwrap();

    let (status, history) = app
        .send(
            Method::GET,
            &format!("/api/v1/stock/{batch_id}/events?limit=10"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let events = history["items"].as_array().unwrap();
    let consume_events: Vec<_> = events
        .iter()
        .filter(|event| event["consume_request_id"].as_str() == Some(request_id))
        .collect();
    assert_eq!(consume_events.len(), 2);
    assert!(consume_events
        .iter()
        .any(|event| event["quantity_delta"] == "-150"));
    assert!(consume_events
        .iter()
        .any(|event| event["quantity_delta"] == "-250"));
}

#[tokio::test]
async fn consume_and_store_accepts_explicit_remainder_expiry_without_product_rule() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let locations = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap();
    let pantry = locations.iter().find(|l| l.kind == "pantry").unwrap().id;
    let fridge = locations.iter().find(|l| l.kind == "fridge").unwrap().id;
    let (_, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Manual Expiry Leftovers",
                "brand": null,
                "family": "count",
                "preferred_unit": "piece",
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
                "quantity": "1",
                "unit": "piece",
            })),
            Some(&alice),
        )
        .await;
    let batch_id = batch["id"].as_str().unwrap();

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/consume-and-store"),
            Some(json!({
                "used_quantity": "0.25",
                "remainder_location_id": fridge,
                "opened_on": "2026-05-01",
                "remainder_expires_on": "2026-05-02",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["remainder"]["quantity"], "0.75");
    assert_eq!(body["remainder"]["expires_on"], "2026-05-02");
}

#[tokio::test]
async fn consume_and_store_rejects_missing_expiry_source_and_non_partial_quantities() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let locations = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap();
    let pantry = locations.iter().find(|l| l.kind == "pantry").unwrap().id;
    let fridge = locations.iter().find(|l| l.kind == "fridge").unwrap().id;
    let (_, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Reject Leftovers",
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
                "quantity": "100",
                "unit": "g",
            })),
            Some(&alice),
        )
        .await;
    let batch_id = batch["id"].as_str().unwrap();

    for used_quantity in ["0", "100", "150"] {
        let (status, _) = app
            .send(
                Method::POST,
                &format!("/api/v1/stock/{batch_id}/consume-and-store"),
                Some(json!({
                    "used_quantity": used_quantity,
                    "remainder_location_id": fridge,
                    "opened_on": "2026-05-01",
                    "remainder_expires_on": "2026-05-02",
                })),
                Some(&alice),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/consume-and-store"),
            Some(json!({
                "used_quantity": "20",
                "remainder_location_id": fridge,
                "opened_on": "2026-05-01",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/consume-and-store"),
            Some(json!({
                "used_quantity": "20",
                "remainder_location_id": fridge,
                "opened_on": "2026-05-02",
                "remainder_expires_on": "2026-05-01",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["message"],
        "bad request: remainder_expires_on cannot be before opened_on"
    );
}

#[tokio::test]
async fn metadata_only_stock_updates_do_not_write_quantity_events() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let locations = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap();
    let pantry = locations.iter().find(|l| l.kind == "pantry").unwrap().id;
    let fridge = locations.iter().find(|l| l.kind == "fridge").unwrap().id;

    let (_, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Yogurt",
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
                "quantity": "500",
                "unit": "g",
                "note": "shelf",
            })),
            Some(&alice),
        )
        .await;
    let batch_id = batch["id"].as_str().unwrap();

    let (status, _) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/stock/{batch_id}"),
            Some(json!([
                { "op": "replace", "path": "/location_id", "value": fridge.to_string() },
                { "op": "replace", "path": "/note", "value": "moved" },
            ])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, events) = app
        .send(
            Method::GET,
            &format!("/api/v1/stock/{batch_id}/events"),
            None,
            Some(&alice),
        )
        .await;
    let items = events["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["event_type"], "add");
}

#[tokio::test]
async fn stock_patch_uses_json_patch_replace_and_remove() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let locations = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap();
    let pantry = locations.iter().find(|l| l.kind == "pantry").unwrap().id;
    let fridge = locations.iter().find(|l| l.kind == "fridge").unwrap().id;

    let (_, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Patch Tea",
                "brand": null,
                "family": "count",
                "preferred_unit": "piece",
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
                "quantity": "4",
                "unit": "piece",
                "produced_on": "2026-04-28",
                "expires_on": "2026-06-01",
                "opened_on": "2026-05-01",
                "note": "box",
            })),
            Some(&alice),
        )
        .await;
    let batch_id = batch["id"].as_str().unwrap();

    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/stock/{batch_id}"),
            Some(json!([
                { "op": "replace", "path": "/location_id", "value": fridge.to_string() },
                { "op": "remove", "path": "/produced_on" },
                { "op": "remove", "path": "/expires_on" },
                { "op": "replace", "path": "/note", "value": "top shelf" },
            ])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["location_id"], fridge.to_string());
    assert!(updated["produced_on"].is_null());
    assert!(updated["expires_on"].is_null());
    assert_eq!(updated["opened_on"], "2026-05-01");
    assert_eq!(updated["note"], "top shelf");

    for body in [
        json!([{ "op": "replace", "path": "/note" }]),
        json!([{ "op": "remove", "path": "/quantity" }]),
        json!([{ "op": "add", "path": "/note", "value": "x" }]),
    ] {
        let (status, _) = app
            .send(
                Method::PATCH,
                &format!("/api/v1/stock/{batch_id}"),
                Some(body),
                Some(&alice),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    let (status, _) = app
        .send(
            Method::DELETE,
            &format!("/api/v1/stock/{batch_id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/stock/{batch_id}"),
            Some(json!([{ "op": "replace", "path": "/note", "value": "should fail" }])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["message"],
        "bad request: depleted stock cannot be edited; restore it before editing"
    );

    let (status, restored) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/restore"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["quantity"], "4");

    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/stock/{batch_id}"),
            Some(json!([{ "op": "replace", "path": "/note", "value": "after restore" }])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["note"], "after restore");
}

#[tokio::test]
async fn restore_many_failure_reports_every_unrestorable_id_and_rolls_back() {
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
                "name": "Beans",
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

    let (_, a) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry,
                "quantity": "100",
                "unit": "g",
            })),
            Some(&alice),
        )
        .await;
    let (_, b) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry,
                "quantity": "200",
                "unit": "g",
            })),
            Some(&alice),
        )
        .await;

    let a_id = a["id"].as_str().unwrap();
    let b_id = b["id"].as_str().unwrap();
    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/stock/restore-many",
            Some(json!({ "ids": [a_id, b_id] })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let unrestorable = body["unrestorable_ids"].as_array().unwrap();
    assert_eq!(unrestorable.len(), 2);

    let (_, after_a) = app
        .send(
            Method::GET,
            &format!("/api/v1/stock/{a_id}"),
            None,
            Some(&alice),
        )
        .await;
    let (_, after_b) = app
        .send(
            Method::GET,
            &format!("/api/v1/stock/{b_id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(after_a["quantity"], "100");
    assert_eq!(after_b["quantity"], "200");
}
