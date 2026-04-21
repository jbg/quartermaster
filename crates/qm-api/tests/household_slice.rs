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
async fn invite_admin_flow_and_registration_work() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (status, invite) = app
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
    assert_eq!(status, StatusCode::CREATED);
    let code = invite["code"].as_str().unwrap().to_owned();

    let (status, list) = app
        .send(Method::GET, "/households/current/invites", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    assert_eq!(app.register("bob", Some(&code)).await.0, StatusCode::CREATED);
    let bob = app.login("bob").await;
    let me = app.me(&bob).await;
    assert_eq!(me["household_id"].as_str().unwrap(), household_id.to_string());
    assert_eq!(app.register("carol", Some(&code)).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn revoke_invite_and_existing_user_redeem_flow() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (target_household, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (_, invite) = app
        .send(
            Method::POST,
            "/households/current/invites",
            Some(json!({
                "expires_at": "2999-01-01T00:00:00.000Z",
                "max_uses": 2,
                "role_granted": "member",
            })),
            Some(&alice),
        )
        .await;
    let code = invite["code"].as_str().unwrap().to_owned();

    let _ = app.seed_household_admin("bob").await;
    let bob = app.login("bob").await;
    let (status, _) = app
        .send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": code })),
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let me = app.me(&bob).await;
    assert_eq!(me["household_id"].as_str().unwrap(), target_household.to_string());

    let (_, invite2) = app
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
    let invite_id = invite2["id"].as_str().unwrap();
    let invite_code = invite2["code"].as_str().unwrap();
    assert_eq!(
        app.send(
            Method::DELETE,
            &format!("/invites/{invite_id}"),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(app.register("carol", Some(invite_code)).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn redeeming_same_household_invite_is_idempotent() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (target_household, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let (_, invite) = app
        .send(
            Method::POST,
            "/households/current/invites",
            Some(json!({
                "expires_at": "2999-01-01T00:00:00.000Z",
                "max_uses": 2,
                "role_granted": "member",
            })),
            Some(&alice),
        )
        .await;
    let invite_id = Uuid::parse_str(invite["id"].as_str().unwrap()).unwrap();
    let code = invite["code"].as_str().unwrap().to_owned();

    let _ = app.seed_household_admin("bob").await;
    let bob = app.login("bob").await;
    assert_eq!(
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": code })),
            Some(&bob),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": invite["code"].as_str().unwrap() })),
            Some(&bob),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me = app.me(&bob).await;
    assert_eq!(me["household_id"].as_str().unwrap(), target_household.to_string());
    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id).await.unwrap().unwrap();
    assert_eq!(invite_row.use_count, 1);
}

#[tokio::test]
async fn invalid_invite_registration_does_not_create_orphaned_user() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let _ = app.seed_household_admin("alice").await;
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

    assert_eq!(app.register("bob", Some(&code)).await.0, StatusCode::CREATED);
    assert_eq!(app.register("carol", Some(&code)).await.0, StatusCode::BAD_REQUEST);
    assert!(qm_db::users::find_by_username(&app.db, "carol").await.unwrap().is_none());
}

#[tokio::test]
async fn current_household_uses_latest_joined_then_household_id_tiebreak() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (older_household, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;

    let newer_household = qm_db::households::create(&app.db, "Cabin").await.unwrap();
    qm_db::locations::seed_defaults(&app.db, newer_household.id).await.unwrap();
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
    assert_eq!(me["household_id"].as_str().unwrap(), newer_household.id.to_string());

    sqlx::query("UPDATE membership SET joined_at = ? WHERE user_id = ?")
        .bind("2026-01-01T00:00:00.000Z")
        .bind(me["user"]["id"].as_str().unwrap())
        .execute(&app.db.pool)
        .await
        .unwrap();

    let me_after_tie = app.me(&alice).await;
    let expected = if newer_household.id > older_household {
        newer_household.id
    } else {
        older_household
    };
    assert_eq!(me_after_tie["household_id"].as_str().unwrap(), expected.to_string());

    qm_db::memberships::remove(
        &app.db,
        newer_household.id,
        Uuid::parse_str(me["user"]["id"].as_str().unwrap()).unwrap(),
    )
    .await
    .unwrap();
    let me_after_removal = app.me(&alice).await;
    assert_eq!(
        me_after_removal["household_id"].as_str().unwrap(),
        older_household.to_string()
    );
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
    assert_eq!(app.register("bob", Some(&code)).await.0, StatusCode::CREATED);

    let members = app
        .send(Method::GET, "/households/current/members", None, Some(&alice))
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
