mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use serde_json::json;
use support::{me_current_household_id, TestApp};
use uuid::Uuid;

#[tokio::test]
async fn ingredient_mapping_allows_recipe_conversion_without_changing_product_family() {
    let app = TestApp::start(ApiConfig::default()).await;
    assert_eq!(app.register("alice", None).await.0, StatusCode::CREATED);
    let alice = app.login("alice").await;
    let household_id =
        Uuid::parse_str(me_current_household_id(&app.me(&alice).await).unwrap()).unwrap();
    let pantry = qm_db::locations::list_for_household(&app.db, household_id)
        .await
        .unwrap()
        .into_iter()
        .find(|loc| loc.kind == "pantry")
        .unwrap()
        .id;

    let (status, product) = app
        .send(
            Method::POST,
            "/api/v1/products",
            Some(json!({
                "name": "Bread Flour",
                "brand": null,
                "family": "mass",
                "preferred_unit": "g",
                "barcode": null,
                "image_url": null,
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let product_id = product["id"].as_str().unwrap();

    let (status, _) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry,
                "quantity": "240",
                "unit": "g",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, ingredient) = app
        .send(
            Method::POST,
            "/api/v1/ingredients",
            Some(json!({
                "display_name": "Flour",
                "category": "baking",
                "default_family": "volume",
                "aliases": ["plain flour"],
                "dietary_tags": [],
                "allergen_tags": ["wheat"],
                "notes": null,
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let ingredient_id = ingredient["id"].as_str().unwrap();

    let (status, mapping) = app
        .send(
            Method::POST,
            &format!("/api/v1/ingredients/{ingredient_id}/product-mappings"),
            Some(json!({
                "product_id": product_id,
                "rank": 0,
                "match_kind": "exact_product_link",
                "match_metadata": { "density_source": "King Arthur ingredient weight chart" },
                "conversion": {
                    "recipe_quantity": {
                        "amount": "1",
                        "unit": "cup",
                        "family": "volume",
                        "range": null,
                        "to_taste": false,
                        "preparation_note": "scooped"
                    },
                    "inventory_quantity": {
                        "amount": "120",
                        "unit": "g",
                        "family": "mass",
                        "range": null,
                        "to_taste": false,
                        "preparation_note": null
                    },
                    "provenance": "user_entered_density_yield",
                    "notes": "Recipe-layer density only; inventory remains mass."
                }
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(mapping["conversion"]["recipe_quantity"]["family"], "volume");
    assert_eq!(
        mapping["conversion"]["inventory_quantity"]["family"],
        "mass"
    );

    let (status, metadata) = app
        .send(
            Method::PUT,
            &format!("/api/v1/products/{product_id}/recipe-metadata"),
            Some(json!({
                "product_id": product_id,
                "edible_yield_percent": "95",
                "drained_quantity": null,
                "drained_unit": null,
                "density_recipe_quantity": "1",
                "density_recipe_unit": "cup",
                "density_inventory_quantity": "120",
                "density_inventory_unit": "g",
                "density_provenance": "user_entered_density_yield",
                "preparation_state": "sifted",
                "counts_as_aliases": ["flour"],
                "notes": "Product-specific recipe metadata only.",
                "updated_at": null,
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metadata["density_recipe_unit"], "cup");
    assert_eq!(metadata["density_inventory_unit"], "g");

    let (status, product_after_mapping) = app
        .send(
            Method::GET,
            &format!("/api/v1/products/{product_id}"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(product_after_mapping["family"], "mass");

    let (status, rejected_stock) = app
        .send(
            Method::POST,
            "/api/v1/stock",
            Some(json!({
                "product_id": product_id,
                "location_id": pantry,
                "quantity": "1",
                "unit": "cup",
            })),
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(rejected_stock["code"], "unit_family_mismatch");

    let (status, availability) = app
        .send(
            Method::GET,
            &format!("/api/v1/ingredients/{ingredient_id}/availability"),
            None,
            Some(&alice),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let items = availability["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["product_id"], product_id);
    assert_eq!(items[0]["quantity"], "240");
    assert_eq!(items[0]["unit"], "g");
}
