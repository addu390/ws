//! Account activity feed items: trades, deposits, dividends, and so on.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{AccountId, Money};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    id: String,
    account: AccountId,
    kind: String,
    subkind: Option<String>,
    status: Option<String>,
    occurred: DateTime<Utc>,
    amount: Option<Money>,
    symbol: Option<String>,
    quantity: Option<Decimal>,
}

impl Activity {
    #[must_use]
    pub fn new(id: impl Into<String>, account: AccountId, kind: impl Into<String>, occurred: DateTime<Utc>) -> Self {
        Self {
            id: id.into(),
            account,
            kind: kind.into(),
            subkind: None,
            status: None,
            occurred,
            amount: None,
            symbol: None,
            quantity: None,
        }
    }

    #[must_use]
    pub fn subkind(mut self, subkind: impl Into<String>) -> Self {
        self.subkind = Some(subkind.into());
        self
    }

    #[must_use]
    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    #[must_use]
    pub fn amount(mut self, amount: Money) -> Self {
        self.amount = Some(amount);
        self
    }

    #[must_use]
    pub fn asset(mut self, symbol: impl Into<String>, quantity: Decimal) -> Self {
        self.symbol = Some(symbol.into());
        self.quantity = Some(quantity);
        self
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn account(&self) -> &AccountId {
        &self.account
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub fn occurred(&self) -> DateTime<Utc> {
        self.occurred
    }
}
