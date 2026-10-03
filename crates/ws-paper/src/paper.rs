use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ws_broker::{Broker, Error, Reader};
use ws_common::Locked;
use ws_core::{
    Account, AccountId, Activity, Clock, IdempotencyKey, Money, Order, OrderId, Placed, Position, Quantity, Quote, Report, Security,
    SecurityId,
};

use crate::ledger::Ledger;

pub struct Paper {
    market: Arc<dyn Reader>,
    cash: Money,
    ledger: PathBuf,
    clock: Arc<dyn Clock>,
}

impl Paper {
    #[must_use]
    pub fn new(market: Arc<dyn Reader>, cash: Money, ledger: impl Into<PathBuf>, clock: Arc<dyn Clock>) -> Self {
        Self { market, cash, ledger: ledger.into(), clock }
    }

    pub fn cash(&self, account: &AccountId) -> Result<Money, Error> {
        Ok(self.read()?.cash(account).unwrap_or(self.cash))
    }

    /// Quotes are fetched before the ledger is locked.
    async fn sweep(&self) -> Result<(), Error> {
        let mut quotes = HashMap::new();
        for security in self.read()?.securities_on_order() {
            if let Ok(quote) = self.market.quote(&security).await {
                quotes.insert(security, quote);
            }
        }
        let now = self.clock.now();
        self.edit(|ledger| ledger.sweep(&quotes, self.cash, now))
    }

    fn read(&self) -> Result<Ledger, Error> {
        Ok(ws_common::read_json(&self.ledger).map_err(Error::backend)?.unwrap_or_default())
    }

    fn edit<T>(&self, change: impl FnOnce(&mut Ledger) -> T) -> Result<T, Error> {
        let mut ledger = Locked::<Ledger>::open(&self.ledger).map_err(Error::backend)?;
        let out = change(ledger.value_mut());
        ledger.save().map_err(Error::backend)?;
        Ok(out)
    }
}

#[async_trait]
impl Reader for Paper {
    async fn accounts(&self) -> Result<Vec<Account>, Error> {
        self.market.accounts().await
    }

    async fn positions(&self, account: &AccountId) -> Result<Vec<Position>, Error> {
        self.sweep().await?;
        let mut positions = Vec::new();
        for (security, lot) in self.read()?.lots(account) {
            let quote = self.market.quote(security.id()).await?;
            let value = quote.price().times(lot.quantity).map_err(Error::backend)?;
            let quantity = Quantity::new(lot.quantity).map_err(Error::backend)?;
            positions.push(Position::new(account.clone(), security, quantity, value).with_cost(lot.cost));
        }
        Ok(positions)
    }

    async fn activities(&self, account: &AccountId, limit: usize) -> Result<Vec<Activity>, Error> {
        self.sweep().await?;
        Ok(self.read()?.history(account, limit))
    }

    async fn search(&self, query: &str) -> Result<Vec<Security>, Error> {
        let found = self.market.search(query).await?;
        self.edit(|ledger| ledger.remember(&found))?;
        Ok(found)
    }

    async fn quote(&self, security: &SecurityId) -> Result<Quote, Error> {
        self.market.quote(security).await
    }

    async fn pending_orders(&self, account: &AccountId) -> Result<Vec<Placed>, Error> {
        self.sweep().await?;
        Ok(self.read()?.pending(account))
    }

    async fn order(&self, key: &IdempotencyKey) -> Result<Report, Error> {
        self.sweep().await?;
        self.read()?.report(key).ok_or_else(|| Error::NotFound(format!("paper order {key}")))
    }
}

#[async_trait]
impl Broker for Paper {
    async fn place(&self, order: &Order, key: &IdempotencyKey) -> Result<OrderId, Error> {
        let quote = self.market.quote(order.security()).await?;
        let now = self.clock.now();
        self.edit(|ledger| ledger.place(order, key, &quote, self.cash, now))?.map_err(Error::Rejected)
    }

    async fn cancel(&self, key: &IdempotencyKey) -> Result<(), Error> {
        let now = self.clock.now();
        if self.edit(|ledger| ledger.cancel(key, now))? {
            Ok(())
        } else {
            Err(Error::NotFound(format!("open paper order {key}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rust_decimal::{Decimal, dec};
    use ws_common::fixtures::Scratch;
    use ws_core::fixtures::{account, cad, limit_buy, noon, security};
    use ws_core::{FixedClock, MarketStatus};

    use super::*;

    struct Market(Mutex<Decimal>);

    impl Market {
        fn set(&self, price: Decimal) {
            *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = price;
        }
    }

    #[async_trait]
    impl Reader for Market {
        async fn accounts(&self) -> Result<Vec<Account>, Error> {
            Ok(Vec::new())
        }
        async fn positions(&self, _: &AccountId) -> Result<Vec<Position>, Error> {
            Err(Error::NotFound("real positions must not be used".into()))
        }
        async fn activities(&self, _: &AccountId, _: usize) -> Result<Vec<Activity>, Error> {
            Err(Error::NotFound("real activity must not be used".into()))
        }
        async fn search(&self, _: &str) -> Result<Vec<Security>, Error> {
            Ok(vec![Security::new(security(), "XEQT", "iShares Core Equity ETF Portfolio")])
        }
        async fn quote(&self, security: &SecurityId) -> Result<Quote, Error> {
            let price = *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(Quote::new(security.clone(), cad(price), noon(), MarketStatus::Open))
        }
        async fn pending_orders(&self, _: &AccountId) -> Result<Vec<Placed>, Error> {
            Err(Error::NotFound("real orders must not be used".into()))
        }
        async fn order(&self, _: &IdempotencyKey) -> Result<Report, Error> {
            Err(Error::NotFound("real orders must not be used".into()))
        }
    }

    fn paper(scratch: &Scratch) -> (Paper, Arc<Market>) {
        let market = Arc::new(Market(Mutex::new(dec!(40))));
        let paper = Paper::new(market.clone(), cad(dec!(1000)), scratch.path("ledger.json"), Arc::new(FixedClock::at(noon())));
        (paper, market)
    }

    #[tokio::test]
    async fn simulates_holdings_priced_from_the_market() {
        let scratch = Scratch::new();
        let (paper, market) = paper(&scratch);
        paper.search("xeqt").await.unwrap_or_else(|e| panic!("{e}"));
        paper.place(&limit_buy(dec!(2), dec!(40)), &IdempotencyKey::fresh()).await.unwrap_or_else(|e| panic!("{e}"));
        market.set(dec!(45));

        let positions = paper.positions(&account()).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].security().symbol(), "XEQT");
        assert_eq!(positions[0].value(), cad(dec!(90)));
        assert_eq!(positions[0].cost(), Some(cad(dec!(80))));
    }

    #[tokio::test]
    async fn resting_orders_fill_when_the_price_comes_down() {
        let scratch = Scratch::new();
        let (paper, market) = paper(&scratch);
        assert_eq!(paper.cash(&account()).ok(), Some(cad(dec!(1000))));
        let id = paper.place(&limit_buy(dec!(2), dec!(35)), &IdempotencyKey::fresh()).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(paper.pending_orders(&account()).await.map(|p| p.len()).ok(), Some(1));

        market.set(dec!(34));
        assert_eq!(paper.pending_orders(&account()).await.map(|p| p.len()).ok(), Some(0));
        let history = paper.activities(&account(), 10).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((history[0].id(), history[0].status()), (id.as_str(), Some("FILLED")));
        assert_eq!(paper.cash(&account()).ok(), Some(cad(dec!(932))));
    }

    #[tokio::test]
    async fn cancels_open_orders_only() {
        let scratch = Scratch::new();
        let (paper, _) = paper(&scratch);
        let key = IdempotencyKey::fresh();
        paper.place(&limit_buy(dec!(2), dec!(35)), &key).await.unwrap_or_else(|e| panic!("{e}"));
        assert!(paper.cancel(&key).await.is_ok());
        assert!(matches!(paper.cancel(&key).await, Err(Error::NotFound(_))));
    }

    #[tokio::test]
    async fn rejections_surface_as_broker_rejections() {
        let scratch = Scratch::new();
        let (paper, _) = paper(&scratch);
        let huge = limit_buy(dec!(100), dec!(40));
        assert!(matches!(paper.place(&huge, &IdempotencyKey::fresh()).await, Err(Error::Rejected(_))));
    }

    #[tokio::test]
    async fn a_corrupt_ledger_stops_paper_trading() {
        let scratch = Scratch::new();
        let (paper, _) = paper(&scratch);
        ws_common::write(&scratch.path("ledger.json"), b"{oops").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(paper.place(&limit_buy(dec!(1), dec!(40)), &IdempotencyKey::fresh()).await, Err(Error::Backend(_))));
    }
}
