mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, PlanLimits, RateLimitConfig};
use serde_json::json;
use support::off_http::MockOffServer;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn product_limit_blocks_new_manual_products() {
    let app = TestApp::start(ApiConfig {
        plan_limits: PlanLimits {
            products_per_household: Some(1),
            ..PlanLimits::default()
        },
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    assert_eq!(
        create_product(&app, &alice, "Rice").await.0,
        StatusCode::CREATED
    );
    let second = create_product(&app, &alice, "Beans").await;

    assert_eq!(second.0, StatusCode::FORBIDDEN);
    assert_eq!(second.1["code"], "plan_limit_exceeded");
}

#[tokio::test]
async fn member_limit_blocks_invite_join() {
    let app = TestApp::start(ApiConfig {
        plan_limits: PlanLimits {
            members_per_household: Some(1),
            ..PlanLimits::default()
        },
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let invite = app
        .send(
            Method::POST,
            "/api/v1/households/current/invites",
            Some(json!({ "max_uses": 1, "role_granted": "read_write" })),
            Some(&alice),
        )
        .await;
    assert_eq!(invite.0, StatusCode::CREATED);

    let bob = app.register("bob", invite.1["code"].as_str()).await;
    assert_eq!(bob.0, StatusCode::FORBIDDEN);
    assert_eq!(bob.1["code"], "plan_limit_exceeded");
}

#[tokio::test]
async fn stock_batch_limit_blocks_new_batches() {
    let app = TestApp::start(ApiConfig {
        plan_limits: PlanLimits {
            stock_batches_per_household: Some(1),
            ..PlanLimits::default()
        },
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let product = create_product(&app, &alice, "Rice").await.1;
    let household_id =
        Uuid::parse_str(me_current_household_id(&app.me(&alice).await).unwrap()).unwrap();
    let location_id = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()[0]
        .id;

    assert_eq!(
        create_stock(&app, &alice, product["id"].as_str().unwrap(), location_id)
            .await
            .0,
        StatusCode::CREATED
    );
    let second = create_stock(&app, &alice, product["id"].as_str().unwrap(), location_id).await;

    assert_eq!(second.0, StatusCode::FORBIDDEN);
    assert_eq!(second.1["code"], "plan_limit_exceeded");
}

#[tokio::test]
async fn barcode_lookup_can_be_rate_limited_per_user() {
    let mock = MockOffServer::start().await;
    let app = TestApp::start(ApiConfig {
        off_api_base_url: mock.base_url(),
        rate_limit_barcode_user: RateLimitConfig {
            requests_per_minute: 1,
            burst: 1,
        },
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let first = app
        .send(
            Method::GET,
            "/api/v1/products/by-barcode/2222222222222",
            None,
            Some(&alice),
        )
        .await;
    let second = app
        .send(
            Method::GET,
            "/api/v1/products/by-barcode/3333333333333",
            None,
            Some(&alice),
        )
        .await;

    assert_eq!(first.0, StatusCode::NOT_FOUND);
    assert_eq!(second.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(second.1["code"], "rate_limited");
}

async fn create_product(app: &TestApp, token: &str, name: &str) -> (StatusCode, serde_json::Value) {
    app.send(
        Method::POST,
        "/api/v1/products",
        Some(json!({
            "name": name,
            "family": "count",
            "preferred_unit": "piece"
        })),
        Some(token),
    )
    .await
}

async fn create_stock(
    app: &TestApp,
    token: &str,
    product_id: &str,
    location_id: Uuid,
) -> (StatusCode, serde_json::Value) {
    app.send(
        Method::POST,
        "/api/v1/stock",
        Some(json!({
            "product_id": product_id,
            "location_id": location_id,
            "quantity": "1",
            "unit": "piece"
        })),
        Some(token),
    )
    .await
}
