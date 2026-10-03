use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AccountId, IdempotencyKey, Money, OrderId, Quantity, SecurityId, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    New,
    PendingSubmission,
    PendingReview,
    PendingFundTransfer,
    Submitted,
    Placed,
    PartiallyFilled,
    Contingent,
    CancelPending,
    Filled,
    Cancelled,
    Rejected,
    Expired,
}

impl Status {
    #[must_use]
    pub fn is_open(self) -> bool {
        !matches!(self, Self::Filled | Self::Cancelled | Self::Rejected | Self::Expired)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placed {
    id: OrderId,
    key: Option<IdempotencyKey>,
    account: AccountId,
    security: SecurityId,
    symbol: Option<String>,
    side: Side,
    status: Status,
    quantity: Option<Quantity>,
    limit: Option<Money>,
    stop: Option<Money>,
    created: DateTime<Utc>,
}

impl Placed {
    #[must_use]
    pub fn new(id: OrderId, account: AccountId, security: SecurityId, side: Side, status: Status, created: DateTime<Utc>) -> Self {
        Self { id, key: None, account, security, symbol: None, side, status, quantity: None, limit: None, stop: None, created }
    }

    #[must_use]
    pub fn with_key(mut self, key: IdempotencyKey) -> Self {
        self.key = Some(key);
        self
    }

    #[must_use]
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    #[must_use]
    pub fn with_quantity(mut self, quantity: Quantity) -> Self {
        self.quantity = Some(quantity);
        self
    }

    #[must_use]
    pub fn with_limit(mut self, limit: Money) -> Self {
        self.limit = Some(limit);
        self
    }

    #[must_use]
    pub fn with_stop(mut self, stop: Money) -> Self {
        self.stop = Some(stop);
        self
    }

    #[must_use]
    pub fn id(&self) -> &OrderId {
        &self.id
    }

    #[must_use]
    pub fn key(&self) -> Option<&IdempotencyKey> {
        self.key.as_ref()
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
    pub fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    #[must_use]
    pub fn side(&self) -> Side {
        self.side
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }

    #[must_use]
    pub fn quantity(&self) -> Option<Quantity> {
        self.quantity
    }

    #[must_use]
    pub fn limit(&self) -> Option<Money> {
        self.limit
    }

    #[must_use]
    pub fn stop(&self) -> Option<Money> {
        self.stop
    }

    #[must_use]
    pub fn created(&self) -> DateTime<Utc> {
        self.created
    }
}
