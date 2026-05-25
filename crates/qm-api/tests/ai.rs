mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::json;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn ai_status_is_disabled_by_default() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let (status, body) = app
        .send(Method::GET, "/api/v1/ai/status", None, Some(&alice))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["provider"], "disabled");
    assert_eq!(body["enabled"], false);
    assert_eq!(body["configured"], true);
    assert_eq!(body["structured_outputs"], false);
}

#[tokio::test]
async fn ai_tasks_are_household_scoped_and_stateful() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let alice_household =
        Uuid::parse_str(me_current_household_id(&app.me(&alice).await).unwrap()).unwrap();

    assert_eq!(app.register("bob", None).await.0, StatusCode::CREATED);
    let bob = app.login("bob").await;
    let bob_household =
        Uuid::parse_str(me_current_household_id(&app.me(&bob).await).unwrap()).unwrap();

    let task = qm_db::ai_tasks::create(
        &app.db,
        alice_household,
        &qm_db::ai_tasks::NewAiTask {
            created_by: None,
            task_type: "recipe_generation",
            provider: "openrouter",
            model: Some("openai/gpt-4.1-mini"),
            prompt_version: "recipe-generation.v1",
            input_digest: "sha256:abc123",
            input_summary_json: r#"{"ingredients":["beans","rice"]}"#,
            output_json: Some(r#"{"name":"Beans and Rice"}"#),
            validation_status: "valid",
            validation_errors_json: "[]",
            user_state: "proposed",
            credentials_assertion: true,
            raw_response_json: None,
        },
    )
    .await
    .unwrap();
    qm_db::ai_tasks::create(
        &app.db,
        bob_household,
        &qm_db::ai_tasks::NewAiTask {
            created_by: None,
            task_type: "pantry_suggestion",
            provider: "openrouter",
            model: Some("openai/gpt-4.1-mini"),
            prompt_version: "pantry-suggestion.v1",
            input_digest: "sha256:def456",
            input_summary_json: r#"{"ingredients":["eggs"]}"#,
            output_json: None,
            validation_status: "pending",
            validation_errors_json: "[]",
            user_state: "proposed",
            credentials_assertion: true,
            raw_response_json: None,
        },
    )
    .await
    .unwrap();

    let (status, list) = app
        .send(Method::GET, "/api/v1/ai/tasks", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["id"], task.id.to_string());
    assert_eq!(list["items"][0]["task_type"], "recipe_generation");
    assert_eq!(list["items"][0]["user_state"], "proposed");

    let (status, body) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/ai/tasks/{}/state", task.id),
            Some(json!({ "user_state": "accepted" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user_state"], "accepted");

    let (status, _) = app
        .send(
            Method::GET,
            &format!("/api/v1/ai/tasks/{}", task.id),
            None,
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
