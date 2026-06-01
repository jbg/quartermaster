mod support;

use std::{future::Future, pin::Pin, sync::Arc};

use axum::http::{Method, StatusCode};
use qm_ai::{
    AiError, AiProvider, AiProviderKind, AiProviderStatus, StructuredOutputRequest,
    StructuredOutputResponse,
};
use qm_api::ApiConfig;
use serde_json::json;
use sqlx::Row;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn meal_plan_generates_non_contiguous_dates_and_reserves_sequentially() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let actor = Uuid::parse_str(me["user"]["id"].as_str().unwrap()).unwrap();
    let pantry = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|location| location.kind == "pantry")
        .unwrap();
    let rice = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Plan Rice",
        None,
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();
    let batch = qm_db::stock::create(
        &app.db,
        household_id,
        rice.id,
        pantry.id,
        "100",
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

    let (status, recipe) = app
        .send(
            Method::POST,
            "/api/v1/recipes",
            Some(json!({
                "name": "Rice bowl",
                "description": null,
                "serving_count": "1",
                "source": "manual",
                "visibility": "household",
                "tags": [],
                "source_text": null,
                "ingredients": [{
                    "ingredient_id": null,
                    "product_id": rice.id,
                    "display_name": "Rice",
                    "quantity": {
                        "amount": "80",
                        "unit": "g",
                        "family": "mass",
                        "range": null,
                        "to_taste": false,
                        "preparation_note": null
                    },
                    "preparation": null,
                    "optional": false,
                    "group_label": null,
                    "substitution_hints": []
                }],
                "steps": [{ "instruction": "Cook rice.", "timers": [], "equipment": [], "ingredient_refs": [] }],
                "outputs": [],
                "provenance": []
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recipe["name"], "Rice bowl");

    let (status, plan) = app
        .send(
            Method::POST,
            "/api/v1/meal-plans/generate",
            Some(json!({
                "title": "Travel week",
                "dates": ["2026-06-02", "2026-06-04"],
                "slots": [{ "key": "dinner", "label": "Dinner" }],
                "constraints": {}
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(plan["days"].as_array().unwrap().len(), 2);
    assert_eq!(plan["days"][0]["date"], "2026-06-02");
    assert_eq!(plan["days"][1]["date"], "2026-06-04");
    assert_eq!(plan["days"][0]["meals"][0]["status"], "planned");
    assert_eq!(plan["days"][1]["meals"][0]["status"], "conflicted");
    assert_eq!(
        plan["days"][0]["meals"][0]["reservations"][0]["batch_id"],
        batch.id.to_string()
    );

    let row = sqlx::query("SELECT quantity FROM stock_batch WHERE id = ?")
        .bind(batch.id.to_string())
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("quantity"), "100");

    let plan_id = plan["id"].as_str().unwrap();
    let meal_id = plan["days"][0]["meals"][0]["id"].as_str().unwrap();
    let (status, cooked) = app
        .send(
            Method::POST,
            &format!("/api/v1/meal-plans/{plan_id}/meals/{meal_id}/execute"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cooked["plan"]["can_execute"], true);

    let row = sqlx::query("SELECT quantity FROM stock_batch WHERE id = ?")
        .bind(batch.id.to_string())
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("quantity"), "20");
}

#[tokio::test]
async fn meal_plan_ai_generation_saves_llm_recipe_and_reserves_stock() {
    let output = json!({
        "recipe": {
            "name": "AI Chickpea Supper",
            "description": "A generated pantry dinner.",
            "serving_count": "1",
            "ingredients": [{
                "id": null,
                "ingredient_id": null,
                "product_id": "__PRODUCT_ID__",
                "display_name": "Chickpeas",
                "quantity": {
                    "amount": "50",
                    "unit": "g",
                    "family": "mass",
                    "range": null,
                    "to_taste": false,
                    "preparation_note": null
                },
                "preparation": null,
                "optional": false,
                "group_label": null,
                "substitution_hints": []
            }],
            "steps": [{
                "id": null,
                "instruction": "Warm the chickpeas.",
                "timers": [],
                "equipment": [],
                "ingredient_refs": ["Chickpeas"]
            }],
            "explanation": "Uses pantry chickpeas.",
            "unresolved_conversions": [],
            "substitutions": []
        }
    });
    let app =
        TestApp::start_with_ai_provider(ApiConfig::default(), Arc::new(MockAiProvider { output }))
            .await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let me = app.me(&alice).await;
    let household_id = Uuid::parse_str(me_current_household_id(&me).unwrap()).unwrap();
    let actor = Uuid::parse_str(me["user"]["id"].as_str().unwrap()).unwrap();
    let pantry = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|location| location.kind == "pantry")
        .unwrap();
    let chickpeas = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Plan Chickpeas",
        None,
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();
    let batch = qm_db::stock::create(
        &app.db,
        household_id,
        chickpeas.id,
        pantry.id,
        "75",
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

    let (status, plan) = app
        .send(
            Method::POST,
            "/api/v1/meal-plans/generate",
            Some(json!({
                "title": "AI week",
                "dates": ["2026-06-02"],
                "slots": [{ "key": "dinner", "label": "Dinner" }],
                "constraints": { "diet": "vegetarian" }
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(plan["ai_task_id"].as_str().is_some());
    let meal = &plan["days"][0]["meals"][0];
    assert_eq!(meal["status"], "planned");
    assert_eq!(meal["recipe_name"], "AI Chickpea Supper");
    assert_eq!(meal["reservations"][0]["batch_id"], batch.id.to_string());

    let recipe_id = meal["recipe_id"].as_str().unwrap();
    let row = sqlx::query("SELECT source FROM recipe WHERE id = ?")
        .bind(recipe_id)
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("source"), "llm_generated");

    let row = sqlx::query(
        "SELECT source_type, prompt_version, model FROM recipe_provenance WHERE recipe_id = ?",
    )
    .bind(recipe_id)
    .fetch_one(&app.db.pool)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("source_type"), "llm");
    assert_eq!(
        row.get::<String, _>("prompt_version"),
        "meal-plan-recipe.v1"
    );
    assert_eq!(row.get::<String, _>("model"), "mock/meal-plan");
}

#[derive(Debug)]
struct MockAiProvider {
    output: serde_json::Value,
}

impl AiProvider for MockAiProvider {
    fn status(&self) -> AiProviderStatus {
        AiProviderStatus {
            provider: AiProviderKind::OpenRouter,
            enabled: true,
            configured: true,
            model: Some("mock/meal-plan".into()),
            structured_outputs: true,
            raw_response_retention: false,
        }
    }

    fn complete_structured<'a>(
        &'a self,
        request: StructuredOutputRequest,
    ) -> Pin<Box<dyn Future<Output = Result<StructuredOutputResponse, AiError>> + Send + 'a>> {
        Box::pin(async move {
            let product_id = serde_json::from_str::<serde_json::Value>(&request.user_prompt)
                .ok()
                .and_then(|value| {
                    value["inventory"][0]["product_id"]
                        .as_str()
                        .map(str::to_owned)
                })
                .unwrap();
            let mut output = self.output.clone();
            replace_product_placeholder(&mut output, &product_id);
            Ok(StructuredOutputResponse {
                provider: AiProviderKind::OpenRouter,
                model: "mock/meal-plan".into(),
                output_json: output,
                raw_response_json: None,
            })
        })
    }
}

fn replace_product_placeholder(value: &mut serde_json::Value, product_id: &str) {
    match value {
        serde_json::Value::String(current) if current == "__PRODUCT_ID__" => {
            *current = product_id.to_owned();
        }
        serde_json::Value::Array(items) => {
            for item in items {
                replace_product_placeholder(item, product_id);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values_mut() {
                replace_product_placeholder(item, product_id);
            }
        }
        _ => {}
    }
}
