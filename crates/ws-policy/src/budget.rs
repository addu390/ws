use std::path::PathBuf;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ws_common::Locked;
use ws_core::{Currency, Money};

use crate::{Denial, Error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    orders: u32,
    spent: Money,
}

impl Usage {
    pub(crate) fn new(orders: u32, spent: Money) -> Self {
        Self { orders, spent }
    }

    #[must_use]
    pub fn orders(&self) -> u32 {
        self.orders
    }

    #[must_use]
    pub fn spent(&self) -> Money {
        self.spent
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Budget {
    path: PathBuf,
}

impl Budget {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub(crate) fn usage(&self, day: NaiveDate) -> Result<Usage, Error> {
        ws_common::read_json::<Record>(&self.path)?.unwrap_or_default().usage(day)
    }

    pub(crate) fn hold(&self, day: NaiveDate) -> Result<Hold, Error> {
        let record = Locked::<Record>::try_open(&self.path)?.ok_or(Denial::Busy)?;
        Ok(Hold { usage: record.value().usage(day)?, record, day })
    }
}

pub(crate) struct Hold {
    record: Locked<Record>,
    day: NaiveDate,
    usage: Usage,
}

impl Hold {
    pub(crate) fn usage(&self) -> Usage {
        self.usage
    }

/// Records the order before it is sent. A failed send is not refunded, since it may have reached the broker.
    pub(crate) fn charge(&mut self, value: Money, buy: bool) -> Result<(), Error> {
        let spent = if buy { self.usage.spent.checked_add(value)? } else { self.usage.spent };
        let usage = Usage::new(self.usage.orders.saturating_add(1), spent);
        *self.record.value_mut() = Record { day: Some(self.day), orders: usage.orders, spent: usage.spent.amount() };
        self.record.save()?;
        self.usage = usage;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    day: Option<NaiveDate>,
    orders: u32,
    spent: Decimal,
}

impl Record {
    fn usage(&self, day: NaiveDate) -> Result<Usage, Error> {
        if self.day == Some(day) {
            Ok(Usage::new(self.orders, Money::new(self.spent, Currency::Cad)?))
        } else {
            Ok(Usage::new(0, Money::zero(Currency::Cad)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;
    use ws_common::fixtures::Scratch;
    use ws_core::fixtures::cad;

    fn budget(scratch: &Scratch) -> Budget {
        Budget::new(scratch.path("budget.json"))
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap_or_default()
    }

    #[test]
    fn counts_orders_and_buys_only() {
        let scratch = Scratch::new();
        {
            let mut hold = budget(&scratch).hold(day(2)).unwrap_or_else(|e| panic!("{e}"));
            hold.charge(cad(dec!(100)), true).unwrap_or_else(|e| panic!("{e}"));
            hold.charge(cad(dec!(40)), false).unwrap_or_else(|e| panic!("{e}"));
        }
        let usage = budget(&scratch).usage(day(2)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((usage.orders(), usage.spent()), (2, cad(dec!(100))));
    }

    #[test]
    fn resets_on_a_new_day() {
        let scratch = Scratch::new();
        budget(&scratch).hold(day(2)).and_then(|mut h| h.charge(cad(dec!(100)), true)).unwrap_or_else(|e| panic!("{e}"));
        let usage = budget(&scratch).usage(day(3)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((usage.orders(), usage.spent()), (0, cad(dec!(0))));
    }

    #[test]
    fn a_second_hold_is_busy() {
        let scratch = Scratch::new();
        let _first = budget(&scratch).hold(day(2)).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(budget(&scratch).hold(day(2)), Err(Error::Denied(Denial::Busy))));
    }

    #[test]
    fn corrupt_state_fails_closed() {
        let scratch = Scratch::new();
        ws_common::write(&scratch.path("budget.json"), b"{oops").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(budget(&scratch).hold(day(2)), Err(Error::State(ws_common::Error::Corrupt { .. }))));
    }
}
