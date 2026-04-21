mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::json;
use support::TestApp;
use uuid::Uuid;

fn invite_body(max_uses: i64) -> serde_json::Value {
    json!({
        "expires_at": "2999-01-01T00:00:00.000Z",
        "max_uses": max_uses,
        "role_granted": "member",
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_redeem_for_same_user_consumes_invite_once() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (target_household, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let (_, invite) = app
        .send(Method::POST, "/households/current/invites", Some(invite_body(2)), Some(&alice))
        .await;
    let code = invite["code"].as_str().unwrap().to_owned();
    let invite_id = Uuid::parse_str(invite["id"].as_str().unwrap()).unwrap();

    let _ = app.seed_household_admin("bob").await;
    let bob = app.login("bob").await;

    let (first, second) = tokio::join!(
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": code.clone() })),
            Some(&bob),
        ),
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": code })),
            Some(&bob),
        )
    );
    assert_eq!(first.0, StatusCode::NO_CONTENT);
    assert_eq!(second.0, StatusCode::NO_CONTENT);

    let me = app.me(&bob).await;
    assert_eq!(me["household_id"].as_str().unwrap(), target_household.to_string());
    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id).await.unwrap().unwrap();
    assert_eq!(invite_row.use_count, 1);
    assert!(qm_db::memberships::find(&app.db, target_household, Uuid::parse_str(me["user"]["id"].as_str().unwrap()).unwrap())
        .await
        .unwrap()
        .is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_single_use_registration_creates_no_orphaned_user() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let _ = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let (_, invite) = app
        .send(Method::POST, "/households/current/invites", Some(invite_body(1)), Some(&alice))
        .await;
    let code = invite["code"].as_str().unwrap().to_owned();
    let invite_id = Uuid::parse_str(invite["id"].as_str().unwrap()).unwrap();

    let a = tokio::join!(
        app.register("bob", Some(&code)),
        app.register("carol", Some(&code))
    );
    let statuses = [a.0 .0, a.1 .0];
    assert_eq!(statuses.iter().filter(|s| **s == StatusCode::CREATED).count(), 1);
    assert_eq!(statuses.iter().filter(|s| **s != StatusCode::CREATED).count(), 1);

    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id).await.unwrap().unwrap();
    assert_eq!(invite_row.use_count, 1);
    assert_eq!(qm_db::users::count(&app.db).await.unwrap(), 2);
    let bob_exists = qm_db::users::find_by_username(&app.db, "bob").await.unwrap().is_some();
    let carol_exists = qm_db::users::find_by_username(&app.db, "carol").await.unwrap().is_some();
    assert_ne!(bob_exists, carol_exists);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_multi_use_redeems_do_not_exceed_max_uses() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (target_household, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let (_, invite) = app
        .send(Method::POST, "/households/current/invites", Some(invite_body(2)), Some(&alice))
        .await;
    let code = invite["code"].as_str().unwrap().to_owned();
    let invite_id = Uuid::parse_str(invite["id"].as_str().unwrap()).unwrap();

    for username in ["bob", "carol", "dave"] {
        let _ = app.seed_household_admin(username).await;
    }
    let bob = app.login("bob").await;
    let carol = app.login("carol").await;
    let dave = app.login("dave").await;

    let (r1, r2, r3) = tokio::join!(
        app.send(Method::POST, "/invites/redeem", Some(json!({ "invite_code": code.clone() })), Some(&bob)),
        app.send(Method::POST, "/invites/redeem", Some(json!({ "invite_code": code.clone() })), Some(&carol)),
        app.send(Method::POST, "/invites/redeem", Some(json!({ "invite_code": code })), Some(&dave)),
    );
    let statuses = [r1.0, r2.0, r3.0];
    let success_count = statuses.iter().filter(|s| **s == StatusCode::NO_CONTENT).count();
    assert!(success_count >= 1);
    assert!(success_count <= 2);

    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id).await.unwrap().unwrap();
    assert!(invite_row.use_count <= 2);
    let bob_me = app.me(&bob).await;
    let carol_me = app.me(&carol).await;
    let dave_me = app.me(&dave).await;
    let joined = [bob_me, carol_me, dave_me]
        .into_iter()
        .filter(|me| me["household_id"].as_str().unwrap() == target_household.to_string())
        .count();
    assert_eq!(joined as i64, invite_row.use_count);
}

#[tokio::test]
async fn metadata_only_stock_updates_do_not_write_quantity_events() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me["household_id"].as_str().unwrap()).unwrap();
    let locations = qm_db::locations::list_for_household(&app.db, household_id).await.unwrap();
    let pantry = locations.iter().find(|l| l.kind == "pantry").unwrap().id;
    let fridge = locations.iter().find(|l| l.kind == "fridge").unwrap().id;

    let (_, product) = app
        .send(
            Method::POST,
            "/products",
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
            "/stock",
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
            &format!("/stock/{batch_id}"),
            Some(json!({
                "location_id": fridge,
                "note": "moved",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, events) = app
        .send(Method::GET, &format!("/stock/{batch_id}/events"), None, Some(&alice))
        .await;
    let items = events["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["event_type"], "add");
}

#[tokio::test]
async fn restore_many_failure_reports_every_unrestorable_id_and_rolls_back() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me["household_id"].as_str().unwrap()).unwrap();
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
            "/stock",
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
            "/stock",
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
            "/stock/restore-many",
            Some(json!({ "ids": [a_id, b_id] })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let unrestorable = body["unrestorable_ids"].as_array().unwrap();
    assert_eq!(unrestorable.len(), 2);

    let (_, after_a) = app.send(Method::GET, &format!("/stock/{a_id}"), None, Some(&alice)).await;
    let (_, after_b) = app.send(Method::GET, &format!("/stock/{b_id}"), None, Some(&alice)).await;
    assert_eq!(after_a["quantity"], "100");
    assert_eq!(after_b["quantity"], "200");
}

#[tokio::test]
async fn deleted_manual_products_stay_visible_in_history_but_reject_new_stock() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me["household_id"].as_str().unwrap()).unwrap();
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
        app.send(Method::DELETE, &format!("/stock/{batch_id}"), None, Some(&alice))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send(Method::DELETE, &format!("/products/{product_id}"), None, Some(&alice))
            .await
            .0,
        StatusCode::NO_CONTENT
    );

    let (status, history) = app.send(Method::GET, "/stock/events?limit=20", None, Some(&alice)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(history["items"].as_array().unwrap().iter().any(|item| item["product"]["id"] == product_id));

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
        .send(Method::GET, &format!("/stock/{batch_id}"), None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(batch_detail["code"], "not_found");
}

#[tokio::test]
async fn stale_tokens_follow_current_household_and_cannot_access_prior_household_resources() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_a, alice_id) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let pantry_a = qm_db::locations::list_for_household(&app.db, household_a)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;
    let product_a = qm_db::products::create_manual(
        &app.db,
        household_a,
        "Rice",
        None,
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();
    let batch_a = qm_db::stock::create(
        &app.db,
        household_a,
        product_a.id,
        pantry_a,
        "100",
        "g",
        None,
        None,
        None,
        alice_id,
    )
    .await
    .unwrap();

    let a_invite = qm_db::invites::create(
        &app.db,
        household_a,
        "HOUSEA123",
        alice_id,
        "2999-01-01T00:00:00.000Z",
        5,
        "member",
    )
    .await
    .unwrap();

    let household_b = qm_db::households::create(&app.db, "Cabin").await.unwrap();
    qm_db::locations::seed_defaults(&app.db, household_b.id).await.unwrap();
    let owner_b = qm_db::users::create(&app.db, "ownerb", Some("ownerb@example.com"), "hash")
        .await
        .unwrap();
    qm_db::memberships::insert(&app.db, household_b.id, owner_b.id, "admin")
        .await
        .unwrap();
    let b_invite = qm_db::invites::create(
        &app.db,
        household_b.id,
        "HOUSEB123",
        owner_b.id,
        "2999-01-01T00:00:00.000Z",
        5,
        "admin",
    )
    .await
    .unwrap();

    assert_eq!(
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": b_invite.code })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me = app.me(&alice).await;
    assert_eq!(me["household_id"].as_str().unwrap(), household_b.id.to_string());

    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/invites/{}", a_invite.id),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/stock/{}", batch_a.id),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
