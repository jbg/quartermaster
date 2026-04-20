use thiserror::Error;

#[derive(Debug, Error)]
pub enum QmError {
    #[error("unknown unit: {0}")]
    UnknownUnit(String),

    #[error("cannot convert between unit families: {from} is {from_family}, {to} is {to_family}")]
    IncompatibleUnits {
        from: String,
        from_family: &'static str,
        to: String,
        to_family: &'static str,
    },

    #[error("requested quantity ({requested}) exceeds available stock ({available})")]
    InsufficientStock {
        requested: rust_decimal::Decimal,
        available: rust_decimal::Decimal,
    },

    #[error("invalid quantity: {0}")]
    InvalidQuantity(String),
}
