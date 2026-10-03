use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{AccountId, Currency, Valuation};

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
    LineOfCredit,
    Other(String),
}

impl fmt::Display for Registration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tfsa => "TFSA",
            Self::Rrsp => "RRSP",
            Self::SpousalRrsp => "Spousal RRSP",
            Self::Fhsa => "FHSA",
            Self::Resp => "RESP",
            Self::Rrif => "RRIF",
            Self::Lira => "LIRA",
            Self::NonRegistered => "Non-registered",
            Self::Margin => "Margin",
            Self::Cash => "Cash",
            Self::Crypto => "Crypto",
            Self::CreditCard => "Credit card",
            Self::LineOfCredit => "Line of credit",
            Self::Other(name) => name,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Management {
    SelfDirected,
    Managed,
    Automated,
    Neither,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    id: AccountId,
    registration: Registration,
    management: Management,
    currency: Currency,
    nickname: Option<String>,
    valuation: Option<Valuation>,
}

impl Account {
    #[must_use]
    pub fn new(id: AccountId, registration: Registration, management: Management, currency: Currency) -> Self {
        Self { id, registration, management, currency, nickname: None, valuation: None }
    }

    #[must_use]
    pub fn named(mut self, nickname: impl Into<String>) -> Self {
        self.nickname = Some(nickname.into());
        self
    }

    #[must_use]
    pub fn valued(mut self, valuation: Valuation) -> Self {
        self.valuation = Some(valuation);
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

    #[must_use]
    pub fn valuation(&self) -> Option<&Valuation> {
        self.valuation.as_ref()
    }

    #[must_use]
    pub fn tradable(&self) -> bool {
        self.management == Management::SelfDirected
    }
}
