mod support;

use std::{future::Future, pin::Pin, sync::Arc};

use axum::http::{Method, StatusCode};
use qm_ai::{
    AiError, AiProvider, AiProviderKind, AiProviderStatus, StructuredOutputRequest,
    StructuredOutputResponse,
};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::{json, Value};
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn pantry_suggestions_rank_saved_recipes_and_track_lifecycle() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let (household_id, pantry_id) = household_and_pantry(&app, &alice).await;

    let rice = create_product(&app, household_id, "Rice", "mass", "g").await;
    let beans = create_product(&app, household_id, "Beans", "count", "piece").await;
    let cheese = create_product(&app, household_id, "Cheese", "mass", "g").await;
    qm_db::stock::create(
        &app.db,
        household_id,
        rice,
        pantry_id,
        "500",
        "g",
        None,
        Some("2026-05-30"),
        None,
        None,
        actor_id(&app, "alice").await,
        None,
    )
    .await
    .unwrap();
    qm_db::stock::create(
        &app.db,
        household_id,
        beans,
        pantry_id,
        "2",
        "piece",
        None,
        None,
        None,
        None,
        actor_id(&app, "alice").await,
        None,
    )
    .await
    .unwrap();

    create_recipe(
        &app,
        &alice,
        "Rice and beans",
        vec![
            ingredient_json(rice, "Rice", "200", "g", false),
            ingredient_json(beans, "Beans", "1", "piece", false),
        ],
    )
    .await;
    create_recipe(
        &app,
        &alice,
        "Cheesy rice",
        vec![
            ingredient_json(rice, "Rice", "200", "g", false),
            ingredient_json(cheese, "Cheese", "50", "g", false),
        ],
    )
    .await;

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/pantry/suggestions",
            Some(json!({
                "max_missing_required": 2,
                "generate_recipe_ideas": false
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["context"]["inventory"].as_array().unwrap().len(), 2);
    assert_eq!(body["suggestions"][0]["title"], "Rice and beans");
    assert_eq!(body["suggestions"][0]["score_breakdown"]["cookable"], true);
    assert_eq!(body["suggestions"][1]["title"], "Cheesy rice");
    assert_eq!(
        body["suggestions"][1]["missing"][0]["display_name"],
        "Cheese"
    );

    let suggestion_id = body["suggestions"][0]["id"].as_str().unwrap();
    let (status, updated) = app
        .send(
            Method::PATCH,
            &format!("/api/v1/pantry/suggestions/{suggestion_id}/state"),
            Some(json!({ "status": "dismissed" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["status"], "dismissed");
}

#[tokio::test]
async fn pantry_suggestions_are_household_scoped() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::Open,
        ..ApiConfig::default()
    })
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    assert_eq!(app.register("bob", None).await.0, StatusCode::CREATED);
    let bob = app.login("bob").await;
    let (alice_household, pantry_id) = household_and_pantry(&app, &alice).await;
    let rice = create_product(&app, alice_household, "Rice", "mass", "g").await;
    qm_db::stock::create(
        &app.db,
        alice_household,
        rice,
        pantry_id,
        "500",
        "g",
        None,
        None,
        None,
        None,
        actor_id(&app, "alice").await,
        None,
    )
    .await
    .unwrap();
    create_recipe(
        &app,
        &alice,
        "Rice",
        vec![ingredient_json(rice, "Rice", "100", "g", false)],
    )
    .await;

    let suggestion = app
        .send(
            Method::POST,
            "/api/v1/pantry/suggestions",
            Some(json!({})),
            Some(&alice),
        )
        .await
        .1;
    let suggestion_id = suggestion["suggestions"][0]["id"].as_str().unwrap();
    let (status, _) = app
        .send(
            Method::GET,
            &format!("/api/v1/pantry/suggestions/{suggestion_id}"),
            None,
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pantry_generation_records_ai_task_and_candidate_suggestion() {
    let app = TestApp::start_with_ai_provider(
        ApiConfig::default(),
        Arc::new(MockAiProvider {
            output: json!({
                "ideas": [{
                    "name": "Pantry rice bowl",
                    "description": "Simple bowl",
                    "serving_count": "2",
                    "ingredients": [],
                    "steps": [{
                        "instruction": "Warm and serve.",
                        "timers": [],
                        "equipment": [],
                        "ingredient_refs": []
                    }],
                    "explanation": "Uses the pantry context.",
                    "unresolved_conversions": [],
                    "substitutions": []
                }]
            }),
        }),
    )
    .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let (household_id, pantry_id) = household_and_pantry(&app, &alice).await;
    let rice = create_product(&app, household_id, "Rice", "mass", "g").await;
    qm_db::stock::create(
        &app.db,
        household_id,
        rice,
        pantry_id,
        "500",
        "g",
        None,
        None,
        None,
        None,
        actor_id(&app, "alice").await,
        None,
    )
    .await
    .unwrap();

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/pantry/suggestions",
            Some(json!({ "generate_recipe_ideas": true })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["generation_task"].as_str().is_some());
    assert_eq!(
        body["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["source"] == "ai_recipe"),
        true
    );

    let (status, tasks) = app
        .send(Method::GET, "/api/v1/ai/tasks", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tasks["items"][0]["task_type"], "recipe_generation");
    assert_eq!(tasks["items"][0]["credentials_assertion"], true);
}

#[derive(Debug)]
struct MockAiProvider {
    output: Value,
}

impl AiProvider for MockAiProvider {
    fn status(&self) -> AiProviderStatus {
        AiProviderStatus {
            provider: AiProviderKind::OpenRouter,
            enabled: true,
            configured: true,
            model: Some("mock/pantry".into()),
            structured_outputs: true,
            raw_response_retention: false,
        }
    }

    fn complete_structured<'a>(
        &'a self,
        _request: StructuredOutputRequest,
    ) -> Pin<Box<dyn Future<Output = Result<StructuredOutputResponse, AiError>> + Send + 'a>> {
        Box::pin(async move {
            Ok(StructuredOutputResponse {
                provider: AiProviderKind::OpenRouter,
                model: "mock/pantry".into(),
                output_json: self.output.clone(),
                raw_response_json: None,
            })
        })
    }
}

async fn household_and_pantry(app: &TestApp, bearer: &str) -> (Uuid, Uuid) {
    let me = app.me(bearer).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let pantry = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap();
    (household_id, pantry.id)
}

async fn actor_id(app: &TestApp, username: &str) -> Uuid {
    let row: (String,) = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(format!("{username}@example.com"))
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    Uuid::parse_str(&row.0).unwrap()
}

async fn create_product(
    app: &TestApp,
    household_id: Uuid,
    name: &str,
    family: &str,
    preferred_unit: &str,
) -> Uuid {
    qm_db::products::create_manual(
        &app.db,
        household_id,
        name,
        None,
        family,
        Some(preferred_unit),
        None,
        None,
    )
    .await
    .unwrap()
    .id
}

async fn create_recipe(app: &TestApp, bearer: &str, name: &str, ingredients: Vec<Value>) {
    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/recipes",
            Some(json!({
                "name": name,
                "description": null,
                "serving_count": "2",
                "source": "manual",
                "visibility": "household",
                "tags": [],
                "source_text": null,
                "ingredients": ingredients,
                "steps": [{
                    "instruction": "Cook until ready.",
                    "timers": [],
                    "equipment": [],
                    "ingredient_refs": []
                }],
                "outputs": [],
                "provenance": []
            })),
            Some(bearer),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

fn ingredient_json(
    product_id: Uuid,
    display_name: &str,
    amount: &str,
    unit: &str,
    optional: bool,
) -> Value {
    json!({
        "ingredient_id": null,
        "product_id": product_id,
        "display_name": display_name,
        "quantity": {
            "amount": amount,
            "unit": unit,
            "family": null,
            "range": null,
            "to_taste": false,
            "preparation_note": null
        },
        "preparation": null,
        "optional": optional,
        "group_label": null,
        "substitution_hints": []
    })
}
