//! Minimal OpenFoodFacts v2 API client.
//!
//! We only touch the product lookup endpoint and we only pull the fields the
//! rest of Quartermaster cares about. Heavy lifting (caching, TTL, fallback to
//! manual entry) lives in the products route handler, not here.

use std::time::Duration;

use qm_core::units::UnitFamily;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tracing::{debug, warn};

const API_BASE: &str = "https://world.openfoodfacts.org/api/v2/product";
const FIELDS: &str = "code,product_name,product_name_en,brands,image_url,product_quantity_unit";

#[derive(Debug, Clone)]
pub struct OpenFoodFactsClient {
    http: Client,
}

#[derive(Debug, Clone)]
pub struct OffProduct {
    pub barcode: String,
    pub name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    /// Raw unit string as OFF reports it, for family inference by the caller.
    pub quantity_unit: Option<String>,
}

#[derive(Debug)]
pub enum OffResult {
    Found(OffProduct),
    NotFound,
    Upstream(String),
}

impl OpenFoodFactsClient {
    pub fn new(http: Client) -> Self {
        Self { http }
    }

    pub async fn fetch(&self, barcode: &str) -> OffResult {
        let url = format!("{API_BASE}/{barcode}.json?fields={FIELDS}");
        debug!(%url, "OFF lookup");

        let response = match self.http.get(&url).timeout(Duration::from_secs(5)).send().await {
            Ok(r) => r,
            Err(err) => {
                warn!(%barcode, ?err, "OFF request failed");
                return OffResult::Upstream(err.to_string());
            }
        };

        if response.status() == StatusCode::NOT_FOUND {
            return OffResult::NotFound;
        }
        if !response.status().is_success() {
            let status = response.status();
            warn!(%barcode, %status, "OFF non-success response");
            return OffResult::Upstream(format!("OFF returned {status}"));
        }

        let payload: OffResponse = match response.json().await {
            Ok(p) => p,
            Err(err) => {
                warn!(%barcode, ?err, "OFF payload decode failed");
                return OffResult::Upstream(err.to_string());
            }
        };

        if payload.status != 1 {
            return OffResult::NotFound;
        }
        let Some(product) = payload.product else {
            return OffResult::NotFound;
        };

        let name = product
            .product_name_en
            .filter(|s| !s.trim().is_empty())
            .or(product.product_name)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("Barcode {barcode}"));

        OffResult::Found(OffProduct {
            barcode: barcode.to_owned(),
            name,
            brand: product.brands.filter(|s| !s.trim().is_empty()),
            image_url: product.image_url.filter(|s| !s.trim().is_empty()),
            quantity_unit: product.product_quantity_unit.filter(|s| !s.trim().is_empty()),
        })
    }
}

/// Map an OFF unit hint to a Quartermaster unit family. Unknown or missing
/// hints fall back to `Count` — the least-wrong default given that a mass or
/// volume guess would be silently wrong.
pub fn infer_family(hint: Option<&str>) -> UnitFamily {
    let Some(raw) = hint else { return UnitFamily::Count };
    let lowered = raw.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return UnitFamily::Count;
    }
    if let Ok(u) = qm_core::units::lookup(&lowered) {
        return u.family;
    }
    // A few OFF conventions that don't round-trip cleanly through our unit
    // table (plural, long-form). Normalise the common ones.
    match lowered.as_str() {
        "grams" | "gram" => UnitFamily::Mass,
        "kilograms" | "kilogram" => UnitFamily::Mass,
        "ounces" | "ounce" => UnitFamily::Mass,
        "pounds" | "pound" => UnitFamily::Mass,
        "milliliters" | "milliliter" | "millilitre" | "millilitres" => UnitFamily::Volume,
        "liters" | "liter" | "litre" | "litres" => UnitFamily::Volume,
        "pieces" | "piece" | "units" | "unit" | "ct" | "count" => UnitFamily::Count,
        _ => UnitFamily::Count,
    }
}

#[derive(Debug, Deserialize)]
struct OffResponse {
    #[serde(default)]
    status: i64,
    #[serde(default)]
    product: Option<OffInnerProduct>,
}

#[derive(Debug, Deserialize)]
struct OffInnerProduct {
    product_name: Option<String>,
    product_name_en: Option<String>,
    brands: Option<String>,
    image_url: Option<String>,
    product_quantity_unit: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_family_from_known_units() {
        assert_eq!(infer_family(Some("g")), UnitFamily::Mass);
        assert_eq!(infer_family(Some("kg")), UnitFamily::Mass);
        assert_eq!(infer_family(Some("ml")), UnitFamily::Volume);
        assert_eq!(infer_family(Some("l")), UnitFamily::Volume);
        assert_eq!(infer_family(Some("piece")), UnitFamily::Count);
    }

    #[test]
    fn infer_family_from_long_forms() {
        assert_eq!(infer_family(Some("grams")), UnitFamily::Mass);
        assert_eq!(infer_family(Some("Milliliters")), UnitFamily::Volume);
        assert_eq!(infer_family(Some("units")), UnitFamily::Count);
    }

    #[test]
    fn infer_family_falls_back_to_count() {
        assert_eq!(infer_family(None), UnitFamily::Count);
        assert_eq!(infer_family(Some("")), UnitFamily::Count);
        assert_eq!(infer_family(Some("  ")), UnitFamily::Count);
        assert_eq!(infer_family(Some("whatever")), UnitFamily::Count);
    }
}
