use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
    Router,
};
use qm_api::{ApiConfig, AppState, RegistrationMode};
use qm_db::Database;
use serde_json::{json, Value};
use tower::util::ServiceExt;
use uuid::Uuid;

fn temp_db_url() -> String {
    format!("sqlite:///tmp/qm-api-{}.db?mode=rwc", Uuid::now_v7())
}

async fn start_app(config: ApiConfig) -> (Router, Database) {
    let db = Database::connect(&temp_db_url()).await.unwrap();
    db.migrate().await.unwrap();
    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
        http: reqwest::Client::new(),
    };
    (qm_api::router(state), db)
}

async fn send(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let req = req
        .body(match body {
            Some(value) => Body::from(serde_json::to_vec(&value).unwrap()),
            None => Body::empty(),
        })
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

async fn register(app: &Router, username: &str, invite_code: Option<&str>) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        "/auth/register",
        Some(json!({
            "username": username,
            "password": "password123",
            "email": format!("{username}@example.com"),
            "invite_code": invite_code,
        })),
        None,
    )
    .await
}

async fn login(app: &Router, username: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        "/auth/login",
        Some(json!({
            "username": username,
            "password": "password123",
        })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body["access_token"].as_str().unwrap().to_owned()
}

async fn seed_household_admin(db: &Database, username: &str) -> (Uuid, Uuid) {
    let household = qm_db::households::create(db, "Home").await.unwrap();
    qm_db::locations::seed_defaults(db, household.id).await.unwrap();
    let hash = qm_api::auth::hash_password("password123").unwrap();
    let user = qm_db::users::create(db, username, Some(&format!("{username}@example.com")), &hash)
        .await
        .unwrap();
    qm_db::memberships::insert(db, household.id, user.id, "admin")
        .await
        .unwrap();
    (household.id, user.id)
}

#[tokio::test]
async fn first_run_only_bootstraps_once() {
    let (app, _) = start_app(ApiConfig::default()).await;
    assert_eq!(register(&app, "alice", None).await.0, StatusCode::CREATED);
    assert_eq!(register(&app, "bob", None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn open_registration_creates_distinct_households() {
    let (app, _) = start_app(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(register(&app, "alice", None).await.0, StatusCode::CREATED);
    assert_eq!(register(&app, "bob", None).await.0, StatusCode::CREATED);

    let alice = login(&app, "alice").await;
    let bob = login(&app, "bob").await;
    let alice_me = send(&app, Method::GET, "/auth/me", None, Some(&alice)).await.1;
    let bob_me = send(&app, Method::GET, "/auth/me", None, Some(&bob)).await.1;
    assert_ne!(alice_me["household_id"], bob_me["household_id"]);
}

#[tokio::test]
async fn invite_admin_flow_and_registration_work() {
    let (app, db) = start_app(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, _) = seed_household_admin(&db, "alice").await;
    let alice = login(&app, "alice").await;

    let (status, invite) = send(
        &app,
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

    let (status, list) = send(
        &app,
        Method::GET,
        "/households/current/invites",
        None,
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);

    assert_eq!(register(&app, "bob", Some(&code)).await.0, StatusCode::CREATED);
    let bob = login(&app, "bob").await;
    let me = send(&app, Method::GET, "/auth/me", None, Some(&bob)).await.1;
    assert_eq!(me["household_id"].as_str().unwrap(), household_id.to_string());
    assert_eq!(register(&app, "carol", Some(&code)).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn revoke_invite_and_existing_user_redeem_flow() {
    let (app, db) = start_app(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (target_household, _) = seed_household_admin(&db, "alice").await;
    let alice = login(&app, "alice").await;

    let (_, invite) = send(
        &app,
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

    let _ = seed_household_admin(&db, "bob").await;
    let bob = login(&app, "bob").await;
    let (status, _) = send(
        &app,
        Method::POST,
        "/invites/redeem",
        Some(json!({ "invite_code": code })),
        Some(&bob),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let me = send(&app, Method::GET, "/auth/me", None, Some(&bob)).await.1;
    assert_eq!(me["household_id"].as_str().unwrap(), target_household.to_string());

    let (_, invite2) = send(
        &app,
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
        send(&app, Method::DELETE, &format!("/invites/{invite_id}"), None, Some(&alice))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(register(&app, "carol", Some(invite_code)).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn redeeming_same_household_invite_is_idempotent() {
    let (app, db) = start_app(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (target_household, _) = seed_household_admin(&db, "alice").await;
    let alice = login(&app, "alice").await;

    let (_, invite) = send(
        &app,
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

    let _ = seed_household_admin(&db, "bob").await;
    let bob = login(&app, "bob").await;
    assert_eq!(
        send(
            &app,
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
        send(
            &app,
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": invite["code"].as_str().unwrap() })),
            Some(&bob),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me = send(&app, Method::GET, "/auth/me", None, Some(&bob)).await.1;
    assert_eq!(me["household_id"].as_str().unwrap(), target_household.to_string());
    let invite_row = qm_db::invites::find_by_id(&db, invite_id).await.unwrap().unwrap();
    assert_eq!(invite_row.use_count, 1);
}

#[tokio::test]
async fn invalid_invite_registration_does_not_create_orphaned_user() {
    let (app, db) = start_app(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (_, _) = seed_household_admin(&db, "alice").await;
    let alice = login(&app, "alice").await;

    let (_, invite) = send(
        &app,
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

    assert_eq!(register(&app, "bob", Some(&code)).await.0, StatusCode::CREATED);
    assert_eq!(register(&app, "carol", Some(&code)).await.0, StatusCode::BAD_REQUEST);
    assert!(qm_db::users::find_by_username(&db, "carol").await.unwrap().is_none());
}

#[tokio::test]
async fn current_household_uses_latest_joined_then_household_id_tiebreak() {
    let (app, db) = start_app(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (older_household, _) = seed_household_admin(&db, "alice").await;
    let alice = login(&app, "alice").await;

    let newer_household = qm_db::households::create(&db, "Cabin").await.unwrap();
    qm_db::locations::seed_defaults(&db, newer_household.id).await.unwrap();
    let admin = qm_db::users::create(&db, "owner2", Some("owner2@example.com"), "hash")
        .await
        .unwrap();
    qm_db::memberships::insert(&db, newer_household.id, admin.id, "admin")
        .await
        .unwrap();

    let invite = qm_db::invites::create(
        &db,
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
        send(
            &app,
            Method::POST,
            "/invites/redeem",
            Some(json!({ "invite_code": invite.code })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let me = send(&app, Method::GET, "/auth/me", None, Some(&alice)).await.1;
    assert_eq!(me["household_id"].as_str().unwrap(), newer_household.id.to_string());

    sqlx::query("UPDATE membership SET joined_at = ? WHERE user_id = ?")
        .bind("2026-01-01T00:00:00.000Z")
        .bind(me["user"]["id"].as_str().unwrap())
        .execute(&db.pool)
        .await
        .unwrap();

    let me_after_tie = send(&app, Method::GET, "/auth/me", None, Some(&alice)).await.1;
    let expected = if newer_household.id > older_household {
        newer_household.id
    } else {
        older_household
    };
    assert_eq!(me_after_tie["household_id"].as_str().unwrap(), expected.to_string());

    qm_db::memberships::remove(&db, newer_household.id, Uuid::parse_str(me["user"]["id"].as_str().unwrap()).unwrap())
        .await
        .unwrap();
    let me_after_removal = send(&app, Method::GET, "/auth/me", None, Some(&alice)).await.1;
    assert_eq!(me_after_removal["household_id"].as_str().unwrap(), older_household.to_string());
}

#[tokio::test]
async fn member_removal_and_location_deletion_guards_work() {
    let (app, db) = start_app(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, alice_id) = seed_household_admin(&db, "alice").await;
    let alice = login(&app, "alice").await;

    let (_, invite) = send(
        &app,
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
    assert_eq!(register(&app, "bob", Some(&code)).await.0, StatusCode::CREATED);

    let members = send(
        &app,
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
        send(
            &app,
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
        send(
            &app,
            Method::DELETE,
            &format!("/households/current/members/{alice_id}"),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    let pantry_id = qm_db::locations::list_for_household(&db, household_id)
        .await
        .unwrap()[0]
        .id;
    let product = qm_db::products::create_manual(&db, household_id, "Rice", None, "mass", Some("g"), None, None)
        .await
        .unwrap();
    qm_db::stock::create(&db, household_id, product.id, pantry_id, "100", "g", None, None, None, alice_id)
        .await
        .unwrap();
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/locations/{pantry_id}"),
            None,
            Some(&alice),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    let (status, new_loc) = send(
        &app,
        Method::POST,
        "/locations",
        Some(json!({ "name": "Overflow", "kind": "pantry" })),
        Some(&alice),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        send(
            &app,
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
