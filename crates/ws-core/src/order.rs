use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::{AccountId, Error, Money, Quote, SecurityId};

const FRACTION_SCALE: u32 = 4;
const VALUE_SCALE: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "Decimal", into = "Decimal")]
pub struct Quantity(Decimal);

impl Quantity {
    pub fn new(value: Decimal) -> Result<Self, Error> {
        if value > Decimal::ZERO { Ok(Self(value.normalize())) } else { Err(Error::NonPositiveQuantity(value)) }
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
#[serde(rename_all = "snake_case")]
pub enum Size {
    Shares(Quantity),
    Value(Money),
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
    size: Size,
    kind: Kind,
    tif: Tif,
}

impl Order {
    #[must_use]
    pub fn market_buy(account: AccountId, security: SecurityId, quantity: Quantity) -> Self {
        Self::build(account, security, Side::Buy, Size::Shares(quantity), Kind::Market)
    }

    #[must_use]
    pub fn market_sell(account: AccountId, security: SecurityId, quantity: Quantity) -> Self {
        Self::build(account, security, Side::Sell, Size::Shares(quantity), Kind::Market)
    }

    pub fn value_buy(account: AccountId, security: SecurityId, value: Money) -> Result<Self, Error> {
        if !value.is_positive() {
            return Err(Error::NonPositiveValue(value.amount()));
        }
        if value.amount().scale() > VALUE_SCALE {
            return Err(Error::TooPrecise(value.amount()));
        }
        Ok(Self::build(account, security, Side::Buy, Size::Value(value), Kind::Market))
    }

    pub fn limit_buy(
        account: AccountId,
        security: SecurityId,
        quantity: Quantity,
        limit: Money,
    ) -> Result<Self, Error> {
        positive(limit)?;
        Ok(Self::build(account, security, Side::Buy, Size::Shares(quantity), Kind::Limit { limit }))
    }

    pub fn limit_sell(
        account: AccountId,
        security: SecurityId,
        quantity: Quantity,
        limit: Money,
    ) -> Result<Self, Error> {
        positive(limit)?;
        Ok(Self::build(account, security, Side::Sell, Size::Shares(quantity), Kind::Limit { limit }))
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
        Ok(Self::build(account, security, Side::Sell, Size::Shares(quantity), Kind::StopLimit { stop, limit }))
    }

    #[must_use]
    pub fn good_till_cancelled(mut self) -> Self {
        self.tif = Tif::Gtc;
        self
    }

    fn build(account: AccountId, security: SecurityId, side: Side, amount: Size, kind: Kind) -> Self {
        Self { account, security, side, size: amount, kind, tif: Tif::Day }
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
    pub fn size(&self) -> Size {
        self.size
    }

    #[must_use]
    pub fn shares(&self) -> Option<Quantity> {
        match self.size {
            Size::Shares(quantity) => Some(quantity),
            Size::Value(_) => None,
        }
    }

    pub fn shares_at(&self, price: Money) -> Result<Quantity, Error> {
        match self.size {
            Size::Shares(quantity) => Ok(quantity),
            Size::Value(value) => {
                if value.currency() != price.currency() {
                    return Err(Error::CurrencyMismatch {
                        left: value.currency().code(),
                        right: price.currency().code(),
                    });
                }
                let shares = value.amount().checked_div(price.amount()).ok_or(Error::Overflow)?;
                Quantity::new(shares.round_dp_with_strategy(FRACTION_SCALE, RoundingStrategy::ToZero))
            }
        }
    }

    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    #[must_use]
    pub fn tif(&self) -> Tif {
        self.tif
    }

    pub fn notional(&self, quote: &Quote) -> Result<Money, Error> {
        let quantity = match self.size {
            Size::Value(value) => return Ok(value),
            Size::Shares(quantity) => quantity,
        };
        let price = match self.kind {
            Kind::Market => quote.price(),
            Kind::Limit { limit } | Kind::StopLimit { limit, .. } => limit,
        };
        price.times(quantity.value())
    }
}

fn positive(price: Money) -> Result<(), Error> {
    if price.is_positive() { Ok(()) } else { Err(Error::NonPositivePrice(price.amount())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MarketStatus;
    use crate::fixtures::{account, cad, quantity as qty, quote, security, usd};
    use rust_decimal::dec;

    fn ids() -> (AccountId, SecurityId) {
        (account(), security())
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
        let quote = quote(dec!(40), MarketStatus::Open);
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

    #[test]
    fn value_buys_take_positive_whole_cents() {
        let (account, security) = ids();
        assert_eq!(
            Order::value_buy(account.clone(), security.clone(), cad(dec!(0))),
            Err(Error::NonPositiveValue(dec!(0)))
        );
        assert_eq!(
            Order::value_buy(account.clone(), security.clone(), cad(dec!(1.005))),
            Err(Error::TooPrecise(dec!(1.005)))
        );
        let order = Order::value_buy(account, security, cad(dec!(25))).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((order.side(), order.kind(), order.shares()), (Side::Buy, Kind::Market, None));
        assert_eq!(order.notional(&quote(dec!(121.4), MarketStatus::Open)), Ok(cad(dec!(25))));
    }

    #[test]
    fn value_buys_round_fractional_shares_down() {
        let (account, security) = ids();
        let order = Order::value_buy(account, security, cad(dec!(25))).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(order.shares_at(cad(dec!(121.4))), Ok(qty(dec!(0.2059))));
        assert_eq!(order.shares_at(cad(dec!(1000000))), Err(Error::NonPositiveQuantity(dec!(0))));
        assert!(matches!(order.shares_at(usd(dec!(10))), Err(Error::CurrencyMismatch { .. })));
    }
}
