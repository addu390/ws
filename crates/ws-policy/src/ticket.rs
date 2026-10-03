use std::fmt;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ws_core::{IdempotencyKey, Money, Order};

const TTL: Duration = Duration::seconds(60);
const APPROVAL_TTL: Duration = Duration::minutes(10);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TicketId(String);

impl TicketId {
    fn fresh() -> Self {
        Self(format!("ticket-{}", uuid::Uuid::new_v4()))
    }

    /// Ids name files on disk, so only `ticket-` followed by lowercase letters, digits and dashes is accepted.
    pub fn parse(raw: impl Into<String>) -> Result<Self, ws_core::Error> {
        let raw = raw.into();
        let valid = raw.strip_prefix("ticket-").is_some_and(|rest| {
            !rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
        if valid { Ok(Self(raw)) } else { Err(ws_core::Error::InvalidId { kind: "ticket", value: raw }) }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for TicketId {
    type Err = ws_core::Error;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

impl TryFrom<String> for TicketId {
    type Error = ws_core::Error;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl From<TicketId> for String {
    fn from(id: TicketId) -> Self {
        id.0
    }
}

impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    NotNeeded,
    Pending,
    Granted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    id: TicketId,
    order: Order,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    value: Money,
    key: IdempotencyKey,
    expires: DateTime<Utc>,
    approval: Approval,
}

impl Ticket {
    pub(crate) fn issue(order: Order, symbol: Option<String>, value: Money, needs_approval: bool, now: DateTime<Utc>) -> Self {
        let (approval, ttl) = if needs_approval { (Approval::Pending, APPROVAL_TTL) } else { (Approval::NotNeeded, TTL) };
        Self { id: TicketId::fresh(), order, symbol, value, key: IdempotencyKey::fresh(), expires: now + ttl, approval }
    }

    pub(crate) fn approve(&mut self) {
        if self.approval == Approval::Pending {
            self.approval = Approval::Granted;
        }
    }

    #[must_use]
    pub fn id(&self) -> &TicketId {
        &self.id
    }

    #[must_use]
    pub fn order(&self) -> &Order {
        &self.order
    }

    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        self.symbol.as_deref()
    }

    #[must_use]
    pub fn value(&self) -> Money {
        self.value
    }

    #[must_use]
    pub fn key(&self) -> &IdempotencyKey {
        &self.key
    }

    #[must_use]
    pub fn expires(&self) -> DateTime<Utc> {
        self.expires
    }

    #[must_use]
    pub fn approval(&self) -> Approval {
        self.approval
    }

    #[must_use]
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_cannot_escape_the_ticket_directory() {
        assert!(TicketId::parse(TicketId::fresh().as_str()).is_ok());
        for bad in ["", "ticket-", "../ticket-1", "ticket-../x", "ticket-A", "ticket-1/2", "order-1"] {
            assert!(TicketId::parse(bad).is_err(), "{bad}");
        }
    }
}
