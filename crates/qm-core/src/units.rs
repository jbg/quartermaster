use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::errors::QmError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UnitFamily {
    Mass,
    Volume,
    Count,
}

impl UnitFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            UnitFamily::Mass => "mass",
            UnitFamily::Volume => "volume",
            UnitFamily::Count => "count",
        }
    }

    pub fn from_str_ci(s: &str) -> Option<Self> {
        match s {
            "mass" => Some(Self::Mass),
            "volume" => Some(Self::Volume),
            "count" => Some(Self::Count),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Unit {
    pub code: &'static str,
    pub family: UnitFamily,
    /// Factor applied to convert a value in this unit to the family's base unit
    /// (g for Mass, ml for Volume, piece for Count).
    pub to_base_milli: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementSystem {
    Metric,
    UsCustomary,
    Australian,
    Imperial,
}

impl MeasurementSystem {
    pub const DEFAULT: Self = Self::Metric;

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::UsCustomary => "us_customary",
            Self::Australian => "australian",
            Self::Imperial => "imperial",
        }
    }

    pub fn from_str_ci(value: &str) -> Option<Self> {
        match value {
            "metric" => Some(Self::Metric),
            "us_customary" | "us-customary" | "us" => Some(Self::UsCustomary),
            "australian" | "au" => Some(Self::Australian),
            "imperial" | "uk" => Some(Self::Imperial),
            _ => None,
        }
    }

    const fn teaspoon_to_base_milli(self) -> u64 {
        match self {
            Self::Metric | Self::Australian => 5_000,
            Self::UsCustomary => 4_929,
            Self::Imperial => 5_919,
        }
    }

    const fn tablespoon_to_base_milli(self) -> u64 {
        match self {
            Self::Metric => 15_000,
            Self::UsCustomary => 14_787,
            Self::Australian => 20_000,
            Self::Imperial => 17_758,
        }
    }
}

impl Unit {
    const fn new(code: &'static str, family: UnitFamily, to_base_milli: u64) -> Self {
        Self {
            code,
            family,
            to_base_milli,
        }
    }

    fn to_base_factor(self) -> Decimal {
        Decimal::new(self.to_base_milli as i64, 3)
    }
}

const BASE_UNITS: &[Unit] = &[
    // Mass — base is gram (g)
    Unit::new("mg", UnitFamily::Mass, 1),         // 0.001 g
    Unit::new("g", UnitFamily::Mass, 1_000),      // 1 g
    Unit::new("kg", UnitFamily::Mass, 1_000_000), // 1000 g
    Unit::new("oz", UnitFamily::Mass, 28_349),    // 28.349 g
    Unit::new("lb", UnitFamily::Mass, 453_592),   // 453.592 g
    // Volume — base is millilitre (ml)
    Unit::new("ml", UnitFamily::Volume, 1_000),     // 1 ml
    Unit::new("l", UnitFamily::Volume, 1_000_000),  // 1000 ml
    Unit::new("tsp", UnitFamily::Volume, 5_000),    // 5 ml
    Unit::new("tbsp", UnitFamily::Volume, 15_000),  // 15 ml
    Unit::new("cup", UnitFamily::Volume, 236_588),  // 236.588 ml (US customary)
    Unit::new("fl_oz", UnitFamily::Volume, 29_574), // 29.574 ml (US customary)
    // Count — base is piece
    Unit::new("piece", UnitFamily::Count, 1_000),
];

pub fn lookup(code: &str) -> Result<Unit, QmError> {
    lookup_with_measurement_system(code, MeasurementSystem::DEFAULT)
}

pub fn lookup_with_measurement_system(
    code: &str,
    measurement_system: MeasurementSystem,
) -> Result<Unit, QmError> {
    if code.eq_ignore_ascii_case("tsp") {
        return Ok(Unit::new(
            "tsp",
            UnitFamily::Volume,
            measurement_system.teaspoon_to_base_milli(),
        ));
    }
    if code.eq_ignore_ascii_case("tbsp") {
        return Ok(Unit::new(
            "tbsp",
            UnitFamily::Volume,
            measurement_system.tablespoon_to_base_milli(),
        ));
    }

    BASE_UNITS
        .iter()
        .copied()
        .find(|u| u.code.eq_ignore_ascii_case(code))
        .ok_or_else(|| QmError::UnknownUnit(code.to_owned()))
}

pub fn all_units() -> Vec<Unit> {
    all_units_with_measurement_system(MeasurementSystem::DEFAULT)
}

pub fn all_units_with_measurement_system(measurement_system: MeasurementSystem) -> Vec<Unit> {
    BASE_UNITS
        .iter()
        .map(|unit| {
            lookup_with_measurement_system(unit.code, measurement_system)
                .expect("built-in unit should be known")
        })
        .collect()
}

/// Convert `qty` expressed in unit `from` into unit `to`.
/// Returns an error if the units belong to different families or are unknown.
pub fn convert(qty: Decimal, from: &str, to: &str) -> Result<Decimal, QmError> {
    convert_with_measurement_system(qty, from, to, MeasurementSystem::DEFAULT)
}

pub fn convert_with_measurement_system(
    qty: Decimal,
    from: &str,
    to: &str,
    measurement_system: MeasurementSystem,
) -> Result<Decimal, QmError> {
    let from_u = lookup_with_measurement_system(from, measurement_system)?;
    let to_u = lookup_with_measurement_system(to, measurement_system)?;
    if from_u.family != to_u.family {
        return Err(QmError::IncompatibleUnits {
            from: from.to_owned(),
            from_family: from_u.family.as_str(),
            to: to.to_owned(),
            to_family: to_u.family.as_str(),
        });
    }
    let base = qty * from_u.to_base_factor();
    Ok(base / to_u.to_base_factor())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn kg_to_g() {
        assert_eq!(convert(d("1"), "kg", "g").unwrap(), d("1000"));
    }

    #[test]
    fn g_to_kg() {
        assert_eq!(convert(d("500"), "g", "kg").unwrap(), d("0.5"));
    }

    #[test]
    fn ml_to_l() {
        assert_eq!(convert(d("250"), "ml", "l").unwrap(), d("0.25"));
    }

    #[test]
    fn metric_spoons_to_ml() {
        assert_eq!(convert(d("2"), "tbsp", "ml").unwrap(), d("30"));
        assert_eq!(convert(d("3"), "tsp", "ml").unwrap(), d("15"));
    }

    #[test]
    fn household_measurement_systems_to_ml() {
        assert_eq!(
            convert_with_measurement_system(d("2"), "tbsp", "ml", MeasurementSystem::UsCustomary)
                .unwrap(),
            d("29.574")
        );
        assert_eq!(
            convert_with_measurement_system(d("2"), "tbsp", "ml", MeasurementSystem::Australian)
                .unwrap(),
            d("40")
        );
        assert_eq!(
            convert_with_measurement_system(d("2"), "tbsp", "ml", MeasurementSystem::Imperial)
                .unwrap(),
            d("35.516")
        );
    }

    #[test]
    fn mass_to_volume_errors() {
        let err = convert(d("1"), "g", "ml").unwrap_err();
        assert!(matches!(err, QmError::IncompatibleUnits { .. }));
    }

    #[test]
    fn unknown_unit_errors() {
        assert!(matches!(
            convert(d("1"), "stone", "g"),
            Err(QmError::UnknownUnit(_))
        ));
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(convert(d("1"), "KG", "G").unwrap(), d("1000"));
    }
}
