use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{AccountId, IdempotencyKey, Money, Quantity, SecurityId, Side, Status};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    key: IdempotencyKey,
    account: AccountId,
    security: SecurityId,
    side: Side,
    status: Status,
    quantity: Option<Quantity>,
    filled: Option<Quantity>,
    average: Option<Money>,
    limit: Option<Money>,
    stop: Option<Money>,
    submitted: Option<DateTime<Utc>>,
    expires: Option<DateTime<Utc>>,
    rejection: Option<String>,
}

impl Report {
    #[must_use]
    pub fn new(key: IdempotencyKey, account: AccountId, security: SecurityId, side: Side, status: Status) -> Self {
        Self {
            key,
            account,
            security,
            side,
            status,
            quantity: None,
            filled: None,
            average: None,
            limit: None,
            stop: None,
            submitted: None,
            expires: None,
            rejection: None,
        }
    }

    #[must_use]
    pub fn with_quantity(mut self, quantity: Quantity) -> Self {
        self.quantity = Some(quantity);
        self
    }

    #[must_use]
    pub fn with_fill(mut self, filled: Quantity, average: Money) -> Self {
        self.filled = Some(filled);
        self.average = Some(average);
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
    pub fn with_submitted(mut self, at: DateTime<Utc>) -> Self {
        self.submitted = Some(at);
        self
    }

    #[must_use]
    pub fn with_expiry(mut self, at: DateTime<Utc>) -> Self {
        self.expires = Some(at);
        self
    }

    #[must_use]
    pub fn with_rejection(mut self, reason: impl Into<String>) -> Self {
        self.rejection = Some(reason.into());
        self
    }

    #[must_use]
    pub fn key(&self) -> &IdempotencyKey {
        &self.key
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
    pub fn status(&self) -> Status {
        self.status
    }

    #[must_use]
    pub fn quantity(&self) -> Option<Quantity> {
        self.quantity
    }

    #[must_use]
    pub fn filled(&self) -> Option<Quantity> {
        self.filled
    }

    #[must_use]
    pub fn average(&self) -> Option<Money> {
        self.average
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
    pub fn submitted(&self) -> Option<DateTime<Utc>> {
        self.submitted
    }

    #[must_use]
    pub fn expires(&self) -> Option<DateTime<Utc>> {
        self.expires
    }

    #[must_use]
    pub fn rejection(&self) -> Option<&str> {
        self.rejection.as_deref()
    }
}
