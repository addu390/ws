use rust_decimal::{Decimal, dec};
use serde::Deserialize;
use ws_core::{AccountId, Currency, Money, SecurityId};

use crate::Error;

const CURRENCY: Currency = Currency::Cad;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    max_order_value: Money,
    max_daily_spend: Money,
    max_orders_per_day: u32,
    limit_only: bool,
    max_limit_deviation_pct: Decimal,
    market_hours_only: bool,
    allowed_accounts: Vec<AccountId>,
    blocked_securities: Vec<SecurityId>,
    require_approval_above: Money,
    approve_in_chat: bool,
}

impl Limits {
    #[must_use]
    pub fn max_order_value(&self) -> Money {
        self.max_order_value
    }

    #[must_use]
    pub fn max_daily_spend(&self) -> Money {
        self.max_daily_spend
    }

    #[must_use]
    pub fn max_orders_per_day(&self) -> u32 {
        self.max_orders_per_day
    }

    #[must_use]
    pub fn limit_only(&self) -> bool {
        self.limit_only
    }

    #[must_use]
    pub fn max_limit_deviation_pct(&self) -> Decimal {
        self.max_limit_deviation_pct
    }

    #[must_use]
    pub fn market_hours_only(&self) -> bool {
        self.market_hours_only
    }

    #[must_use]
    pub fn allows(&self, account: &AccountId) -> bool {
        self.allowed_accounts.contains(account)
    }

    #[must_use]
    pub fn blocks(&self, security: &SecurityId) -> bool {
        self.blocked_securities.contains(security)
    }

    #[must_use]
    pub fn require_approval_above(&self) -> Money {
        self.require_approval_above
    }

    #[must_use]
    pub fn approve_in_chat(&self) -> bool {
        self.approve_in_chat
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Raw {
    max_order_value: Decimal,
    max_daily_spend: Decimal,
    max_orders_per_day: u32,
    limit_only: bool,
    max_limit_deviation_pct: Decimal,
    market_hours_only: bool,
    allowed_accounts: Vec<String>,
    blocked_securities: Vec<String>,
    require_approval_above: Decimal,
    approve_in_chat: bool,
}

impl Default for Raw {
    fn default() -> Self {
        Self {
            max_order_value: dec!(500),
            max_daily_spend: dec!(1000),
            max_orders_per_day: 5,
            limit_only: true,
            max_limit_deviation_pct: dec!(3),
            market_hours_only: true,
            allowed_accounts: Vec::new(),
            blocked_securities: Vec::new(),
            require_approval_above: dec!(250),
            approve_in_chat: false,
        }
    }
}

impl TryFrom<Raw> for Limits {
    type Error = Error;

    fn try_from(raw: Raw) -> Result<Self, Error> {
        let limits = Self {
            max_order_value: positive("max_order_value", raw.max_order_value)?,
            max_daily_spend: positive("max_daily_spend", raw.max_daily_spend)?,
            max_orders_per_day: raw.max_orders_per_day,
            limit_only: raw.limit_only,
            max_limit_deviation_pct: raw.max_limit_deviation_pct,
            market_hours_only: raw.market_hours_only,
            allowed_accounts: raw.allowed_accounts.into_iter().map(AccountId::parse).collect::<Result<_, _>>()?,
            blocked_securities: raw.blocked_securities.into_iter().map(SecurityId::parse).collect::<Result<_, _>>()?,
            require_approval_above: positive("require_approval_above", raw.require_approval_above)?,
            approve_in_chat: raw.approve_in_chat,
        };
        if limits.max_orders_per_day == 0 {
            return Err(invalid("max_orders_per_day", "must be at least 1"));
        }
        if limits.max_order_value.amount() > limits.max_daily_spend.amount() {
            return Err(invalid("max_order_value", "must not exceed max_daily_spend"));
        }
        if limits.max_limit_deviation_pct <= Decimal::ZERO || limits.max_limit_deviation_pct > dec!(100) {
            return Err(invalid("max_limit_deviation_pct", "must be above 0 and at most 100"));
        }
        Ok(limits)
    }
}

pub(crate) fn positive(field: &'static str, amount: Decimal) -> Result<Money, Error> {
    let money = Money::new(amount, CURRENCY)?;
    if money.is_positive() { Ok(money) } else { Err(invalid(field, "must be positive")) }
}

fn invalid(field: &'static str, reason: &'static str) -> Error {
    Error::Invalid { field, reason }
}
