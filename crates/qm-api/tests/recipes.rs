mod support;

use axum::http::{Method, StatusCode};
use qm_api::{ApiConfig, RegistrationMode};
use serde_json::json;
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
