//! Holdings within an account.

use serde::{Deserialize, Serialize};

use crate::{AccountId, Money, Quantity, Security};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    account: AccountId,
    security: Security,
    quantity: Quantity,
    value: Money,
    cost: Option<Money>,
}

impl Position {
    #[must_use]
    pub fn new(account: AccountId, security: Security, quantity: Quantity, value: Money) -> Self {
        Self { account, security, quantity, value, cost: None }
    }

    #[must_use]
    pub fn with_cost(mut self, cost: Money) -> Self {
        self.cost = Some(cost);
        self
    }

    #[must_use]
    pub fn account(&self) -> &AccountId {
        &self.account
    }

    #[must_use]
    pub fn security(&self) -> &Security {
        &self.security
    }

    #[must_use]
    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    #[must_use]
    pub fn value(&self) -> Money {
        self.value
    }

    #[must_use]
    pub fn cost(&self) -> Option<Money> {
        self.cost
    }
}
