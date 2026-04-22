use jiff::{civil::Date, Timestamp};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::errors::QmError;
use crate::units;

#[derive(Debug, Clone)]
pub struct BatchRef {
    pub id: Uuid,
    pub quantity: Decimal,
    pub unit: String,
    pub expires_on: Option<Date>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchConsumption {
    pub batch_id: Uuid,
    /// How much is taken from this batch, expressed in the batch's own unit.
    pub quantity: Decimal,
    /// Whether this consumption depletes the batch.
    pub depletes: bool,
}

/// Given a set of batches for a single product, plan a FIFO consumption of
/// `requested` in `requested_unit`. Batches are consumed in order of earliest
/// expiry first, with NULL expiries pushed to the end (break ties by
/// `created_at`). Mass↔mass and volume↔volume conversions are supported; any
/// cross-family conversion returns an error.
pub fn plan_consumption(
    mut batches: Vec<BatchRef>,
    requested: Decimal,
    requested_unit: &str,
) -> Result<Vec<BatchConsumption>, QmError> {
    if requested <= Decimal::ZERO {
        return Err(QmError::InvalidQuantity(requested.to_string()));
    }

    batches.sort_by(|a, b| match (a.expires_on, b.expires_on) {
        (Some(x), Some(y)) => x.cmp(&y).then(a.created_at.cmp(&b.created_at)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.created_at.cmp(&b.created_at),
    });

    let mut plan = Vec::new();
    let mut remaining_requested = requested;
    let mut total_available_in_requested_unit = Decimal::ZERO;

    for batch in batches {
        if remaining_requested <= Decimal::ZERO {
            break;
        }

        let available_in_requested_unit =
            units::convert(batch.quantity, &batch.unit, requested_unit)?;
        total_available_in_requested_unit += available_in_requested_unit;

        if available_in_requested_unit >= remaining_requested {
            // This batch satisfies the rest of the request.
            let taken_in_batch_unit =
                units::convert(remaining_requested, requested_unit, &batch.unit)?;
            let depletes = taken_in_batch_unit >= batch.quantity;
            plan.push(BatchConsumption {
                batch_id: batch.id,
                quantity: if depletes {
                    batch.quantity
                } else {
                    taken_in_batch_unit
                },
                depletes,
            });
            remaining_requested = Decimal::ZERO;
        } else {
            // Take the whole batch and continue.
            plan.push(BatchConsumption {
                batch_id: batch.id,
                quantity: batch.quantity,
                depletes: true,
            });
            remaining_requested -= available_in_requested_unit;
        }
    }

    if remaining_requested > Decimal::ZERO {
        return Err(QmError::InsufficientStock {
            requested,
            available: total_available_in_requested_unit,
        });
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::{civil::Date, Timestamp};
    use std::str::FromStr;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn batch(id: u8, qty: &str, unit: &str, expires: Option<&str>, created_ts: i64) -> BatchRef {
        BatchRef {
            id: Uuid::from_u128(id as u128),
            quantity: dec(qty),
            unit: unit.to_owned(),
            expires_on: expires.map(|s| Date::from_str(s).unwrap()),
            created_at: Timestamp::from_second(created_ts).unwrap(),
        }
    }

    #[test]
    fn single_batch_exact() {
        let plan = plan_consumption(
            vec![batch(1, "500", "g", Some("2026-05-01"), 1)],
            dec("500"),
            "g",
        )
        .unwrap();
        assert_eq!(plan.len(), 1);
        assert!(plan[0].depletes);
        assert_eq!(plan[0].quantity, dec("500"));
    }

    #[test]
    fn fifo_earliest_expiry_first() {
        let plan = plan_consumption(
            vec![
                batch(2, "500", "g", Some("2026-06-01"), 1),
                batch(1, "500", "g", Some("2026-05-01"), 2),
            ],
            dec("300"),
            "g",
        )
        .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].batch_id, Uuid::from_u128(1)); // earliest expiry
        assert_eq!(plan[0].quantity, dec("300"));
        assert!(!plan[0].depletes);
    }

    #[test]
    fn spans_two_batches() {
        let plan = plan_consumption(
            vec![
                batch(1, "500", "g", Some("2026-05-01"), 1),
                batch(2, "500", "g", Some("2026-06-01"), 2),
            ],
            dec("750"),
            "g",
        )
        .unwrap();
        assert_eq!(plan.len(), 2);
        assert!(plan[0].depletes);
        assert_eq!(plan[0].quantity, dec("500"));
        assert_eq!(plan[1].quantity, dec("250"));
        assert!(!plan[1].depletes);
    }

    #[test]
    fn cross_unit_within_family() {
        // 1 kg batch, consume 250 g — should leave 750 g (partial).
        let plan = plan_consumption(
            vec![batch(1, "1", "kg", Some("2026-05-01"), 1)],
            dec("250"),
            "g",
        )
        .unwrap();
        assert_eq!(plan.len(), 1);
        assert!(!plan[0].depletes);
        assert_eq!(plan[0].quantity, dec("0.250")); // expressed in batch's unit (kg)
    }

    #[test]
    fn cross_family_errors() {
        let err =
            plan_consumption(vec![batch(1, "500", "g", None, 1)], dec("250"), "ml").unwrap_err();
        assert!(matches!(err, QmError::IncompatibleUnits { .. }));
    }

    #[test]
    fn insufficient_stock_errors() {
        let err =
            plan_consumption(vec![batch(1, "100", "g", None, 1)], dec("250"), "g").unwrap_err();
        assert!(matches!(err, QmError::InsufficientStock { .. }));
    }

    #[test]
    fn null_expiry_sorted_last() {
        let plan = plan_consumption(
            vec![
                batch(1, "100", "g", None, 1),
                batch(2, "100", "g", Some("2026-05-01"), 2),
            ],
            dec("100"),
            "g",
        )
        .unwrap();
        assert_eq!(plan[0].batch_id, Uuid::from_u128(2));
    }
}
