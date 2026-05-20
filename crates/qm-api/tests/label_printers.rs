mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::{json, Value};
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn dry_run_prints_batch_label_with_public_batch_url() {
    let app = TestApp::start(ApiConfig {
        public_base_url: Some("https://quartermaster.example.com".into()),
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let batch_id = seed_batch(&app, &alice).await;

    let (status, printer) = app
        .send(
            Method::POST,
            "/api/v1/label-printers",
            Some(json!({
                "name": "Kitchen Brother",
                "driver": "brother_ql_raster",
                "address": "127.0.0.1",
                "media": "dk_62_continuous",
                "enabled": true,
                "is_default": true
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(printer["port"], 9100);
    assert_eq!(printer["delivery"], "server");

    let (status, printed) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/labels/print"),
            Some(json!({ "dry_run": true, "copies": 2 })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(printed["status"], "rendered");
    assert_eq!(printed["copies"], 2);
    assert_eq!(
        printed["batch_url"],
        format!("https://quartermaster.example.com/batches/{batch_id}")
    );
    assert_eq!(printed["printer_id"], printer["id"]);
}

#[tokio::test]
async fn render_returns_client_print_artifact_without_sending_socket() {
    let app = TestApp::start(ApiConfig {
        public_base_url: Some("https://quartermaster.example.com".into()),
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let batch_id = seed_batch(&app, &alice).await;

    let (status, printer) = app
        .send(
            Method::POST,
            "/api/v1/label-printers",
            Some(json!({
                "name": "Kitchen Brother",
                "driver": "brother_ql_raster",
                "address": "192.0.2.55",
                "port": 9100,
                "media": "dk_62_continuous",
                "delivery": "client",
                "enabled": true,
                "is_default": true
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(printer["delivery"], "client");

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/labels/print"),
            Some(json!({ "dry_run": true })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("render endpoint"));

    let (status, artifact) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/labels/render"),
            Some(json!({
                "copies": 2,
                "label_size": "small",
                "include_quantity": true
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(artifact["printer_id"], printer["id"]);
    assert_eq!(artifact["driver"], "brother_ql_raster");
    assert_eq!(artifact["media"], "dk_62_continuous");
    assert_eq!(artifact["address"], "192.0.2.55");
    assert_eq!(artifact["port"], 9100);
    assert_eq!(artifact["copies"], 2);
    assert_eq!(
        artifact["batch_url"],
        format!("https://quartermaster.example.com/batches/{batch_id}")
    );
    let payload = artifact["payload"].as_str().unwrap();
    assert!(payload.len() > 1000);
    assert!(payload
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=')));

    let (status, test_artifact) = app
        .send(
            Method::POST,
            &format!(
                "/api/v1/label-printers/{}/test/render",
                printer["id"].as_str().unwrap()
            ),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(test_artifact["batch_id"], Uuid::nil().to_string());
    assert!(test_artifact["payload"].as_str().unwrap().len() > 1000);
}

#[tokio::test]
async fn small_label_prints_only_on_continuous_media() {
    let app = TestApp::start(ApiConfig {
        public_base_url: Some("https://quartermaster.example.com".into()),
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let batch_id = seed_batch(&app, &alice).await;

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/label-printers",
            Some(json!({
                "name": "Kitchen Brother",
                "driver": "brother_ql_raster",
                "address": "127.0.0.1",
                "media": "dk_62_continuous",
                "enabled": true,
                "is_default": true
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, printed) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/labels/print"),
            Some(json!({ "dry_run": true, "label_size": "small" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(printed["status"], "rendered");

    let (status, printer) = app
        .send(
            Method::POST,
            "/api/v1/label-printers",
            Some(json!({
                "name": "Die cut Brother",
                "driver": "brother_ql_raster",
                "address": "127.0.0.1",
                "media": "dk_29x90",
                "enabled": true,
                "is_default": false
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/labels/print"),
            Some(json!({
                "dry_run": true,
                "label_size": "small",
                "printer_id": printer["id"],
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].as_str().unwrap().contains("continuous"));
}

#[tokio::test]
async fn print_requires_public_base_url_even_for_dry_run() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let batch_id = seed_batch(&app, &alice).await;
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/label-printers",
            Some(json!({
                "name": "Kitchen Brother",
                "driver": "brother_ql_raster",
                "address": "127.0.0.1",
                "media": "dk_62_continuous",
                "enabled": true,
                "is_default": true
            })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/stock/{batch_id}/labels/print"),
            Some(json!({ "dry_run": true })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("QM_PUBLIC_BASE_URL"));
}

#[tokio::test]
async fn label_printer_defaults_are_household_scoped_and_admin_only() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let (_, first) = app
        .send(
            Method::POST,
            "/api/v1/label-printers",
            Some(printer_json("First", "192.0.2.10", true)),
            Some(&alice),
        )
        .await;
    let (_, second) = app
        .send(
            Method::POST,
            "/api/v1/label-printers",
            Some(printer_json("Second", "192.0.2.11", true)),
            Some(&alice),
        )
        .await;

    let (_, listed) = app
        .send(Method::GET, "/api/v1/label-printers", None, Some(&alice))
        .await;
    let items = listed["items"].as_array().unwrap();
    let first_row = items.iter().find(|item| item["id"] == first["id"]).unwrap();
    let second_row = items
        .iter()
        .find(|item| item["id"] == second["id"])
        .unwrap();
    assert_eq!(first_row["is_default"], false);
    assert_eq!(second_row["is_default"], true);

    let (invite_status, invite) = app
        .send(
            Method::POST,
            "/api/v1/households/current/invites",
            Some(json!({
                "max_uses": 1,
                "role_granted": "read_write"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(invite_status, StatusCode::CREATED);
    assert_eq!(
        app.register("bob", invite["code"].as_str()).await.0,
        StatusCode::CREATED
    );
    let bob = app.login("bob").await;
    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/label-printers",
            Some(printer_json("Member", "192.0.2.12", false)),
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

async fn seed_batch(app: &TestApp, bearer: &str) -> String {
    let me = app.me(bearer).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let pantry = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap();

    let (status, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Flour",
                "brand": "Acme",
                "family": "mass",
                "preferred_unit": "g",
                "barcode": null,
                "image_url": null,
            })),
            Some(bearer),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, batch) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product["id"].as_str().unwrap(),
                "location_id": pantry.id,
                "quantity": "500",
                "unit": "g",
                "produced_on": "2026-05-01",
                "expires_on": "2026-06-01",
                "opened_on": null,
                "note": "bag",
            })),
            Some(bearer),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    batch["id"].as_str().unwrap().to_owned()
}

fn printer_json(name: &str, address: &str, is_default: bool) -> Value {
    json!({
        "name": name,
        "driver": "brother_ql_raster",
        "address": address,
        "port": 9100,
        "media": "dk_62_continuous",
        "enabled": true,
        "is_default": is_default,
    })
}
