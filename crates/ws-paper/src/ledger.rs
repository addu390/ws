use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ws_core::{
    AccountId, Activity, IdempotencyKey, Money, Order, OrderId, Placed, Quantity, Quote, Report, Security, SecurityId, Side, Status,
    exchange_day,
};

use crate::book::{Book, Resting};
use crate::fill;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Lot {
    pub(crate) quantity: Decimal,
    pub(crate) cost: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Holdings {
    cash: Money,
    lots: BTreeMap<SecurityId, Lot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ledger {
    accounts: BTreeMap<AccountId, Holdings>,
    book: Book,
    history: Vec<Activity>,
    keys: BTreeMap<IdempotencyKey, OrderId>,
    #[serde(default)]
    finished: BTreeMap<IdempotencyKey, Report>,
    securities: BTreeMap<SecurityId, Security>,
    next: u64,
}

impl Ledger {
    pub(crate) fn place(&mut self, order: &Order, key: &IdempotencyKey, quote: &Quote, cash: Money, now: DateTime<Utc>) -> Result<OrderId, String> {
        if let Some(id) = self.keys.get(key) {
            return Ok(id.clone());
        }
        let holdings = self.accounts.entry(order.account().clone()).or_insert_with(|| Holdings { cash, lots: BTreeMap::new() });
        match order.side() {
            Side::Buy => {
                let needed = order.notional(quote).map_err(|e| e.to_string())?;
                if holdings.cash.amount() < needed.amount() {
                    return Err(format!("insufficient paper cash: {} available, about {needed} needed", holdings.cash));
                }
            }
            Side::Sell => {
                let held = holdings.lots.get(order.security()).map_or(Decimal::ZERO, |l| l.quantity);
                let selling = order.shares_at(quote.price()).map_err(|e| e.to_string())?;
                if held < selling.value() {
                    return Err(format!("only {held} shares held in paper"));
                }
            }
        }
        self.next += 1;
        let id = OrderId::parse(format!("paper-{}", self.next)).map_err(|e| e.to_string())?;
        self.keys.insert(key.clone(), id.clone());
        self.book.add(Resting { id: id.clone(), key: key.clone(), order: order.clone(), placed: now, day: exchange_day(now) });
        self.sweep(&HashMap::from([(quote.security().clone(), quote.clone())]), cash, now);
        Ok(id)
    }

    pub(crate) fn cancel(&mut self, key: &IdempotencyKey, now: DateTime<Utc>) -> bool {
        let Some(resting) = self.book.remove(key) else { return false };
        self.finish(resting.activity("CANCELLED", now), resting.report(Status::Cancelled));
        true
    }

    pub(crate) fn sweep(&mut self, quotes: &HashMap<SecurityId, Quote>, cash: Money, now: DateTime<Utc>) {
        let today = exchange_day(now);
        for resting in self.book.drain() {
            if resting.lapsed(today) {
                self.finish(resting.activity("EXPIRED", now), resting.report(Status::Expired));
                continue;
            }
            let Some(price) = quotes.get(resting.order.security()).and_then(|q| fill::price(&resting.order, q)) else {
                self.book.add(resting);
                continue;
            };
            let holdings = self.accounts.entry(resting.order.account().clone()).or_insert_with(|| Holdings { cash, lots: BTreeMap::new() });
            match execute(holdings, &resting.order, price) {
                Ok((amount, quantity)) => {
                    let symbol = self.securities.get(resting.order.security()).map_or_else(|| resting.order.security().to_string(), |s| s.symbol().to_owned());
                    let activity = resting.activity("FILLED", now).with_amount(amount).with_asset(symbol, Some(quantity.value()));
                    self.finish(activity, resting.report(Status::Filled).with_fill(quantity, price));
                }
                Err(reason) => {
                    let report = resting.report(Status::Rejected).with_rejection(reason.clone());
                    self.finish(resting.activity("REJECTED", now).with_subkind(reason), report);
                }
            }
        }
    }

    pub(crate) fn report(&self, key: &IdempotencyKey) -> Option<Report> {
        self.book.find(key).map(|r| r.report(Status::Placed)).or_else(|| self.finished.get(key).cloned())
    }

    fn finish(&mut self, activity: Activity, report: Report) {
        self.history.push(activity);
        self.finished.insert(report.key().clone(), report);
    }

    pub(crate) fn remember(&mut self, securities: &[Security]) {
        for security in securities {
            self.securities.insert(security.id().clone(), security.clone());
        }
    }

    pub(crate) fn securities_on_order(&self) -> Vec<SecurityId> {
        self.book.securities()
    }

    pub(crate) fn lots(&self, account: &AccountId) -> Vec<(Security, Lot)> {
        let Some(holdings) = self.accounts.get(account) else { return Vec::new() };
        holdings.lots.iter().map(|(id, lot)| (self.security(id), lot.clone())).collect()
    }

    pub(crate) fn cash(&self, account: &AccountId) -> Option<Money> {
        self.accounts.get(account).map(|h| h.cash)
    }

    pub(crate) fn history(&self, account: &AccountId, limit: usize) -> Vec<Activity> {
        self.history.iter().rev().filter(|a| a.account() == account).take(limit).cloned().collect()
    }

    pub(crate) fn pending(&self, account: &AccountId) -> Vec<Placed> {
        self.book.of(account).map(Resting::placed).collect()
    }

    fn security(&self, id: &SecurityId) -> Security {
        self.securities.get(id).cloned().unwrap_or_else(|| Security::new(id.clone(), id.as_str(), id.as_str()))
    }
}

fn execute(holdings: &mut Holdings, order: &Order, price: Money) -> Result<(Money, Quantity), String> {
    let shares = order.shares_at(price).map_err(|e| e.to_string())?;
    let quantity = shares.value();
    let value = price.times(quantity).map_err(|e| e.to_string())?;
    match order.side() {
        Side::Buy => {
            if holdings.cash.amount() < value.amount() {
                return Err(format!("insufficient paper cash: {} available, {value} needed", holdings.cash));
            }
            holdings.cash = holdings.cash.checked_sub(value).map_err(|e| e.to_string())?;
            let lot = holdings.lots.entry(order.security().clone()).or_insert(Lot { quantity: Decimal::ZERO, cost: Money::zero(value.currency()) });
            lot.quantity += quantity;
            lot.cost = lot.cost.checked_add(value).map_err(|e| e.to_string())?;
            Ok((Money::new(-value.amount(), value.currency()).map_err(|e| e.to_string())?, shares))
        }
        Side::Sell => {
            let lot = holdings.lots.get_mut(order.security()).filter(|l| l.quantity >= quantity);
            let Some(lot) = lot else { return Err("not enough paper shares".to_owned()) };
            let released = lot.cost.times(quantity / lot.quantity).map_err(|e| e.to_string())?;
            lot.quantity -= quantity;
            lot.cost = lot.cost.checked_sub(released).map_err(|e| e.to_string())?;
            if lot.quantity.is_zero() {
                holdings.lots.remove(order.security());
            }
            holdings.cash = holdings.cash.checked_add(value).map_err(|e| e.to_string())?;
            Ok((value, shares))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use rust_decimal::dec;
    use ws_core::MarketStatus;
    use ws_core::fixtures::{account, cad, limit_buy as buy, limit_sell as sell, noon as now, security};

    fn quote(price: Decimal) -> Quote {
        ws_core::fixtures::quote(price, MarketStatus::Open)
    }

    fn place(ledger: &mut Ledger, order: &Order, price: Decimal) -> Result<OrderId, String> {
        ledger.place(order, &IdempotencyKey::fresh(), &quote(price), cad(dec!(1000)), now())
    }

    #[test]
    fn crossing_buy_fills_and_moves_cash() {
        let mut ledger = Ledger::default();
        place(&mut ledger, &buy(dec!(10), dec!(40)), dec!(39)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ledger.cash(&account()), Some(cad(dec!(610))));
        let lots = ledger.lots(&account());
        assert_eq!(lots.len(), 1);
        assert_eq!(lots[0].1, Lot { quantity: dec!(10), cost: cad(dec!(390)) });
        assert_eq!(ledger.history(&account(), 5)[0].status(), Some("FILLED"));
        assert_eq!(ledger.history(&account(), 5)[0].amount(), Some(cad(dec!(-390))));
    }

    #[test]
    fn resting_orders_fill_on_a_later_sweep() {
        let mut ledger = Ledger::default();
        let id = place(&mut ledger, &buy(dec!(1), dec!(30)), dec!(40)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ledger.pending(&account()).len(), 1);
        ledger.sweep(&HashMap::from([(security(), quote(dec!(29)))]), cad(dec!(1000)), now());
        assert!(ledger.pending(&account()).is_empty());
        assert_eq!(ledger.history(&account(), 1)[0].id(), id.as_str());
        assert_eq!(ledger.cash(&account()), Some(cad(dec!(971))));
    }

    #[test]
    fn sells_release_cost_proportionally() {
        let mut ledger = Ledger::default();
        place(&mut ledger, &buy(dec!(4), dec!(40)), dec!(40)).unwrap_or_else(|e| panic!("{e}"));
        place(&mut ledger, &sell(dec!(1), dec!(50)), dec!(50)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ledger.lots(&account())[0].1, Lot { quantity: dec!(3), cost: cad(dec!(120)) });
        assert_eq!(ledger.cash(&account()), Some(cad(dec!(890))));
        place(&mut ledger, &sell(dec!(3), dec!(50)), dec!(50)).unwrap_or_else(|e| panic!("{e}"));
        assert!(ledger.lots(&account()).is_empty());
    }

    #[test]
    fn refuses_what_paper_cannot_cover() {
        let mut ledger = Ledger::default();
        assert!(place(&mut ledger, &buy(dec!(30), dec!(40)), dec!(40)).is_err_and(|e| e.contains("insufficient paper cash")));
        assert!(place(&mut ledger, &sell(dec!(1), dec!(40)), dec!(40)).is_err_and(|e| e.contains("only 0 shares")));
        assert!(ledger.pending(&account()).is_empty());
    }

    #[test]
    fn value_buys_fill_fractional_shares_and_wait_for_the_open() {
        let mut ledger = Ledger::default();
        let order = Order::value_buy(account(), security(), cad(dec!(25))).unwrap_or_else(|e| panic!("{e}"));
        let closed = ws_core::fixtures::quote(dec!(121.4), MarketStatus::Closed);
        ledger.place(&order, &IdempotencyKey::fresh(), &closed, cad(dec!(1000)), now()).unwrap_or_else(|e| panic!("{e}"));
        ledger.sweep(&HashMap::new(), cad(dec!(1000)), now() + Duration::days(3));
        assert_eq!(ledger.pending(&account()).len(), 1, "market orders do not lapse");
        ledger.sweep(&HashMap::from([(security(), quote(dec!(121.4)))]), cad(dec!(1000)), now() + Duration::days(3));
        assert_eq!(ledger.lots(&account())[0].1, Lot { quantity: dec!(0.2059), cost: cad(dec!(25)) });
        assert_eq!(ledger.cash(&account()), Some(cad(dec!(975))));
        assert_eq!(ledger.history(&account(), 1)[0].quantity(), Some(dec!(0.2059)));
    }

    #[test]
    fn reports_orders_through_to_how_they_ended() {
        let mut ledger = Ledger::default();
        let filled = IdempotencyKey::fresh();
        ledger.place(&buy(dec!(2), dec!(30)), &filled, &quote(dec!(40)), cad(dec!(1000)), now()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ledger.report(&filled).map(|r| r.status()), Some(Status::Placed));
        ledger.sweep(&HashMap::from([(security(), quote(dec!(29)))]), cad(dec!(1000)), now());
        let report = ledger.report(&filled).unwrap_or_else(|| panic!("no report"));
        assert_eq!((report.status(), report.filled().map(|q| q.value()), report.average()), (Status::Filled, Some(dec!(2)), Some(cad(dec!(29)))));

        let cancelled = IdempotencyKey::fresh();
        ledger.place(&buy(dec!(1), dec!(10)), &cancelled, &quote(dec!(40)), cad(dec!(1000)), now()).unwrap_or_else(|e| panic!("{e}"));
        ledger.cancel(&cancelled, now());
        assert_eq!(ledger.report(&cancelled).map(|r| r.status()), Some(Status::Cancelled));
        assert!(ledger.report(&IdempotencyKey::fresh()).is_none());
    }

    #[test]
    fn same_key_places_once() {
        let mut ledger = Ledger::default();
        let key = IdempotencyKey::fresh();
        let order = buy(dec!(1), dec!(30));
        let first = ledger.place(&order, &key, &quote(dec!(40)), cad(dec!(1000)), now());
        let second = ledger.place(&order, &key, &quote(dec!(40)), cad(dec!(1000)), now());
        assert_eq!(first, second);
        assert_eq!(ledger.pending(&account()).len(), 1);
    }

    #[test]
    fn day_orders_lapse_and_cancels_are_recorded() {
        let mut ledger = Ledger::default();
        let kept = IdempotencyKey::fresh();
        ledger.place(&buy(dec!(1), dec!(30)).good_till_cancelled(), &kept, &quote(dec!(40)), cad(dec!(1000)), now()).unwrap_or_else(|e| panic!("{e}"));
        place(&mut ledger, &buy(dec!(1), dec!(30)), dec!(40)).unwrap_or_else(|e| panic!("{e}"));
        ledger.sweep(&HashMap::new(), cad(dec!(1000)), now() + Duration::days(1));
        assert_eq!(ledger.history(&account(), 1)[0].status(), Some("EXPIRED"));
        assert_eq!(ledger.pending(&account()).len(), 1);
        assert!(ledger.cancel(&kept, now()));
        assert!(!ledger.cancel(&kept, now()));
        assert_eq!(ledger.history(&account(), 1)[0].status(), Some("CANCELLED"));
    }
}
