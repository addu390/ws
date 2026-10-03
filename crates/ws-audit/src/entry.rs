use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ws_core::{IdempotencyKey, Order, OrderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Preview,
    Place,
    Cancel,
    Approve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Allowed,
    Denied,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    at: DateTime<Utc>,
    action: Action,
    verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<Order>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order_id: Option<OrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<IdempotencyKey>,
}

impl Entry {
    #[must_use]
    pub fn new(at: DateTime<Utc>, action: Action, verdict: Verdict) -> Self {
        Self { at, action, verdict, reason: None, order: None, order_id: None, key: None }
    }

    #[must_use]
    pub fn because(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    #[must_use]
    pub fn for_order(mut self, order: Order) -> Self {
        self.order = Some(order);
        self
    }

    #[must_use]
    pub fn with_order_id(mut self, id: OrderId) -> Self {
        self.order_id = Some(id);
        self
    }

    #[must_use]
    pub fn with_key(mut self, key: IdempotencyKey) -> Self {
        self.key = Some(key);
        self
    }

    #[must_use]
    pub fn at(&self) -> DateTime<Utc> {
        self.at
    }

    #[must_use]
    pub fn action(&self) -> Action {
        self.action
    }

    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    #[must_use]
    pub fn order(&self) -> Option<&Order> {
        self.order.as_ref()
    }

    #[must_use]
    pub fn order_id(&self) -> Option<&OrderId> {
        self.order_id.as_ref()
    }

    #[must_use]
    pub fn key(&self) -> Option<&IdempotencyKey> {
        self.key.as_ref()
    }
}
