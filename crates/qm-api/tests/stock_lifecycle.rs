mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn household_measurement_system_controls_units_and_consumption() {
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

    let (status, units) = app
        .send(Method::GET, "/api/v1/units", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unit_factor(&units, "tsp"), 5_000);
    assert_eq!(unit_factor(&units, "tbsp"), 15_000);

    let metric_batch_id = create_sauce_batch(&app, &alice, pantry_id, "Metric soy").await;
    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/stock/consume",
            Some(json!({
                "product_id": product_id_for_batch(&app, household_id, &metric_batch_id).await,
                "location_id": pantry_id,
                "quantity": "2",
                "unit": "tbsp",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/api/v1/stock/{metric_batch_id}"),
            None,
            Some(&alice),
        )
        .await
        .1["quantity"],
        "970"
    );

    let (status, household) = app
        .send(
            Method::PATCH,
            "/api/v1/households/current",
            Some(json!({
                "name": "Alice's Household",
                "timezone": "UTC",
                "measurement_system": "us_customary",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(household["measurement_system"], "us_customary");

    let (status, units) = app
        .send(Method::GET, "/api/v1/units", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unit_factor(&units, "tsp"), 4_929);
    assert_eq!(unit_factor(&units, "tbsp"), 14_787);

    let us_batch_id = create_sauce_batch(&app, &alice, pantry_id, "US soy").await;
    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/stock/consume",
            Some(json!({
                "product_id": product_id_for_batch(&app, household_id, &us_batch_id).await,
                "location_id": pantry_id,
                "quantity": "2",
                "unit": "tbsp",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/api/v1/stock/{us_batch_id}"),
            None,
            Some(&alice),
        )
        .await
        .1["quantity"],
        "970.426"
    );
}

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

    let (status, vessel) = app
        .send(
            Method::POST,
            "/api/v1/storage-vessels",
            Some(json!({
                "name": "1L Mason jar",
                "tare_weight": "410",
                "tare_unit": "g",
                "sort_order": 0,
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let vessel_id = vessel["id"].as_str().unwrap();

    let (status, batch) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry_id,
                "storage_vessel_id": vessel_id,
                "quantity": "910",
                "unit": "g",
                "quantity_includes_storage_vessel": true,
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
    assert_eq!(batch["initial_quantity"], "500");
    assert_eq!(batch["quantity"], "500");
    assert_eq!(batch["storage_vessel"]["name"], "1L Mason jar");
    assert_eq!(batch["storage_vessel"]["tare_weight"], "410");
    assert_eq!(batch["storage_vessel"]["tare_unit"], "g");
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
    assert_eq!(listed["items"][0]["storage_vessel"]["id"], vessel_id);

    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/stock/{batch_id}"),
            Some(json!([
                { "op": "replace", "path": "/quantity", "value": "450" },
                { "op": "remove", "path": "/storage_vessel_id" }
            ])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["quantity"], "450");
    assert!(updated["storage_vessel"].is_null());

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
async fn stock_and_location_operations_do_not_cross_households() {
    let app = TestApp::start(ApiConfig {
        registration_mode: qm_api::RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    assert_eq!(app.register("bob", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let bob = app.login("bob").await;

    let alice_household =
        Uuid::parse_str(me_current_household_id(&app.me(&alice).await).unwrap()).unwrap();
    let bob_household =
        Uuid::parse_str(me_current_household_id(&app.me(&bob).await).unwrap()).unwrap();
    let alice_pantry = qm_db::locations::list_for_household(&app.db, alice_household)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;
    let bob_pantry = qm_db::locations::list_for_household(&app.db, bob_household)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;

    let (status, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Tenant Rice",
                "brand": null,
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
                "location_id": alice_pantry,
                "quantity": "100",
                "unit": "g",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let batch_id = batch["id"].as_str().unwrap();

    for (method, path, body) in [
        (Method::GET, format!("/api/v1/stock/{batch_id}"), None),
        (
            Method::PATCH,
            format!("/api/v1/stock/{batch_id}"),
            Some(json!([{ "op": "replace", "path": "/quantity", "value": "50" }])),
        ),
        (Method::DELETE, format!("/api/v1/stock/{batch_id}"), None),
        (
            Method::GET,
            format!("/api/v1/stock/{batch_id}/events"),
            None,
        ),
    ] {
        let (status, body) = app.send(method, &path, body, Some(&bob)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "not_found");
    }

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": bob_pantry,
                "quantity": "10",
                "unit": "g",
            })),
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/stock/{batch_id}"),
            Some(json!([{ "op": "replace", "path": "/location_id", "value": bob_pantry.to_string() }])),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, bob_history) = app
        .send(
            Method::GET,
            "/api/v1/stock/events?limit=20",
            None,
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(bob_history["items"].as_array().unwrap().is_empty());

    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/api/v1/stock/{batch_id}"),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (status, _) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/restore"),
            None,
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/stock/restore-many",
            Some(json!({ "ids": [batch_id] })),
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, restored) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/restore"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["id"], batch_id);
}

fn unit_factor(units: &serde_json::Value, code: &str) -> i64 {
    units
        .as_array()
        .unwrap()
        .iter()
        .find(|unit| unit["code"] == code)
        .unwrap()["to_base_milli"]
        .as_i64()
        .unwrap()
}

async fn create_sauce_batch(app: &TestApp, bearer: &str, pantry_id: Uuid, name: &str) -> String {
    let (status, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": name,
                "brand": null,
                "family": "volume",
                "preferred_unit": "ml",
                "barcode": null,
                "image_url": null,
            })),
            Some(bearer),
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
                "quantity": "1000",
                "unit": "ml",
                "produced_on": null,
                "expires_on": null,
                "opened_on": null,
                "note": null,
            })),
            Some(bearer),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    batch["id"].as_str().unwrap().to_owned()
}

async fn product_id_for_batch(app: &TestApp, household_id: Uuid, batch_id: &str) -> String {
    qm_db::stock::get(&app.db, household_id, Uuid::parse_str(batch_id).unwrap())
        .await
        .unwrap()
        .unwrap()
        .product_id
        .to_string()
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
async fn gross_vessel_weight_is_only_for_free_weight_stock() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
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
            "/api/v1/products",
            Some(json!({
                "name": "Packaged flour",
                "brand": null,
                "family": "mass",
                "preferred_unit": "g",
                "package_quantity": "500",
                "package_unit": "g",
                "barcode": null,
                "image_url": null,
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, vessel) = app
        .send(
            Method::POST,
            "/api/v1/storage-vessels",
            Some(json!({
                "name": "Jar",
                "tare_weight": "100",
                "tare_unit": "g",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product["id"],
                "location_id": pantry_id,
                "storage_vessel_id": vessel["id"],
                "quantity": "600",
                "unit": "g",
                "quantity_includes_storage_vessel": true,
                "expires_on": null,
                "opened_on": null,
                "note": null,
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("cannot be used for packaged products"));
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
