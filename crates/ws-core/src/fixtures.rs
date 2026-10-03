#![allow(clippy::missing_panics_doc)]

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;

use crate::{AccountId, Currency, MarketStatus, Money, Order, Quantity, Quote, SecurityId};

#[must_use]
pub fn cad(amount: Decimal) -> Money {
    Money::new(amount, Currency::Cad).unwrap_or_else(|e| panic!("{e}"))
}

#[must_use]
pub fn usd(amount: Decimal) -> Money {
    Money::new(amount, Currency::Usd).unwrap_or_else(|e| panic!("{e}"))
}

#[must_use]
pub fn quantity(value: Decimal) -> Quantity {
    Quantity::new(value).unwrap_or_else(|e| panic!("{e}"))
}

#[must_use]
pub fn account() -> AccountId {
    account_id("tfsa-abc")
}

#[must_use]
pub fn account_id(raw: &str) -> AccountId {
    AccountId::parse(raw).unwrap_or_else(|e| panic!("{e}"))
}

#[must_use]
pub fn security() -> SecurityId {
    SecurityId::parse("sec-s-xeqt").unwrap_or_else(|e| panic!("{e}"))
}

#[must_use]
pub fn noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 2, 15, 0, 0).single().unwrap_or_else(|| panic!("valid instant"))
}

#[must_use]
pub fn quote(price: Decimal, market: MarketStatus) -> Quote {
    Quote::new(security(), cad(price), noon(), market)
}

#[must_use]
pub fn limit_buy(shares: Decimal, limit: Decimal) -> Order {
    Order::limit_buy(account(), security(), quantity(shares), cad(limit)).unwrap_or_else(|e| panic!("{e}"))
}

#[must_use]
pub fn limit_sell(shares: Decimal, limit: Decimal) -> Order {
    Order::limit_sell(account(), security(), quantity(shares), cad(limit)).unwrap_or_else(|e| panic!("{e}"))
}
