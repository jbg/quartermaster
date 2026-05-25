mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use rust_decimal::Decimal;
use serde_json::json;
use sqlx::Row;
use std::str::FromStr;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn recipe_crud_validates_scales_and_versions_structured_recipe_data() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let household_id =
        Uuid::parse_str(me_current_household_id(&app.me(&alice).await).unwrap()).unwrap();

    let product = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Bread Flour",
        None,
        "mass",
        Some("g"),
        None,
        None,
    )
    .await
    .unwrap();
    let ingredient = qm_db::ingredients::create(
        &app.db,
        household_id,
        &qm_db::ingredients::NewIngredient {
            display_name: "Flour",
            category: Some("baking"),
            default_family: Some("mass"),
            aliases_json: "[]",
            dietary_tags_json: "[]",
            allergen_tags_json: "[\"wheat\"]",
            notes: None,
        },
    )
    .await
    .unwrap();

    let (status, recipe) = app
        .send(
            Method::POST,
            "/api/v1/recipes",
            Some(json!({
                "name": "Flatbread",
                "description": "Simple pantry bread",
                "serving_count": "2",
                "source": "structured_json_import",
                "visibility": "household",
                "tags": ["bread", "pantry"],
                "source_text": "Mix flour and water. Cook in a pan.",
                "ingredients": [{
                    "ingredient_id": ingredient.id,
                    "product_id": product.id,
                    "display_name": "Flour",
                    "quantity": {
                        "amount": "200",
                        "unit": "g",
                        "family": "mass",
                        "range": null,
                        "to_taste": false,
                        "preparation_note": null
                    },
                    "preparation": "sifted",
                    "optional": false,
                    "group_label": "Dough",
                    "substitution_hints": ["wholemeal flour"]
                }],
                "steps": [{
                    "instruction": "Mix, rest, and cook in a hot pan.",
                    "timers": [{ "label": "rest", "duration_seconds": 600 }],
                    "equipment": ["pan"],
                    "ingredient_refs": ["Flour"]
                }],
                "outputs": [{
                    "product_id": null,
                    "name": "Flatbread",
                    "quantity": {
                        "amount": "2",
                        "unit": "piece",
                        "family": "count",
                        "range": null,
                        "to_taste": false,
                        "preparation_note": null
                    },
                    "expires_after_days": 2,
                    "storage_notes": "Room temperature"
                }],
                "provenance": [{
                    "source_type": "structured_json",
                    "imported_url": null,
                    "imported_file_name": null,
                    "imported_text": null,
                    "prompt_version": null,
                    "model": null,
                    "user_edits": ["normalized ingredient"],
                    "parser_confidence": "0.9"
                }]
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recipe["name"], "Flatbread");
    assert_eq!(recipe["version"]["version_number"], 1);
    assert_eq!(recipe["validation"]["valid"], true);
    let recipe_id = recipe["id"].as_str().unwrap();

    let (status, scaled) = app
        .send(
            Method::POST,
            &format!("/api/v1/recipes/{recipe_id}/scale"),
            Some(json!({ "serving_count": "4" })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(scaled["ingredients"][0]["scaled_quantity"]["amount"], "400");

    let (status, updated) = app
        .send(
            Method::PUT,
            &format!("/api/v1/recipes/{recipe_id}"),
            Some(json!({
                "name": "Flatbread",
                "description": "Simple pantry bread",
                "serving_count": "4",
                "source": "manual",
                "visibility": "household",
                "tags": ["bread"],
                "source_text": null,
                "ingredients": [{
                    "ingredient_id": null,
                    "product_id": null,
                    "display_name": "Salt",
                    "quantity": {
                        "amount": null,
                        "unit": null,
                        "family": null,
                        "range": null,
                        "to_taste": true,
                        "preparation_note": null
                    },
                    "preparation": null,
                    "optional": true,
                    "group_label": null,
                    "substitution_hints": []
                }],
                "steps": [{
                    "instruction": "Season to taste.",
                    "timers": [],
                    "equipment": [],
                    "ingredient_refs": ["Salt"]
                }],
                "outputs": [],
                "provenance": []
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["version"]["version_number"], 2);
    assert_eq!(updated["validation"]["valid"], true);
    assert_eq!(
        updated["validation"]["warnings"][0]["code"],
        "unresolved_ingredient"
    );
}

#[tokio::test]
async fn plain_text_recipe_import_creates_versioned_recipe_with_provenance() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;

    let (status, recipe) = app
        .send(
            Method::POST,
            "/api/v1/recipes/imports/text",
            Some(json!({
                "name": "Soup note",
                "text": "Soup note\nSimmer everything until done.",
                "serving_count": "3",
                "tags": ["soup"]
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(recipe["source"], "plain_text_import");
    assert_eq!(recipe["version"]["steps"].as_array().unwrap().len(), 1);
    assert_eq!(
        recipe["version"]["provenance"][0]["source_type"],
        "plain_text_paste"
    );
}

#[tokio::test]
async fn household_export_import_round_trips_ingredients_and_recipes() {
    let app = TestApp::start(ApiConfig {
        registration_mode: RegistrationMode::InviteOnly,
        ..ApiConfig::default()
    })
    .await;
    let (household_id, _) = app.seed_household_admin("alice").await;
    let alice = app.login("alice").await;
    app.seed_user_without_household("bob").await;
    let bob = app.login("bob").await;

    let product = qm_db::products::create_manual(
        &app.db,
        household_id,
        "Tomatoes",
        None,
        "count",
        Some("piece"),
        None,
        None,
    )
    .await
    .unwrap();
    let ingredient = qm_db::ingredients::create(
        &app.db,
        household_id,
        &qm_db::ingredients::NewIngredient {
            display_name: "Tomato",
            category: Some("produce"),
            default_family: Some("count"),
            aliases_json: "[\"tomatoes\"]",
            dietary_tags_json: "[]",
            allergen_tags_json: "[]",
            notes: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        app.send(
            Method::POST,
            &format!("/api/v1/ingredients/{}/product-mappings", ingredient.id),
            Some(json!({
                "product_id": product.id,
                "rank": 0,
                "match_kind": "exact_product_link",
                "match_metadata": {},
                "conversion": null
            })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        app.send(
            Method::POST,
            "/api/v1/recipes",
            Some(json!({
                "name": "Tomato snack",
                "description": null,
                "serving_count": "1",
                "source": "manual",
                "visibility": "household",
                "tags": ["snack"],
                "source_text": null,
                "ingredients": [{
                    "ingredient_id": ingredient.id,
                    "product_id": product.id,
                    "display_name": "Tomato",
                    "quantity": {
                        "amount": "1",
                        "unit": "piece",
                        "family": "count",
                        "range": null,
                        "to_taste": false,
                        "preparation_note": null
                    },
                    "preparation": "sliced",
                    "optional": false,
                    "group_label": null,
                    "substitution_hints": []
                }],
                "steps": [{
                    "instruction": "Slice and season.",
                    "timers": [],
                    "equipment": ["knife"],
                    "ingredient_refs": ["Tomato"]
                }],
                "outputs": [],
                "provenance": []
            })),
            Some(&alice),
        )
        .await
        .0,
        StatusCode::CREATED
    );

    let (status, document) = app
        .send(
            Method::GET,
            "/api/v1/households/current/export",
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(document["ingredients"].as_array().unwrap().len(), 1);
    assert_eq!(
        document["ingredient_product_mappings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(document["recipes"].as_array().unwrap().len(), 1);

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/households/import",
            Some(document),
            Some(&bob),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, imported_recipes) = app
        .send(Method::GET, "/api/v1/recipes", None, Some(&bob))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(imported_recipes["items"].as_array().unwrap().len(), 1);
    assert_eq!(imported_recipes["items"][0]["name"], "Tomato snack");
}

#[tokio::test]
async fn recipe_execution_preflights_adjusted_recipe_and_is_idempotent() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let (household_id, pantry_id) = household_and_pantry(&app, &alice).await;

    let rice_id = create_product(&app, &alice, "Rice", "mass", "g").await;
    let leftovers_id = create_product(&app, &alice, "Cooked rice", "mass", "g").await;
    let first_batch = create_stock(
        &app,
        &alice,
        rice_id,
        pantry_id,
        "500",
        "g",
        Some("2026-06-01"),
    )
    .await;
    let second_batch = create_stock(
        &app,
        &alice,
        rice_id,
        pantry_id,
        "300",
        "g",
        Some("2026-06-10"),
    )
    .await;

    let (status, preflight) = app
        .send(
            Method::POST,
            "/api/v1/recipes/executions/preflight",
            Some(json!({
                "recipe_name": "Rice bowls",
                "serving_scale": "1",
                "ingredients": [{
                    "line_id": "rice",
                    "display_name": "rice",
                    "product_id": rice_id,
                    "quantity": "600",
                    "unit": "g"
                }],
                "outputs": [{
                    "product_id": leftovers_id,
                    "location_id": pantry_id,
                    "quantity": "100",
                    "unit": "g",
                    "expires_on": "2026-06-03"
                }]
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preflight["can_execute"], true);
    assert_eq!(
        preflight["ingredients"][0]["matched_batches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let (status, cooked) = app
        .send(
            Method::POST,
            "/api/v1/recipes/executions",
            Some(json!({
                "recipe_name": "Rice bowls",
                "serving_scale": "1",
                "idempotency_key": "cook-rice-once",
                "ingredients": [{
                    "line_id": "rice",
                    "display_name": "rice",
                    "product_id": rice_id,
                    "quantity": "450",
                    "unit": "g"
                }],
                "outputs": [{
                    "product_id": leftovers_id,
                    "location_id": pantry_id,
                    "quantity": "80",
                    "unit": "g",
                    "expires_on": "2026-06-03"
                }]
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cooked["idempotent_replay"], false);
    assert_eq!(
        cooked["plan"]["ingredients"][0]["requested_quantity"],
        "450"
    );
    assert_eq!(cooked["output_batches"][0]["quantity"], "80");
    let execution_id = cooked["execution_id"].as_str().unwrap();
    let consume_request_id = cooked["consume_request_id"].as_str().unwrap();

    assert_eq!(
        app.send(
            Method::GET,
            &format!("/api/v1/stock/{first_batch}"),
            None,
            Some(&alice),
        )
        .await
        .1["quantity"],
        "50"
    );
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/api/v1/stock/{second_batch}"),
            None,
            Some(&alice),
        )
        .await
        .1["quantity"],
        "300"
    );

    let events = qm_db::stock_events::list_for_batch(&app.db, first_batch)
        .await
        .unwrap();
    let consume_event = events
        .iter()
        .find(|event| event.event_type == "consume")
        .unwrap();
    assert_eq!(
        consume_event.recipe_execution_id.unwrap().to_string(),
        execution_id
    );
    assert_eq!(
        consume_event.consume_request_id.unwrap().to_string(),
        consume_request_id
    );

    let output_batch_id =
        Uuid::parse_str(cooked["output_batches"][0]["id"].as_str().unwrap()).unwrap();
    let output_events = qm_db::stock_events::list_for_batch(&app.db, output_batch_id)
        .await
        .unwrap();
    assert_eq!(
        output_events[0].recipe_execution_id.unwrap().to_string(),
        execution_id
    );

    let (status, replay) = app
        .send(
            Method::POST,
            "/api/v1/recipes/executions",
            Some(json!({
                "recipe_name": "Rice bowls",
                "serving_scale": "1",
                "idempotency_key": "cook-rice-once",
                "ingredients": [{
                    "line_id": "rice",
                    "display_name": "rice",
                    "product_id": rice_id,
                    "quantity": "10",
                    "unit": "g"
                }]
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["idempotent_replay"], true);
    assert_eq!(replay["execution_id"], execution_id);
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/api/v1/stock/{first_batch}"),
            None,
            Some(&alice),
        )
        .await
        .1["quantity"],
        "50"
    );

    assert_ledger_balances(&app, household_id).await;
}

#[tokio::test]
async fn recipe_execution_requires_confirmation_for_missing_required_ingredients() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let (_household_id, pantry_id) = household_and_pantry(&app, &alice).await;

    let flour_id = create_product(&app, &alice, "Flour", "mass", "g").await;
    let batch_id = create_stock(&app, &alice, flour_id, pantry_id, "100", "g", None).await;

    let request = json!({
        "recipe_name": "Bread",
        "ingredients": [{
            "line_id": "flour",
            "display_name": "flour",
            "product_id": flour_id,
            "quantity": "250",
            "unit": "g"
        }]
    });
    let (status, preflight) = app
        .send(
            Method::POST,
            "/api/v1/recipes/executions/preflight",
            Some(request.clone()),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preflight["can_execute"], false);
    assert_eq!(
        preflight["missing_ingredients"][0]["missing_quantity"],
        "150"
    );

    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/recipes/executions",
            Some(request),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");
    assert_eq!(
        app.send(
            Method::GET,
            &format!("/api/v1/stock/{batch_id}"),
            None,
            Some(&alice),
        )
        .await
        .1["quantity"],
        "100"
    );
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

async fn create_product(
    app: &TestApp,
    bearer: &str,
    name: &str,
    family: &str,
    preferred_unit: &str,
) -> Uuid {
    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": name,
                "brand": null,
                "family": family,
                "preferred_unit": preferred_unit,
                "barcode": null,
                "image_url": null
            })),
            Some(bearer),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

async fn create_stock(
    app: &TestApp,
    bearer: &str,
    product_id: Uuid,
    location_id: Uuid,
    quantity: &str,
    unit: &str,
    expires_on: Option<&str>,
) -> Uuid {
    let (status, body) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": location_id,
                "quantity": quantity,
                "unit": unit,
                "expires_on": expires_on
            })),
            Some(bearer),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

async fn assert_ledger_balances(app: &TestApp, household_id: Uuid) {
    let rows = sqlx::query(
        "SELECT b.id, b.quantity \
         FROM stock_batch b \
         WHERE b.household_id = ?",
    )
    .bind(household_id.to_string())
    .fetch_all(&app.db.pool)
    .await
    .unwrap();

    for row in rows {
        let batch_id: String = row.try_get("id").unwrap();
        let quantity: String = row.try_get("quantity").unwrap();
        let events =
            qm_db::stock_events::list_for_batch(&app.db, Uuid::parse_str(&batch_id).unwrap())
                .await
                .unwrap();
        let sum = events.into_iter().fold(Decimal::ZERO, |acc, event| {
            acc + Decimal::from_str(&event.quantity_delta).unwrap()
        });
        assert_eq!(
            sum.normalize().to_string(),
            Decimal::from_str(&quantity)
                .unwrap()
                .normalize()
                .to_string(),
            "ledger mismatch for {batch_id}"
        );
    }
}
