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
            Some(invite_body(1)),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let code = invite["code"].as_str().unwrap().to_owned();

    let (status, list) = app
        .send(
            Method::GET,
            "/households/current/invites",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    assert_eq!(
        app.register("bob", Some(&code)).await.0,
        StatusCode::CREATED
    );
    let bob = app.login("bob").await;
    let me = app.me(&bob).await;
    assert_eq!(
        me["household_id"].as_str().unwrap(),
        household_id.to_string()
    );
    assert_eq!(
        app.register("carol", Some(&code)).await.0,
        StatusCode::BAD_REQUEST
    );
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
            Some(invite_body(2)),
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
    assert_eq!(
        me["household_id"].as_str().unwrap(),
        target_household.to_string()
    );

    let (_, invite2) = app
        .send(
            Method::POST,
            "/households/current/invites",
            Some(invite_body(1)),
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
    assert_eq!(
        app.register("carol", Some(invite_code)).await.0,
        StatusCode::BAD_REQUEST
    );
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
            Some(invite_body(2)),
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
    assert_eq!(
        me["household_id"].as_str().unwrap(),
        target_household.to_string()
    );
    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id)
        .await
        .unwrap()
        .unwrap();
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
            Some(invite_body(1)),
            Some(&alice),
        )
        .await;
    let code = invite["code"].as_str().unwrap().to_owned();

    assert_eq!(
        app.register("bob", Some(&code)).await.0,
        StatusCode::CREATED
    );
    assert_eq!(
        app.register("carol", Some(&code)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert!(qm_db::users::find_by_username(&app.db, "carol")
        .await
        .unwrap()
        .is_none());
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
        .send(
            Method::POST,
            "/households/current/invites",
            Some(invite_body(2)),
            Some(&alice),
        )
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
    assert_eq!(
        me["household_id"].as_str().unwrap(),
        target_household.to_string()
    );
    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(invite_row.use_count, 1);
    assert!(qm_db::memberships::find(
        &app.db,
        target_household,
        Uuid::parse_str(me["user"]["id"].as_str().unwrap()).unwrap(),
    )
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
        .send(
            Method::POST,
            "/households/current/invites",
            Some(invite_body(1)),
            Some(&alice),
        )
        .await;
    let code = invite["code"].as_str().unwrap().to_owned();
    let invite_id = Uuid::parse_str(invite["id"].as_str().unwrap()).unwrap();

    let results = tokio::join!(
        app.register("bob", Some(&code)),
        app.register("carol", Some(&code))
    );
    let statuses = [results.0 .0, results.1 .0];
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CREATED)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s != StatusCode::CREATED)
            .count(),
        1
    );

    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(invite_row.use_count, 1);
    assert_eq!(qm_db::users::count(&app.db).await.unwrap(), 2);
    let bob_exists = qm_db::users::find_by_username(&app.db, "bob")
        .await
        .unwrap()
        .is_some();
    let carol_exists = qm_db::users::find_by_username(&app.db, "carol")
        .await
        .unwrap()
        .is_some();
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
        .send(
            Method::POST,
            "/households/current/invites",
            Some(invite_body(2)),
            Some(&alice),
        )
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
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": code.clone() })),
            Some(&bob),
        ),
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": code.clone() })),
            Some(&carol),
        ),
        app.send(
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": code })),
            Some(&dave),
        ),
    );
    let statuses = [r1.0, r2.0, r3.0];
    let success_count = statuses
        .iter()
        .filter(|s| **s == StatusCode::NO_CONTENT)
        .count();
    assert!(success_count >= 1);
    assert!(success_count <= 2);

    let invite_row = qm_db::invites::find_by_id(&app.db, invite_id)
        .await
        .unwrap()
        .unwrap();
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
