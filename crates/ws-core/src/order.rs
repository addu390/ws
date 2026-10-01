//! Order intents: side, quantity, execution kind, and time in force.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{AccountId, Error, Money, Quote, SecurityId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "Decimal", into = "Decimal")]
pub struct Quantity(Decimal);

impl Quantity {
    pub fn new(value: Decimal) -> Result<Self, Error> {
        if value > Decimal::ZERO {
            Ok(Self(value.normalize()))
        } else {
            Err(Error::NonPositiveQuantity(value))
        }
    }

    #[must_use]
    pub fn value(&self) -> Decimal {
        self.0
    }
}

impl TryFrom<Decimal> for Quantity {
    type Error = Error;

    fn try_from(value: Decimal) -> Result<Self, Error> {
        Self::new(value)
    }
}

impl From<Quantity> for Decimal {
    fn from(quantity: Quantity) -> Self {
        quantity.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Kind {
    Market,
    Limit { limit: Money },
    StopLimit { stop: Money, limit: Money },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tif {
    #[default]
    Day,
    Gtc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    account: AccountId,
    security: SecurityId,
    side: Side,
    quantity: Quantity,
    kind: Kind,
    tif: Tif,
}

impl Order {
    #[must_use]
    pub fn market_buy(account: AccountId, security: SecurityId, quantity: Quantity) -> Self {
        Self::build(account, security, Side::Buy, quantity, Kind::Market)
    }

    #[must_use]
    pub fn market_sell(account: AccountId, security: SecurityId, quantity: Quantity) -> Self {
        Self::build(account, security, Side::Sell, quantity, Kind::Market)
    }

    pub fn limit_buy(account: AccountId, security: SecurityId, quantity: Quantity, limit: Money) -> Result<Self, Error> {
        positive(limit)?;
        Ok(Self::build(account, security, Side::Buy, quantity, Kind::Limit { limit }))
    }

    pub fn limit_sell(account: AccountId, security: SecurityId, quantity: Quantity, limit: Money) -> Result<Self, Error> {
        positive(limit)?;
        Ok(Self::build(account, security, Side::Sell, quantity, Kind::Limit { limit }))
    }

    pub fn stop_limit_sell(
        account: AccountId,
        security: SecurityId,
        quantity: Quantity,
        stop: Money,
        limit: Money,
    ) -> Result<Self, Error> {
        positive(stop)?;
        positive(limit)?;
        Ok(Self::build(account, security, Side::Sell, quantity, Kind::StopLimit { stop, limit }))
    }

    #[must_use]
    pub fn good_till_cancelled(mut self) -> Self {
        self.tif = Tif::Gtc;
        self
    }

    fn build(account: AccountId, security: SecurityId, side: Side, quantity: Quantity, kind: Kind) -> Self {
        Self { account, security, side, quantity, kind, tif: Tif::Day }
    }

    #[must_use]
    pub fn account(&self) -> &AccountId {
        &self.account
    }

    #[must_use]
    pub fn security(&self) -> &SecurityId {
        &self.security
    }

    #[must_use]
    pub fn side(&self) -> Side {
        self.side
    }

    #[must_use]
    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    #[must_use]
    pub fn tif(&self) -> Tif {
        self.tif
    }

    /// Estimated value: the limit price when set, otherwise the quote.
    pub fn notional(&self, quote: &Quote) -> Result<Money, Error> {
        let price = match self.kind {
            Kind::Market => quote.price(),
            Kind::Limit { limit } | Kind::StopLimit { limit, .. } => limit,
        };
        price.times(self.quantity.value())
    }
}

fn positive(price: Money) -> Result<(), Error> {
    if price.is_positive() { Ok(()) } else { Err(Error::NonPositivePrice(price.amount())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Currency, MarketStatus};
    use chrono::Utc;
    use rust_decimal::dec;

    fn cad(amount: Decimal) -> Money {
        Money::new(amount, Currency::Cad).unwrap_or_else(|e| panic!("{e}"))
    }

    fn ids() -> (AccountId, SecurityId) {
        let account = AccountId::parse("tfsa-abc").unwrap_or_else(|e| panic!("{e}"));
        let security = SecurityId::parse("sec-s-xeqt").unwrap_or_else(|e| panic!("{e}"));
        (account, security)
    }

    fn qty(value: Decimal) -> Quantity {
        Quantity::new(value).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn quantity_must_be_positive() {
        assert!(Quantity::new(dec!(0)).is_err());
        assert!(Quantity::new(dec!(-1)).is_err());
        assert!(Quantity::new(dec!(0.5)).is_ok());
    }

    #[test]
    fn limit_price_must_be_positive() {
        let (account, security) = ids();
        assert!(Order::limit_buy(account, security, qty(dec!(1)), cad(dec!(0))).is_err());
    }

    #[test]
    fn notional_uses_limit_over_quote() {
        let (account, security) = ids();
        let quote = Quote::new(security.clone(), cad(dec!(40)), Utc::now(), MarketStatus::Open);
        let limit = Order::limit_buy(account.clone(), security.clone(), qty(dec!(3)), cad(dec!(35.10)))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(limit.notional(&quote), Ok(cad(dec!(105.30))));
        let market = Order::market_buy(account, security, qty(dec!(3)));
        assert_eq!(market.notional(&quote), Ok(cad(dec!(120))));
    }

    #[test]
    fn defaults_to_day_orders() {
        let (account, security) = ids();
        let order = Order::market_sell(account, security, qty(dec!(1)));
        assert_eq!(order.tif(), Tif::Day);
        assert_eq!(order.good_till_cancelled().tif(), Tif::Gtc);
    }
}
