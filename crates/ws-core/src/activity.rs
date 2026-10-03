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
    pub fn with_subkind(mut self, subkind: impl Into<String>) -> Self {
        self.subkind = Some(subkind.into());
        self
    }

    #[must_use]
    pub fn with_status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    #[must_use]
    pub fn with_amount(mut self, amount: Money) -> Self {
        self.amount = Some(amount);
        self
    }

    #[must_use]
    pub fn with_asset(mut self, symbol: impl Into<String>, quantity: Option<Decimal>) -> Self {
        self.symbol = Some(symbol.into());
        self.quantity = quantity;
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
    pub fn subkind(&self) -> Option<&str> {
        self.subkind.as_deref()
    }

    #[must_use]
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    #[must_use]
    pub fn occurred(&self) -> DateTime<Utc> {
        self.occurred
    }

    #[must_use]
    pub fn amount(&self) -> Option<Money> {
        self.amount
    }

    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    #[must_use]
    pub fn quantity(&self) -> Option<Decimal> {
        self.quantity
    }
}
