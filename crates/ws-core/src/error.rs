//! Errors raised by domain constructors.

use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("unknown currency {0:?}")]
    UnknownCurrency(String),
    #[error("amount {0} has more than {max} decimal places", max = crate::money::MAX_SCALE)]
    TooPrecise(Decimal),
    #[error("cannot combine {left} with {right}")]
    CurrencyMismatch { left: &'static str, right: &'static str },
    #[error("arithmetic overflow")]
    Overflow,
    #[error("invalid {kind} id {value:?}")]
    InvalidId { kind: &'static str, value: String },
    #[error("quantity must be positive, got {0}")]
    NonPositiveQuantity(Decimal),
    #[error("price must be positive, got {0}")]
    NonPositivePrice(Decimal),
}
