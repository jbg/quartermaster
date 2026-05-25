mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::json;
use support::{me_current_household_id, me_current_household_name, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn first_run_only_bootstraps_once() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    assert_eq!(app.register("bob", None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn open_registration_creates_distinct_households() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    assert_eq!(app.register("bob", None).await.0, StatusCode::CREATED);

    let alice = app.login("alice").await;
    let bob = app.login("bob").await;
    let alice_me = app.me(&alice).await;
    let bob_me = app.me(&bob).await;
    assert_ne!(
        me_current_household_id(&alice_me),
        me_current_household_id(&bob_me)
    );
}

#[tokio::test]
async fn login_initializes_active_household_from_latest_joined_and_me_lists_memberships() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        public_base_url: Some("https://quartermaster.example.com".into()),
        ..ApiConfig::default()
    })
    .await;
    let (older_household, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let newer_household = qm_db::households::create(&app.db, "Cabin", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, newer_household.id)
        .await
        .unwrap();
    let admin = qm_db::users::create(&app.db, "owner2@example.com", "Owner 2", "hash")
        .await
        .unwrap();
    qm_db::memberships::insert(&app.db, newer_household.id, admin.id, "admin")
        .await
        .unwrap();

    let invite = qm_db::invites::create(
        &app.db,
        newer_household.id,
        "JOINCABIN123",
        admin.id,
        "2999-01-01T00:00:00.000Z",
        5,
        "read_write",
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/invites/redeem",
            Some(json!({ "invite_code": invite.code })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me = app.me(&alice).await;
    assert_eq!(
        me_current_household_id(&me).unwrap(),
        newer_household.id.to_string()
    );
    assert_eq!(
        me_current_household_name(&me).unwrap(),
        newer_household.name
    );
    assert_eq!(
        me["public_base_url"].as_str().unwrap(),
        "https://quartermaster.example.com"
    );
    assert_eq!(me["households"].as_array().unwrap().len(), 2);
    assert_eq!(
        me["households"][0]["id"].as_str().unwrap(),
        newer_household.id.to_string()
    );
    assert_eq!(me["households"][0]["role"].as_str().unwrap(), "read_write");

    sqlx::query("UPDATE membership SET joined_at = ? WHERE user_id = ?")
        .bind("2026-01-01T00:00:00.000Z")
        .bind(me["user"]["id"].as_str().unwrap())
        .execute(&app.db.pool)
        .await
        .unwrap();

    let me_after_tie = app.me(&alice).await;
    assert_eq!(
        me_current_household_id(&me_after_tie).unwrap(),
        newer_household.id.to_string()
    );

    let alice_second_session = app.login("alice").await;
    let me_in_new_session = app.me(&alice_second_session).await;
    let expected = if newer_household.id > older_household {
        newer_household.id
    } else {
        older_household
    };
    assert_eq!(
        me_current_household_id(&me_in_new_session).unwrap(),
        expected.to_string()
    );
}

#[tokio::test]
async fn switch_household_is_session_scoped_and_rejects_non_members() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (home_household, home_admin) = app.seed_household_admin("alice").await;

    let cabin_household = qm_db::households::create(&app.db, "Cabin", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, cabin_household.id)
        .await
        .unwrap();
    let cabin_admin = qm_db::users::create(&app.db, "cabin@example.com", "Cabin Owner", "hash")
        .await
        .unwrap();
    qm_db::memberships::insert(&app.db, cabin_household.id, cabin_admin.id, "admin")
        .await
        .unwrap();
    let invite = qm_db::invites::create(
        &app.db,
        cabin_household.id,
        "JOINCABIN123",
        cabin_admin.id,
        "2999-01-01T00:00:00.000Z",
        5,
        "read_write",
    )
    .await
    .unwrap();

    let alice_session_a = app.login("alice").await;
    let alice_session_b = app.login("alice").await;
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/invites/redeem",
            Some(json!({ "invite_code": invite.code })),
            Some(&alice_session_a),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me_a = app.me(&alice_session_a).await;
    let me_b = app.me(&alice_session_b).await;
    assert_eq!(
        me_current_household_id(&me_a).unwrap(),
        cabin_household.id.to_string()
    );
    assert_eq!(
        me_current_household_id(&me_b).unwrap(),
        home_household.to_string()
    );

    let switch_status = app
        .send(
            Method::POST,
            "/api/v1/auth/switch-household",
            Some(json!({ "household_id": home_household })),
            Some(&alice_session_a),
        )
        .await;
    assert_eq!(switch_status.0, StatusCode::OK);
    assert_eq!(
        me_current_household_id(&switch_status.1).unwrap(),
        home_household.to_string()
    );

    let pantry_home = qm_db::locations::list_for_household(&app.db, home_household)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;
    let product_home = qm_db::products::create_manual(
        &app.db,
        home_household,
        "Rice",
        None,
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();
    let home_batch = qm_db::stock::create(
        &app.db,
        home_household,
        product_home.id,
        pantry_home,
        "100",
        "g",
        None,
        None,
        None,
        None,
        home_admin,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/api/v1/stock/{}", home_batch.id),
            None,
            Some(&alice_session_a),
        )
        .await
        .0,
        StatusCode::OK
    );

    let outsider_household = qm_db::households::create(&app.db, "Outsider", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, outsider_household.id)
        .await
        .unwrap();
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/auth/switch-household",
            Some(json!({ "household_id": outsider_household.id })),
            Some(&alice_session_a),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn removing_active_membership_falls_back_and_last_membership_clears_active_household() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (older_household, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let newer_household = qm_db::households::create(&app.db, "Cabin", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, newer_household.id)
        .await
        .unwrap();
    let admin = qm_db::users::create(&app.db, "owner2@example.com", "Owner 2", "hash")
        .await
        .unwrap();
    qm_db::memberships::insert(&app.db, newer_household.id, admin.id, "admin")
        .await
        .unwrap();

    let invite = qm_db::invites::create(
        &app.db,
        newer_household.id,
        "JOINCABIN456",
        admin.id,
        "2999-01-01T00:00:00.000Z",
        5,
        "read_write",
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/invites/redeem",
            Some(json!({ "invite_code": invite.code })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    qm_db::memberships::remove(
        &app.db,
        newer_household.id,
        Uuid::parse_str(app.me(&alice).await["user"]["id"].as_str().unwrap()).unwrap(),
    )
    .await
    .unwrap();
    let me_after_removal = app.me(&alice).await;
    assert_eq!(
        me_current_household_id(&me_after_removal).unwrap(),
        older_household.to_string()
    );

    qm_db::memberships::remove(
        &app.db,
        older_household,
        Uuid::parse_str(me_after_removal["user"]["id"].as_str().unwrap()).unwrap(),
    )
    .await
    .unwrap();
    let me_after_last_removal = app.me(&alice).await;
    assert!(me_after_last_removal["current_household"].is_null());
}

#[tokio::test]
async fn authenticated_user_without_memberships_can_create_household_and_become_active_admin() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let user_id = app.seed_user_without_household("orphaned").await;
    let session_a = app.login("orphaned").await;
    let session_b = app.login("orphaned").await;

    let me_before = app.me(&session_a).await;
    assert!(me_before["current_household"].is_null());
    assert!(me_before["households"].as_array().unwrap().is_empty());

    let (status, created) = app
        .send(
            Method::POST,
            "/api/v1/households",
            Some(json!({ "name": "Fresh Start", "timezone": "UTC" })),
            Some(&session_a),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let household_id = Uuid::parse_str(me_current_household_id(&created).unwrap()).unwrap();
    assert_eq!(me_current_household_name(&created).unwrap(), "Fresh Start");
    assert_eq!(created["households"].as_array().unwrap().len(), 1);
    assert_eq!(created["households"][0]["role"].as_str().unwrap(), "admin");

    let locations = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap();
    let location_kinds: Vec<_> = locations.into_iter().map(|row| row.kind).collect();
    assert_eq!(location_kinds, vec!["pantry", "fridge", "freezer"]);

    let membership = qm_db::memberships::find(&app.db, household_id, user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(membership.role, "admin");

    let me_session_a = app.me(&session_a).await;
    assert_eq!(
        me_current_household_id(&me_session_a).unwrap(),
        household_id.to_string()
    );

    let session_a_id =
        qm_db::tokens::find_active_by_hash(&app.db, &qm_api::auth::sha256_hex(&session_a))
            .await
            .unwrap()
            .unwrap()
            .session_id;
    let session_b_id =
        qm_db::tokens::find_active_by_hash(&app.db, &qm_api::auth::sha256_hex(&session_b))
            .await
            .unwrap()
            .unwrap()
            .session_id;
    assert_eq!(
        qm_db::auth_sessions::find(&app.db, session_a_id)
            .await
            .unwrap()
            .unwrap()
            .active_household_id,
        Some(household_id)
    );
    assert_eq!(
        qm_db::auth_sessions::find(&app.db, session_b_id)
            .await
            .unwrap()
            .unwrap()
            .active_household_id,
        None
    );
}

#[tokio::test]
async fn create_household_restores_active_context_after_last_membership_is_removed() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let alice_id = Uuid::parse_str(app.me(&alice).await["user"]["id"].as_str().unwrap()).unwrap();
    qm_db::memberships::remove(&app.db, household_id, alice_id)
        .await
        .unwrap();

    let me_after_removal = app.me(&alice).await;
    assert!(me_after_removal["current_household"].is_null());

    let (status, created) = app
        .send(
            Method::POST,
            "/api/v1/households",
            Some(json!({ "name": "Replacement Home", "timezone": "UTC" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        me_current_household_name(&created).unwrap(),
        "Replacement Home"
    );
    assert_eq!(created["households"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn household_export_import_round_trips_inventory_audit_data() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (source_household, alice_id) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    let bob_id = app.seed_user_without_household("bob").await;
    let bob = app.login("bob").await;

    let pantry = qm_db::locations::list_for_household(&app.db, source_household)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap();
    let product = qm_db::products::create_manual(
        &app.db,
        source_household,
        "Rice",
        Some("Acme"),
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();
    qm_db::barcode_cache::put_hit(&app.db, source_household, "1234567890123", product.id)
        .await
        .unwrap();
    let batch = qm_db::stock::create(
        &app.db,
        source_household,
        product.id,
        pantry.id,
        "100",
        "g",
        None,
        None,
        None,
        Some("export me"),
        alice_id,
        None,
    )
    .await
    .unwrap();
    qm_db::stock::adjust(
        &app.db,
        source_household,
        batch.id,
        "75",
        alice_id,
        None,
        None,
    )
    .await
    .unwrap();

    let (export_status, document) = app
        .send(
            Method::GET,
            "/api/v1/households/current/export",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(export_status, StatusCode::OK);
    assert_eq!(document["schema_version"], 2);
    assert_eq!(document["products"].as_array().unwrap().len(), 1);
    assert_eq!(document["stock_events"].as_array().unwrap().len(), 2);
    assert!(document.get("memberships").is_none());

    let (import_status, imported_me) = app
        .send(
            Method::POST,
            "/api/v1/households/import",
            Some(document),
            Some(&bob),
        )
        .await;
    assert_eq!(import_status, StatusCode::CREATED);
    let imported_household =
        Uuid::parse_str(me_current_household_id(&imported_me).unwrap()).unwrap();
    assert_ne!(imported_household, source_household);
    assert_eq!(imported_me["households"].as_array().unwrap().len(), 1);
    assert_eq!(imported_me["households"][0]["role"], "admin");

    let imported_stock = qm_db::stock::list(
        &app.db,
        imported_household,
        &qm_db::stock::StockFilter {
            include_depleted: true,
            ..qm_db::stock::StockFilter::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(imported_stock.len(), 1);
    assert_eq!(imported_stock[0].batch.quantity, "75");
    assert_eq!(imported_stock[0].batch.created_by, bob_id);
    assert_eq!(imported_stock[0].product.name, "Rice");

    let imported_events = qm_db::stock_events::list_for_household(&app.db, imported_household, 10)
        .await
        .unwrap();
    assert_eq!(imported_events.len(), 2);
    assert!(imported_events
        .iter()
        .all(|event| event.created_by == bob_id));
}

#[tokio::test]
async fn household_import_rejects_invalid_documents() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (_, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (_, mut document) = app
        .send(
            Method::GET,
            "/api/v1/households/current/export",
            None,
            Some(&alice),
        )
        .await;
    let locations = document["locations"].as_array().unwrap().clone();
    document["locations"] = json!([locations[0].clone(), locations[0].clone()]);

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/households/import",
            Some(document),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");
}

#[tokio::test]
async fn household_export_and_deletion_are_admin_only() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, _) = app.seed_household_admin("alice").await;
    let bob_id = app.seed_user_without_household("bob").await;
    qm_db::memberships::insert(&app.db, household_id, bob_id, "read_write")
        .await
        .unwrap();
    let bob = app.login("bob").await;

    assert_eq!(
        app.send(
            Method::GET,
            "/api/v1/households/current/export",
            None,
            Some(&bob),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/households/current/deletion",
            Some(json!({ "confirmation_name": "Home" })),
            Some(&bob),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn household_deletion_hides_household_and_queues_purge() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (wrong_status, wrong_body) = app
        .send(
            Method::POST,
            "/api/v1/households/current/deletion",
            Some(json!({ "confirmation_name": "Not Home" })),
            Some(&alice),
        )
        .await;
    assert_eq!(wrong_status, StatusCode::BAD_REQUEST);
    assert_eq!(wrong_body["code"], "bad_request");

    let (delete_status, delete_body) = app
        .send(
            Method::POST,
            "/api/v1/households/current/deletion",
            Some(json!({ "confirmation_name": "Home" })),
            Some(&alice),
        )
        .await;
    assert_eq!(delete_status, StatusCode::ACCEPTED);
    assert_eq!(delete_body["household_id"], household_id.to_string());
    assert_eq!(delete_body["status"], "queued");
    assert!(delete_body["purge_job_id"].as_str().is_some());

    let me_after_delete = app.me(&alice).await;
    assert!(me_after_delete["current_household"].is_null());
    assert!(me_after_delete["households"].as_array().unwrap().is_empty());
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/auth/switch-household",
            Some(json!({ "household_id": household_id })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM background_job WHERE kind = ? AND dedupe_key = ? AND status = ?",
    )
    .bind(qm_db::jobs::KIND_HOUSEHOLD_PURGE)
    .bind(household_id.to_string())
    .bind(qm_db::jobs::STATUS_PENDING)
    .fetch_one(&app.db.pool)
    .await
    .unwrap();
    assert_eq!(queued, 1);
}

#[tokio::test]
async fn logout_revokes_tokens_and_deletes_auth_session_row() {
    let app = TestApp::start(ApiConfig::default()).await;
    let _ = app.register("alice", None).await;
    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({
                "email": "alice@example.com",
                "password": "password123",
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let access = body["access_token"].as_str().unwrap();
    let hash = qm_api::auth::sha256_hex(access);
    let token = qm_db::tokens::find_active_by_hash(&app.db, &hash)
        .await
        .unwrap()
        .unwrap();
    assert!(qm_db::auth_sessions::find(&app.db, token.session_id)
        .await
        .unwrap()
        .is_some());

    assert_eq!(
        app.send(Method::POST, "/api/v1/auth/logout", None, Some(access))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert!(qm_db::auth_sessions::find(&app.db, token.session_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn expired_only_session_is_cleaned_up_when_access_token_is_used() {
    let app = TestApp::start(ApiConfig::default()).await;
    let user_id = app.seed_user_without_household("alice").await;
    let session_id = Uuid::now_v7();
    let expired_access = "expired-access-token";

    qm_db::auth_sessions::upsert(&app.db, session_id, user_id, None)
        .await
        .unwrap();
    qm_db::tokens::create(
        &app.db,
        user_id,
        session_id,
        &qm_api::auth::sha256_hex(expired_access),
        qm_db::tokens::KIND_ACCESS,
        Some("iPhone"),
        jiff::Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_mins(1))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(
        app.send(Method::GET, "/api/v1/auth/me", None, Some(expired_access))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert!(qm_db::auth_sessions::find(&app.db, session_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn expired_access_keeps_session_when_refresh_token_is_still_live() {
    let app = TestApp::start(ApiConfig::default()).await;
    let user_id = app.seed_user_without_household("alice").await;
    let session_id = Uuid::now_v7();
    let expired_access = "expired-access-token";
    let live_refresh = "live-refresh-token";

    qm_db::auth_sessions::upsert(&app.db, session_id, user_id, None)
        .await
        .unwrap();
    qm_db::tokens::create(
        &app.db,
        user_id,
        session_id,
        &qm_api::auth::sha256_hex(expired_access),
        qm_db::tokens::KIND_ACCESS,
        Some("iPhone"),
        jiff::Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_mins(1))
            .unwrap(),
    )
    .await
    .unwrap();
    qm_db::tokens::create(
        &app.db,
        user_id,
        session_id,
        &qm_api::auth::sha256_hex(live_refresh),
        qm_db::tokens::KIND_REFRESH,
        Some("iPhone"),
        jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_mins(30))
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(
        app.send(Method::GET, "/api/v1/auth/me", None, Some(expired_access))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert!(qm_db::auth_sessions::find(&app.db, session_id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn refresh_rotation_keeps_auth_session_row_alive() {
    let app = TestApp::start(ApiConfig::default()).await;
    let _ = app.register("alice", None).await;
    let (status, login) = app
        .send(
            Method::POST,
            "/api/v1/auth/login",
            Some(json!({
                "email": "alice@example.com",
                "password": "password123",
            })),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let refresh = login["refresh_token"].as_str().unwrap();
    let token = qm_db::tokens::find_active_by_hash(&app.db, &qm_api::auth::sha256_hex(refresh))
        .await
        .unwrap()
        .unwrap();

    let (refresh_status, rotated) = app
        .send(
            Method::POST,
            "/api/v1/auth/refresh",
            Some(json!({ "refresh_token": refresh })),
            None,
        )
        .await;
    assert_eq!(refresh_status, StatusCode::OK);
    assert!(rotated["access_token"].as_str().is_some());
    assert!(qm_db::auth_sessions::find(&app.db, token.session_id)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn member_removal_and_location_deletion_guards_work() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, alice_id) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (_, invite) = app
        .send(
            Method::POST,
            "/api/v1/households/current/invites",
            Some(json!({
                "max_uses": 1,
                "role_granted": "read_write",
            })),
            Some(&alice),
        )
        .await;
    let code = invite["code"].as_str().unwrap().to_owned();
    assert_eq!(
        app.register("bob", Some(&code)).await.0,
        StatusCode::CREATED
    );

    let members = app
        .send(
            Method::GET,
            "/api/v1/households/current/members",
            None,
            Some(&alice),
        )
        .await
        .1;
    let bob_id = members
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user"]["email"] == "bob@example.com")
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/api/v1/households/current/members/{bob_id}"),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/api/v1/households/current/members/{alice_id}"),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    let pantry_id = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()[0]
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
    qm_db::stock::create(
        &app.db,
        household_id,
        product.id,
        pantry_id,
        "100",
        "g",
        None,
        None,
        None,
        None,
        alice_id,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/api/v1/locations/{pantry_id}"),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    let (status, new_loc) = app
        .send(
            Method::POST,
            "/api/v1/locations",
            Some(json!({ "name": "Overflow", "kind": "pantry" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/api/v1/locations/{}", new_loc["id"].as_str().unwrap()),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
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
        None,
        alice_id,
        None,
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
        "read_write",
    )
    .await
    .unwrap();

    let household_b = qm_db::households::create(&app.db, "Cabin", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, household_b.id)
        .await
        .unwrap();
    let owner_b = qm_db::users::create(&app.db, "ownerb@example.com", "Owner B", "hash")
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
            "/api/v1/invites/redeem",
            Some(json!({ "invite_code": b_invite.code })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me = app.me(&alice).await;
    assert_eq!(
        me_current_household_id(&me).unwrap(),
        household_b.id.to_string()
    );

    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/api/v1/invites/{}", a_invite.id),
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
            &format!("/api/v1/stock/{}", batch_a.id),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
