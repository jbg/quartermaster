mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::json;
use support::TestApp;
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
    assert_ne!(alice_me["household_id"], bob_me["household_id"]);
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
    let admin = qm_db::users::create(&app.db, "owner2", Some("owner2@example.com"), "hash")
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
        "member",
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": invite.code })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me = app.me(&alice).await;
    assert_eq!(
        me["household_id"].as_str().unwrap(),
        newer_household.id.to_string()
    );
    assert_eq!(me["household_name"].as_str().unwrap(), newer_household.name);
    assert_eq!(
        me["public_base_url"].as_str().unwrap(),
        "https://quartermaster.example.com"
    );
    assert_eq!(me["households"].as_array().unwrap().len(), 2);
    assert_eq!(
        me["households"][0]["household"]["id"].as_str().unwrap(),
        newer_household.id.to_string()
    );
    assert_eq!(me["households"][0]["role"].as_str().unwrap(), "member");

    sqlx::query("UPDATE membership SET joined_at = ? WHERE user_id = ?")
        .bind("2026-01-01T00:00:00.000Z")
        .bind(me["user"]["id"].as_str().unwrap())
        .execute(&app.db.pool)
        .await
        .unwrap();

    let me_after_tie = app.me(&alice).await;
    assert_eq!(
        me_after_tie["household_id"].as_str().unwrap(),
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
        me_in_new_session["household_id"].as_str().unwrap(),
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
    let cabin_admin =
        qm_db::users::create(&app.db, "cabin-owner", Some("cabin@example.com"), "hash")
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
        "member",
    )
    .await
    .unwrap();

    let alice_session_a = app.login("alice").await;
    let alice_session_b = app.login("alice").await;
    assert_eq!(
        app.send(
            Method::POST,
            "/invites/redeem",
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
        me_a["household_id"].as_str().unwrap(),
        cabin_household.id.to_string()
    );
    assert_eq!(
        me_b["household_id"].as_str().unwrap(),
        home_household.to_string()
    );

    let switch_status = app
        .send(
            Method::POST,
            "/auth/switch-household",
            Some(json!({ "household_id": home_household })),
            Some(&alice_session_a),
        )
        .await;
    assert_eq!(switch_status.0, StatusCode::OK);
    assert_eq!(
        switch_status.1["household_id"].as_str().unwrap(),
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
        home_admin,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/stock/{}", home_batch.id),
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
            "/auth/switch-household",
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
    let admin = qm_db::users::create(&app.db, "owner2", Some("owner2@example.com"), "hash")
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
        "member",
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::POST,
            "/invites/redeem",
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
        me_after_removal["household_id"].as_str().unwrap(),
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
    assert!(me_after_last_removal["household_id"].is_null());
    assert!(me_after_last_removal["household_name"].is_null());
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
    assert!(me_before["household_id"].is_null());
    assert!(me_before["households"].as_array().unwrap().is_empty());

    let (status, created) = app
        .send(
            Method::POST,
            "/households",
            Some(json!({ "name": "Fresh Start", "timezone": "UTC" })),
            Some(&session_a),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let household_id = Uuid::parse_str(created["household_id"].as_str().unwrap()).unwrap();
    assert_eq!(created["household_name"].as_str().unwrap(), "Fresh Start");
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
        me_session_a["household_id"].as_str().unwrap(),
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
    assert!(me_after_removal["household_id"].is_null());

    let (status, created) = app
        .send(
            Method::POST,
            "/households",
            Some(json!({ "name": "Replacement Home", "timezone": "UTC" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        created["household_name"].as_str().unwrap(),
        "Replacement Home"
    );
    assert_eq!(created["households"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn logout_revokes_tokens_and_deletes_auth_session_row() {
    let app = TestApp::start(ApiConfig::default()).await;
    let _ = app.register("alice", None).await;
    let (status, body) = app
        .send(
            Method::POST,
            "/auth/login",
            Some(json!({
                "username": "alice",
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
        app.send(Method::POST, "/auth/logout", None, Some(access))
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
        app.send(Method::GET, "/auth/me", None, Some(expired_access))
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
        app.send(Method::GET, "/auth/me", None, Some(expired_access))
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
            "/auth/login",
            Some(json!({
                "username": "alice",
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
            "/auth/refresh",
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
            "/households/current/invites",
            Some(json!({
                "expires_at": "2999-01-01T00:00:00.000Z",
                "max_uses": 1,
                "role_granted": "member",
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
            "/households/current/members",
            None,
            Some(&alice),
        )
        .await
        .1;
    let bob_id = members
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user"]["username"] == "bob")
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/households/current/members/{bob_id}"),
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
            &format!("/households/current/members/{alice_id}"),
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
        alice_id,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/locations/{pantry_id}"),
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
            "/locations",
            Some(json!({ "name": "Overflow", "kind": "pantry" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/locations/{}", new_loc["id"].as_str().unwrap()),
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
        "member",
    )
    .await
    .unwrap();

    let household_b = qm_db::households::create(&app.db, "Cabin", "UTC")
        .await
        .unwrap();
    qm_db::locations::seed_defaults(&app.db, household_b.id)
        .await
        .unwrap();
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
    assert_eq!(
        me["household_id"].as_str().unwrap(),
        household_b.id.to_string()
    );

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
