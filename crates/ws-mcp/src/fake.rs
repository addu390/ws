use std::sync::Arc;

use async_trait::async_trait;
use rust_decimal::dec;
use ws_broker::{Error, Reader};
use ws_config::Config;
use ws_core::{
    Account, AccountId, Activity, Clock, Currency, FixedClock, IdempotencyKey, Management, MarketStatus, Placed,
    Position, Quote, Registration, Report, Security, SecurityId, fixtures,
};
use ws_paper::Paper;
use ws_policy::{Guard, Kill};

use crate::home::Home;

pub struct Market;

#[async_trait]
impl Reader for Market {
    async fn accounts(&self) -> Result<Vec<Account>, Error> {
        Ok(vec![
            Account::new(fixtures::account(), Registration::Tfsa, Management::SelfDirected, Currency::Cad)
                .named("Main"),
        ])
    }

    async fn positions(&self, _: &AccountId) -> Result<Vec<Position>, Error> {
        Ok(Vec::new())
    }

    async fn activities(&self, _: &AccountId, limit: usize) -> Result<Vec<Activity>, Error> {
        Err(Error::NotFound(format!("limit {limit}")))
    }

    async fn search(&self, _: &str) -> Result<Vec<Security>, Error> {
        Ok(Vec::new())
    }

    async fn quote(&self, security: &SecurityId) -> Result<Quote, Error> {
        if *security == fixtures::security() {
            return Ok(fixtures::quote(dec!(40), MarketStatus::Open));
        }
        Err(Error::NotFound(security.to_string()))
    }

    async fn pending_orders(&self, _: &AccountId) -> Result<Vec<Placed>, Error> {
        Ok(Vec::new())
    }

    async fn order(&self, key: &IdempotencyKey) -> Result<Report, Error> {
        Err(Error::NotFound(key.to_string()))
    }
}

pub fn paper(home: &Home) -> (Arc<Paper>, Arc<Guard>) {
    configured(home, "")
}

/// `limits` is extra lines for the `[limits]` table.
pub fn configured(home: &Home, limits: &str) -> (Arc<Paper>, Arc<Guard>) {
    let text = format!("mode = \"paper\"\n[limits]\nallowed_accounts = [\"{}\"]\n{limits}\n", fixtures::account());
    let config = Config::parse(&text).unwrap_or_else(|e| panic!("{e}"));
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::at(fixtures::noon()));
    let paper = Arc::new(Paper::new(Arc::new(Market), config.paper_cash(), home.ledger(), clock.clone()));
    let journal = Arc::new(ws_audit::File::open(home.audit()).unwrap_or_else(|e| panic!("{e}")));
    let guard = Guard::new(paper.clone(), &config, journal, Kill::new(home.kill()), &home.paper(), clock);
    (paper, Arc::new(guard))
}
