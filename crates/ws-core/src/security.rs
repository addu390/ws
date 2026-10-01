//! Tradable securities.

use serde::{Deserialize, Serialize};

use crate::SecurityId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Security {
    id: SecurityId,
    symbol: String,
    name: String,
    exchange: Option<String>,
    buyable: bool,
}

impl Security {
    #[must_use]
    pub fn new(id: SecurityId, symbol: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id, symbol: symbol.into(), name: name.into(), exchange: None, buyable: true }
    }

    #[must_use]
    pub fn on(mut self, exchange: impl Into<String>) -> Self {
        self.exchange = Some(exchange.into());
        self
    }

    #[must_use]
    pub fn unbuyable(mut self) -> Self {
        self.buyable = false;
        self
    }

    #[must_use]
    pub fn id(&self) -> &SecurityId {
        &self.id
    }

    #[must_use]
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn exchange(&self) -> Option<&str> {
        self.exchange.as_deref()
    }

    #[must_use]
    pub fn buyable(&self) -> bool {
        self.buyable
    }
}
