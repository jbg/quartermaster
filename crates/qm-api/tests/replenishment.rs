mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn replenishment_rule_lifecycle_and_cart_generation() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
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

    let (status, settings) = app
        .send(
            Method::GET,
            "/api/v1/replenishment/settings",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!settings["global_disabled"].as_bool().unwrap());

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/rules",
            Some(json!({
                "product_id": product.id,
                "minimum_quantity": "1",
                "target_quantity": "2",
                "unit": "ml",
                "automation_level": "confirm_to_submit"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");

    let (status, rule) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/rules",
            Some(json!({
                "product_id": product.id,
                "minimum_quantity": "500",
                "target_quantity": "1500",
                "unit": "g",
                "preferred_supplier_id": "mock",
                "preferred_supplier_item_id": "mock-rice-1kg",
                "preferred_package_quantity": "1000",
                "preferred_package_unit": "g",
                "automation_level": "confirm_to_submit",
                "expiry_suppression_days": 3
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let rule_id = rule["id"].as_str().unwrap();

    let (status, paused) = app
        .send(
            Method::POST,
            &format!("/api/v1/replenishment/rules/{rule_id}/pause"),
            Some(json!({ "reason": "vacation" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["pause_reason"], "vacation");

    let (status, resumed) = app
        .send(
            Method::POST,
            &format!("/api/v1/replenishment/rules/{rule_id}/resume"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(resumed["paused_at"].is_null());

    let (status, signal) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/demand-signals",
            Some(json!({
                "product_id": product.id,
                "signal_type": "manual_shopping",
                "quantity": "250",
                "unit": "g",
                "note": "shopping list"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(signal["status"], "active");

    let (status, generated) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/cart-drafts",
            Some(json!({
                "supplier_id": "mock",
                "include_ai_explanation": true
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(generated["run"]["status"], "draft_created", "{generated}");
    assert_eq!(generated["run"]["guardrail_decision"], "needs_approval");
    assert!(generated["run"]["ai_explanation"].is_object());
    assert_eq!(
        generated["run"]["recommendations"][0]["supplier_item_id"],
        "mock-rice-1kg"
    );
    assert_eq!(generated["run"]["recommendations"][0]["quantity"], "2");
    let draft_id = generated["draft_id"].as_str().unwrap();

    let (status, draft) = app
        .send(
            Method::GET,
            &format!("/api/v1/suppliers/cart-drafts/{draft_id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(draft["source"], "replenishment");
    assert_eq!(draft["status"], "ready");
    assert_eq!(draft["lines"][0]["quantity"], "2");

    let (status, duplicate) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/cart-drafts",
            Some(json!({ "supplier_id": "mock" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(duplicate["draft_id"].is_null());
    assert_eq!(duplicate["run"]["status"], "blocked");
    assert_eq!(
        duplicate["run"]["suppressions"][0]["reason"],
        "pending_replenishment"
    );
}

#[tokio::test]
async fn replenishment_global_disable_blocks_cart_drafts() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let product = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Beans",
        None,
        "count",
        Some("piece"),
        None,
        None,
    )
    .await
    .unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/rules",
            Some(json!({
                "product_id": product.id,
                "minimum_quantity": "1",
                "target_quantity": "4",
                "unit": "piece",
                "preferred_supplier_id": "mock",
                "preferred_supplier_item_id": "mock-beans-4pk",
                "preferred_package_quantity": "4",
                "preferred_package_unit": "piece",
                "automation_level": "trusted_auto_submit"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _) = app
        .send(
            Method::PUT,
            "/api/v1/replenishment/settings",
            Some(json!({
                "global_disabled": true,
                "notification_lead_minutes": 0
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, generated) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/cart-drafts",
            Some(json!({ "supplier_id": "mock", "submit_trusted": true })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(generated["draft_id"].is_null());
    assert_eq!(generated["run"]["guardrail_decision"], "blocked");
    assert_eq!(
        generated["run"]["suppressions"][0]["reason"], "global_disabled",
        "{generated}"
    );
}

#[tokio::test]
async fn replenishment_trusted_submit_is_queued_and_rechecked() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let product = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Beans",
        None,
        "count",
        Some("piece"),
        None,
        None,
    )
    .await
    .unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/rules",
            Some(json!({
                "product_id": product.id,
                "minimum_quantity": "1",
                "target_quantity": "4",
                "unit": "piece",
                "preferred_supplier_id": "mock",
                "preferred_supplier_item_id": "mock-beans-4pk",
                "preferred_package_quantity": "4",
                "preferred_package_unit": "piece",
                "automation_level": "trusted_auto_submit"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, generated) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/cart-drafts",
            Some(json!({ "supplier_id": "mock", "submit_trusted": true })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(generated["run"]["guardrail_decision"], "allowed");
    let draft_id = generated["draft_id"].as_str().unwrap();
    assert!(qm_db::jobs::active_job_exists(
        &app.db,
        qm_db::jobs::KIND_SUPPLIER_CART_SUBMIT,
        draft_id
    )
    .await
    .unwrap());

    let (status, _) = app
        .send(
            Method::PUT,
            "/api/v1/replenishment/suppliers/mock/policy",
            Some(json!({
                "disabled": true
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = app
        .send(
            Method::POST,
            &format!("/api/v1/suppliers/cart-drafts/{draft_id}/submit"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");
}

#[tokio::test]
async fn replenishment_budget_and_expiry_suppress_cart_lines() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let user = qm_db::users::find_by_email(&app.db, "alice@example.com")
        .await
        .unwrap()
        .unwrap();
    let pantry_id = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|location| location.kind == "pantry")
        .unwrap()
        .id;
    let rice = qm_db::products::create_manual(
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
    qm_db::stock::create(
        &app.db,
        household_id,
        rice.id,
        pantry_id,
        "100",
        "g",
        None,
        Some("2099-01-01"),
        None,
        None,
        user.id,
        None,
    )
    .await
    .unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/rules",
            Some(json!({
                "product_id": rice.id,
                "minimum_quantity": "500",
                "target_quantity": "1000",
                "unit": "g",
                "preferred_supplier_id": "mock",
                "preferred_supplier_item_id": "mock-rice-1kg",
                "preferred_package_quantity": "1000",
                "preferred_package_unit": "g",
                "automation_level": "confirm_to_submit",
                "expiry_suppression_days": 50000
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, generated) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/cart-drafts",
            Some(json!({ "supplier_id": "mock" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(generated["draft_id"].is_null());
    assert_eq!(
        generated["run"]["suppressions"][0]["reason"], "expiring_stock_available",
        "{generated}"
    );

    let beans = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Beans",
        None,
        "count",
        Some("piece"),
        None,
        None,
    )
    .await
    .unwrap();
    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/rules",
            Some(json!({
                "product_id": beans.id,
                "minimum_quantity": "1",
                "target_quantity": "4",
                "unit": "piece",
                "preferred_supplier_id": "mock",
                "preferred_supplier_item_id": "mock-beans-4pk",
                "preferred_package_quantity": "4",
                "preferred_package_unit": "piece",
                "automation_level": "trusted_auto_submit"
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = app
        .send(
            Method::PUT,
            "/api/v1/replenishment/settings",
            Some(json!({
                "global_disabled": false,
                "default_spend_cap_amount": "1.00",
                "default_spend_cap_currency": "USD",
                "notification_lead_minutes": 0
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, generated) = app
        .send(
            Method::POST,
            "/api/v1/replenishment/cart-drafts",
            Some(json!({ "supplier_id": "mock", "submit_trusted": true })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(generated["run"]["guardrail_decision"], "blocked");
    assert!(generated["draft_id"].is_null());
    assert_eq!(
        generated["run"]["guardrail_snapshot"]["reasons"][0],
        "budget_exceeded"
    );
}
