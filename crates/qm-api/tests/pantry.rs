mod support;

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::http::{Method, StatusCode};
use qm_ai::{
    AiConfig, AiError, AiProvider, AiProviderKind, AiProviderStatus, OpenRouterConfig,
    StructuredOutputRequest, StructuredOutputResponse,
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
    let captured_request = Arc::new(Mutex::new(None));
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
            captured_request: Some(Arc::clone(&captured_request)),
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

    let request = captured_request.lock().unwrap().clone().unwrap();
    assert_strict_schema_objects(&request.json_schema, "$");
    assert_eq!(
        request.json_schema.pointer("/properties/ideas/minItems"),
        Some(&Value::from(1))
    );
    assert_eq!(
        request
            .json_schema
            .pointer("/properties/ideas/items/properties/steps/minItems"),
        Some(&Value::from(1))
    );
    assert_eq!(
        request
            .json_schema
            .pointer("/properties/ideas/items/properties/ingredients/maxItems"),
        Some(&Value::from(8))
    );
    assert_eq!(
        request
            .json_schema
            .pointer("/properties/ideas/items/properties/steps/maxItems"),
        Some(&Value::from(6))
    );
    assert_eq!(
        request
            .json_schema
            .pointer("/properties/ideas/items/properties/ingredients/items/additionalProperties"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        request
            .json_schema
            .pointer("/properties/ideas/items/properties/steps/items/additionalProperties"),
        Some(&Value::Bool(false))
    );
    assert_eq!(request.max_output_tokens, Some(12_000));
    assert!(request.user_prompt.contains("\"name\":\"Rice\""));
    assert!(!request.user_prompt.contains(&rice.to_string()));
    assert!(!request.user_prompt.contains("image_url"));
}

#[tokio::test]
async fn pantry_generation_uses_configured_output_token_budget() {
    let captured_request = Arc::new(Mutex::new(None));
    let app = TestApp::start_with_ai_provider(
        ApiConfig {
            ai_pantry_suggestion_max_output_tokens: 12_345,
            ..ApiConfig::default()
        },
        Arc::new(MockAiProvider {
            output: json!({
                "ideas": [generated_idea_json("Pantry rice bowl", "2")]
            }),
            captured_request: Some(Arc::clone(&captured_request)),
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
    assert!(body["warnings"].as_array().unwrap().is_empty());
    assert_eq!(body["suggestions"].as_array().unwrap().len(), 1);
    assert_eq!(body["suggestions"][0]["title"], "Pantry rice bowl");

    let request = captured_request.lock().unwrap().clone().unwrap();
    assert_eq!(request.max_output_tokens, Some(12_345));
}

#[tokio::test]
async fn pantry_generation_rejections_are_reported_as_warnings() {
    let app = TestApp::start_with_ai_provider(
        ApiConfig::default(),
        Arc::new(MockAiProvider {
            output: json!({ "ideas": [] }),
            captured_request: None,
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
    assert!(body["suggestions"].as_array().unwrap().is_empty());
    assert_eq!(
        body["warnings"][0].as_str(),
        Some("AI recipe generation returned invalid candidates: ideas must include at least one recipe candidate")
    );
}

#[tokio::test]
async fn pantry_generation_keeps_valid_candidates_when_others_are_rejected() {
    let app = TestApp::start_with_ai_provider(
        ApiConfig::default(),
        Arc::new(MockAiProvider {
            output: json!({
                "ideas": [
                    generated_idea_json("Pantry rice bowl", "2"),
                    generated_idea_json("Range serving bowl", "2-3")
                ]
            }),
            captured_request: None,
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
    assert_eq!(body["suggestions"].as_array().unwrap().len(), 1);
    assert_eq!(body["suggestions"][0]["title"], "Pantry rice bowl");
    assert_eq!(
        body["warnings"][0].as_str(),
        Some("AI recipe generation returned invalid candidates: ideas[1].serving_count must be a positive decimal")
    );

    let (status, tasks) = app
        .send(Method::GET, "/api/v1/ai/tasks", None, Some(&alice))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tasks["items"][0]["validation_status"], "valid");
    assert_eq!(
        tasks["items"][0]["validation_errors"][0],
        "ideas[1].serving_count must be a positive decimal"
    );
}

#[tokio::test]
#[ignore = "requires QM_AI_OPENROUTER_API_KEY and makes a live OpenRouter request"]
async fn pantry_generation_live_openrouter_matches_route_usage() {
    let api_key = std::env::var("QM_AI_OPENROUTER_API_KEY")
        .expect("QM_AI_OPENROUTER_API_KEY must be set for the live OpenRouter pantry test");
    let model = std::env::var("QM_AI_MODEL").unwrap_or_else(|_| "openai/gpt-4.1-mini".into());
    let base_url = std::env::var("QM_AI_OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into());
    let timeout_seconds = std::env::var("QM_AI_TEST_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| ApiConfig::default().ai_timeout.as_secs());
    let ai_provider = qm_ai::build_provider(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .expect("HTTP client should build"),
        &AiConfig {
            provider: AiProviderKind::OpenRouter,
            model: Some(model),
            retain_raw_responses: false,
            openrouter: OpenRouterConfig {
                api_key: Some(api_key),
                base_url,
            },
        },
    )
    .expect("OpenRouter provider should build");
    let app = TestApp::start_with_ai_provider(ApiConfig::default(), ai_provider).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let (household_id, pantry_id) = household_and_pantry(&app, &alice).await;
    let actor = actor_id(&app, "alice").await;
    let rice = create_product(&app, household_id, "Rice", "mass", "g").await;
    let beans = create_product(&app, household_id, "Beans", "count", "piece").await;
    let tomatoes = create_product(&app, household_id, "Tomatoes", "count", "piece").await;
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
        actor,
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
        actor,
        None,
    )
    .await
    .unwrap();
    qm_db::stock::create(
        &app.db,
        household_id,
        tomatoes,
        pantry_id,
        "3",
        "piece",
        None,
        None,
        None,
        None,
        actor,
        None,
    )
    .await
    .unwrap();
    let extra_products = std::env::var("QM_AI_TEST_EXTRA_PRODUCTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let extra_batches_per_product = std::env::var("QM_AI_TEST_EXTRA_BATCHES_PER_PRODUCT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let extra_name_words = std::env::var("QM_AI_TEST_EXTRA_NAME_WORDS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    for idx in 0..extra_products {
        let mut name = format!("Live test pantry item {idx:03}");
        for word_idx in 0..extra_name_words {
            name.push_str(&format!(" descriptive-word-{word_idx:02}"));
        }
        let product = create_product(&app, household_id, &name, "count", "piece").await;
        for batch_idx in 0..extra_batches_per_product {
            qm_db::stock::create(
                &app.db,
                household_id,
                product,
                pantry_id,
                "1",
                "piece",
                None,
                (batch_idx % 2 == 0).then_some("2026-06-15"),
                None,
                None,
                actor,
                None,
            )
            .await
            .unwrap();
        }
    }
    eprintln!(
        "live pantry test using {} inventory products, {} extra batches/product, {} extra name words, timeout {}s",
        3 + extra_products,
        extra_batches_per_product,
        extra_name_words,
        timeout_seconds
    );

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/pantry/suggestions",
            Some(json!({
                "generate_recipe_ideas": true,
                "max_ai_suggestions": 3,
                "max_missing_required": 2
            })),
            Some(&alice),
        )
        .await;
    eprintln!("{}", serde_json::to_string_pretty(&body).unwrap());
    let (task_status, tasks) = app
        .send(Method::GET, "/api/v1/ai/tasks", None, Some(&alice))
        .await;
    assert_eq!(task_status, StatusCode::OK);
    eprintln!("{}", serde_json::to_string_pretty(&tasks).unwrap());
    assert_eq!(status, StatusCode::OK);
    assert!(body["generation_task"].as_str().is_some());
    assert!(body["suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["source"] == "ai_recipe"));
}

#[derive(Debug)]
struct MockAiProvider {
    output: Value,
    captured_request: Option<Arc<Mutex<Option<StructuredOutputRequest>>>>,
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
        request: StructuredOutputRequest,
    ) -> Pin<Box<dyn Future<Output = Result<StructuredOutputResponse, AiError>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(captured_request) = &self.captured_request {
                *captured_request.lock().unwrap() = Some(request);
            }
            Ok(StructuredOutputResponse {
                provider: AiProviderKind::OpenRouter,
                model: "mock/pantry".into(),
                output_json: self.output.clone(),
                raw_response_json: None,
            })
        })
    }
}

fn assert_strict_schema_objects(schema: &Value, path: &str) {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&Value::Bool(false)),
            "{path} must reject extra properties"
        );
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{path} must require all declared properties"));
        for key in properties.keys() {
            assert!(
                required.iter().any(|value| value.as_str() == Some(key)),
                "{path} must require {key}"
            );
        }
    }

    match schema {
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                assert_strict_schema_objects(item, &format!("{path}[{idx}]"));
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                assert_strict_schema_objects(value, &format!("{path}.{key}"));
            }
        }
        _ => {}
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

fn generated_idea_json(name: &str, serving_count: &str) -> Value {
    json!({
        "name": name,
        "description": "Simple bowl",
        "serving_count": serving_count,
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
    })
}
