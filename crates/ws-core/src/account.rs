//! Accounts and their kinds.

use serde::{Deserialize, Serialize};

use crate::{AccountId, Currency};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Registration {
    Tfsa,
    Rrsp,
    SpousalRrsp,
    Fhsa,
    Resp,
    Rrif,
    Lira,
    NonRegistered,
    Margin,
    Cash,
    Crypto,
    CreditCard,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Management {
    SelfDirected,
    Managed,
    Neither,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    id: AccountId,
    registration: Registration,
    management: Management,
    currency: Currency,
    nickname: Option<String>,
}

impl Account {
    #[must_use]
    pub fn new(id: AccountId, registration: Registration, management: Management, currency: Currency) -> Self {
        Self { id, registration, management, currency, nickname: None }
    }

    #[must_use]
    pub fn named(mut self, nickname: impl Into<String>) -> Self {
        self.nickname = Some(nickname.into());
        self
    }

    #[must_use]
    pub fn id(&self) -> &AccountId {
        &self.id
    }

    #[must_use]
    pub fn registration(&self) -> &Registration {
        &self.registration
    }

    #[must_use]
    pub fn management(&self) -> Management {
        self.management
    }

    #[must_use]
    pub fn currency(&self) -> Currency {
        self.currency
    }

    #[must_use]
    pub fn nickname(&self) -> Option<&str> {
        self.nickname.as_deref()
    }

    /// Only self-directed accounts accept orders through this API.
    #[must_use]
    pub fn tradable(&self) -> bool {
        self.management == Management::SelfDirected
    }
}
