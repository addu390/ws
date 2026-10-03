use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ws_core::{
    AccountId, Activity, IdempotencyKey, Kind, Order, OrderId, Placed, Report, SecurityId, Side, Status, Tif,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Resting {
    pub(crate) id: OrderId,
    pub(crate) key: IdempotencyKey,
    pub(crate) order: Order,
    pub(crate) placed: DateTime<Utc>,
    pub(crate) day: NaiveDate,
}

impl Resting {
    pub(crate) fn lapsed(&self, today: NaiveDate) -> bool {
        self.order.tif() == Tif::Day && self.order.kind() != Kind::Market && self.day < today
    }

    pub(crate) fn activity(&self, status: &str, at: DateTime<Utc>) -> Activity {
        let kind = match self.order.side() {
            Side::Buy => "DIY_BUY",
            Side::Sell => "DIY_SELL",
        };
        Activity::new(self.id.as_str(), self.order.account().clone(), kind, at)
            .with_status(status)
            .with_asset(self.order.security().as_str(), self.order.shares().map(|q| q.value()))
    }

    pub(crate) fn placed(&self) -> Placed {
        let order = &self.order;
        let placed = Placed::new(
            self.id.clone(),
            order.account().clone(),
            order.security().clone(),
            order.side(),
            Status::Placed,
            self.placed,
        )
        .with_key(self.key.clone());
        let placed = match order.shares() {
            Some(quantity) => placed.with_quantity(quantity),
            None => placed,
        };
        match order.kind() {
            Kind::Market => placed,
            Kind::Limit { limit } => placed.with_limit(limit),
            Kind::StopLimit { stop, limit } => placed.with_stop(stop).with_limit(limit),
        }
    }

    pub(crate) fn report(&self, status: Status) -> Report {
        let order = &self.order;
        let report =
            Report::new(self.key.clone(), order.account().clone(), order.security().clone(), order.side(), status)
                .with_submitted(self.placed);
        let report = match order.shares() {
            Some(quantity) => report.with_quantity(quantity),
            None => report,
        };
        match order.kind() {
            Kind::Market => report,
            Kind::Limit { limit } => report.with_limit(limit),
            Kind::StopLimit { stop, limit } => report.with_stop(stop).with_limit(limit),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Book {
    open: Vec<Resting>,
}

impl Book {
    pub(crate) fn add(&mut self, resting: Resting) {
        self.open.push(resting);
    }

    pub(crate) fn remove(&mut self, key: &IdempotencyKey) -> Option<Resting> {
        let index = self.open.iter().position(|r| &r.key == key)?;
        Some(self.open.remove(index))
    }

    pub(crate) fn find(&self, key: &IdempotencyKey) -> Option<&Resting> {
        self.open.iter().find(|r| &r.key == key)
    }

    pub(crate) fn drain(&mut self) -> Vec<Resting> {
        std::mem::take(&mut self.open)
    }

    pub(crate) fn securities(&self) -> Vec<SecurityId> {
        let mut ids: Vec<SecurityId> = self.open.iter().map(|r| r.order.security().clone()).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    pub(crate) fn of<'a>(&'a self, account: &'a AccountId) -> impl Iterator<Item = &'a Resting> {
        self.open.iter().filter(move |r| r.order.account() == account)
    }
}
