use axum::{routing::get, Json, Router};
use qm_core::units::UnitFamily;
use serde::Serialize;
use utoipa::ToSchema;

use crate::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct UnitDto {
    pub code: String,
    pub family: UnitFamily,
    /// Conversion factor to the family's base unit (`g` for mass, `ml` for
    /// volume, `piece` for count), expressed as thousandths so we can keep it
    /// an integer. Divide by 1000 to get the multiplier.
    pub to_base_milli: i64,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/units", get(list_units))
}

#[utoipa::path(
    get,
    path = "/units",
    operation_id = "units_list",
    tag = "units",
    responses((status = 200, body = [UnitDto])),
)]
pub async fn list_units() -> Json<Vec<UnitDto>> {
    Json(
        qm_core::units::all_units()
            .iter()
            .map(|u| UnitDto {
                code: u.code.to_owned(),
                family: u.family,
                to_base_milli: u.to_base_milli as i64,
            })
            .collect(),
    )
}
