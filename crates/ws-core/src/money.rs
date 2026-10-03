use std::fmt;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Error;

pub(crate) const MAX_SCALE: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Cad,
    Usd,
}

impl Currency {
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Cad => "CAD",
            Self::Usd => "USD",
        }
    }
}

impl FromStr for Currency {
    type Err = Error;

    fn from_str(raw: &str) -> Result<Self, Error> {
        match raw.to_ascii_uppercase().as_str() {
            "CAD" => Ok(Self::Cad),
            "USD" => Ok(Self::Usd),
            _ => Err(Error::UnknownCurrency(raw.to_owned())),
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Raw", into = "Raw")]
pub struct Money {
    amount: Decimal,
    currency: Currency,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Result<Self, Error> {
        let amount = amount.normalize();
        if amount.scale() > MAX_SCALE {
            return Err(Error::TooPrecise(amount));
        }
        Ok(Self { amount, currency })
    }

    #[must_use]
    pub fn zero(currency: Currency) -> Self {
        Self { amount: Decimal::ZERO, currency }
    }

    #[must_use]
    pub fn amount(&self) -> Decimal {
        self.amount
    }

    #[must_use]
    pub fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub fn is_positive(&self) -> bool {
        self.amount.is_sign_positive() && !self.amount.is_zero()
    }

    pub fn checked_add(self, other: Self) -> Result<Self, Error> {
        self.same_currency(other)?;
        let amount = self.amount.checked_add(other.amount).ok_or(Error::Overflow)?;
        Ok(Self { amount, currency: self.currency })
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, Error> {
        self.same_currency(other)?;
        let amount = self.amount.checked_sub(other.amount).ok_or(Error::Overflow)?;
        Ok(Self { amount, currency: self.currency })
    }

    pub fn times(self, factor: Decimal) -> Result<Self, Error> {
        let amount = self.amount.checked_mul(factor).ok_or(Error::Overflow)?.round_dp(2);
        Ok(Self { amount, currency: self.currency })
    }

    fn same_currency(self, other: Self) -> Result<(), Error> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(Error::CurrencyMismatch { left: self.currency.code(), right: other.currency.code() })
        }
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} {}", self.amount, self.currency)
    }
}

#[derive(Serialize, Deserialize)]
struct Raw {
    #[serde(with = "rust_decimal::serde::str")]
    amount: Decimal,
    currency: Currency,
}

impl TryFrom<Raw> for Money {
    type Error = Error;

    fn try_from(raw: Raw) -> Result<Self, Error> {
        Self::new(raw.amount, raw.currency)
    }
}

impl From<Money> for Raw {
    fn from(money: Money) -> Self {
        Self { amount: money.amount, currency: money.currency }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{cad, usd};
    use rust_decimal::dec;

    #[test]
    fn rejects_excess_precision() {
        assert_eq!(Money::new(dec!(1.23456), Currency::Cad), Err(Error::TooPrecise(dec!(1.23456))));
        assert!(Money::new(dec!(0.0045), Currency::Cad).is_ok());
    }

    #[test]
    fn trailing_zeros_do_not_count_as_precision() {
        assert_eq!(cad(dec!(1.230000)).amount(), dec!(1.23));
    }

    #[test]
    fn refuses_mixed_currencies() {
        assert!(matches!(cad(dec!(1)).checked_add(usd(dec!(1))), Err(Error::CurrencyMismatch { .. })));
    }

    #[test]
    fn times_rounds_to_cents() {
        assert_eq!(cad(dec!(0.3333)).times(dec!(3)), Ok(cad(dec!(1.00))));
    }

    #[test]
    fn serde_round_trip_validates() {
        let json = serde_json::to_string(&cad(dec!(12.5))).unwrap_or_default();
        assert_eq!(json, r#"{"amount":"12.5","currency":"CAD"}"#);
        let bad = serde_json::from_str::<Money>(r#"{"amount":"1.23456","currency":"CAD"}"#);
        assert!(bad.is_err());
    }

    #[test]
    fn parses_currency_case_insensitively() {
        assert_eq!("usd".parse::<Currency>(), Ok(Currency::Usd));
        assert!("EUR".parse::<Currency>().is_err());
    }
}
