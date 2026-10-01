//! Price quotes and market status.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Money, SecurityId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketStatus {
    Open,
    Closed,
    PreMarket,
    AfterHours,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    security: SecurityId,
    price: Money,
    at: DateTime<Utc>,
    market: MarketStatus,
}

impl Quote {
    #[must_use]
    pub fn new(security: SecurityId, price: Money, at: DateTime<Utc>, market: MarketStatus) -> Self {
        Self { security, price, at, market }
    }

    #[must_use]
    pub fn security(&self) -> &SecurityId {
        &self.security
    }

    #[must_use]
    pub fn price(&self) -> Money {
        self.price
    }

    #[must_use]
    pub fn at(&self) -> DateTime<Utc> {
        self.at
    }

    #[must_use]
    pub fn market(&self) -> MarketStatus {
        self.market
    }
}
