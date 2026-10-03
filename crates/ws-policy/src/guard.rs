use std::fmt;
use std::path::Path;
use std::sync::Arc;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use ws_audit::{Action, Entry, Journal, Verdict};
use ws_broker::{Broker, Reader};
use ws_config::{Config, Limits, Mode};
use ws_core::{Clock, IdempotencyKey, Money, Order, OrderId, Side, exchange_day};

use crate::budget::Budget;
use crate::check::{self, Assessment};
use crate::desk::Desk;
use crate::{Approval, Denial, Error, Kill, Ticket, TicketId, Usage};

pub struct Guard {
    broker: Arc<dyn Broker>,
    mode: Mode,
    limits: Limits,
    journal: Arc<dyn Journal>,
    kill: Kill,
    desk: Desk,
    budget: Budget,
    clock: Arc<dyn Clock>,
}

impl Guard {
    #[must_use]
    pub fn new(
        broker: Arc<dyn Broker>,
        config: &Config,
        journal: Arc<dyn Journal>,
        kill: Kill,
        dir: &Path,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            broker,
            mode: config.mode(),
            limits: config.limits().clone(),
            journal,
            kill,
            desk: Desk::new(dir.join("tickets")),
            budget: Budget::new(dir.join("budget.json")),
            clock,
        }
    }

    pub async fn preview(&self, order: Order, symbol: Option<String>) -> Result<Ticket, Error> {
        let assessed = self.settle(Action::Preview, Some(&order), self.screen_today(&order).await)?;
        let ticket = Ticket::issue(order, symbol, assessed.value, assessed.needs_approval, self.clock.now());
        self.desk.file(&ticket)?;
        let reason = if assessed.needs_approval { "needs approval" } else { "ready to place" };
        self.record(
            &Entry::new(self.clock.now(), Action::Preview, Verdict::Allowed)
                .for_order(ticket.order().clone())
                .because(reason),
        )?;
        Ok(ticket)
    }

    /// Re-runs every check under the budget lock, then sends the ticket's order once.
    pub async fn place(&self, id: &TicketId) -> Result<Receipt, Error> {
        let ticket = self.claim(id);
        let ticket = self.settle(Action::Place, None, ticket)?;
        let order = ticket.order().clone();
        let charged = self.charge(&ticket).await;
        self.settle(Action::Place, Some(&order), charged)?;
        self.record(&Entry::new(self.clock.now(), Action::Place, Verdict::Allowed).for_order(order.clone()))?;

        match self.broker.place(&order, ticket.key()).await {
            Ok(placed) => {
                let entry = Entry::new(self.clock.now(), Action::Place, Verdict::Succeeded)
                    .for_order(order)
                    .with_order_id(placed.clone())
                    .with_key(ticket.key().clone());
                self.record(&entry)?;
                Ok(Receipt { order_id: placed, key: ticket.key().clone() })
            }
            Err(e) => {
                let entry = Entry::new(self.clock.now(), Action::Place, Verdict::Failed)
                    .for_order(order)
                    .with_key(ticket.key().clone());
                self.record(&entry.because(e.to_string()))?;
                Err(e.into())
            }
        }
    }

    pub async fn cancel(&self, key: &IdempotencyKey) -> Result<(), Error> {
        let allowed = if self.kill.engaged() {
            Err(Denial::Killed)
        } else if self.mode.trades() {
            Ok(())
        } else {
            Err(Denial::ReadOnly)
        };
        if let Err(denial) = allowed {
            self.record(
                &Entry::new(self.clock.now(), Action::Cancel, Verdict::Denied)
                    .with_key(key.clone())
                    .because(denial.to_string()),
            )?;
            return Err(denial.into());
        }
        match self.broker.cancel(key).await {
            Ok(()) => {
                self.record(&Entry::new(self.clock.now(), Action::Cancel, Verdict::Succeeded).with_key(key.clone()))?;
                Ok(())
            }
            Err(e) => {
                self.record(
                    &Entry::new(self.clock.now(), Action::Cancel, Verdict::Failed)
                        .with_key(key.clone())
                        .because(e.to_string()),
                )?;
                Err(e.into())
            }
        }
    }

    pub fn ticket(&self, id: &TicketId) -> Result<Ticket, Error> {
        self.desk.get(id)?.ok_or_else(|| Denial::UnknownTicket(id.clone()).into())
    }

    pub fn awaiting_approval(&self) -> Result<Vec<Ticket>, Error> {
        let now = self.clock.now();
        let mut tickets: Vec<Ticket> =
            self.desk.list()?.into_iter().filter(|t| t.approval() == Approval::Pending && !t.is_expired(now)).collect();
        tickets.sort_by_key(Ticket::expires);
        Ok(tickets)
    }

    /// For people only. Never expose it as an MCP tool, since the agent must not be able to approve its own orders.
    pub fn approve(&self, id: &TicketId, by: Approver) -> Result<Ticket, Error> {
        if by == Approver::Chat && !self.limits.approve_in_chat() {
            return Err(Denial::ChatApprovalOff.into());
        }
        let mut ticket = self.ticket(id)?;
        if ticket.is_expired(self.clock.now()) {
            return Err(Denial::Expired(id.clone()).into());
        }
        ticket.approve();
        self.desk.file(&ticket)?;
        let entry = Entry::new(self.clock.now(), Action::Approve, Verdict::Succeeded).for_order(ticket.order().clone());
        self.record(&entry.because(by.to_string()))?;
        Ok(ticket)
    }

    #[must_use]
    pub fn approves_in_chat(&self) -> bool {
        self.limits.approve_in_chat()
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Reads from the same broker orders go to, so paper orders resolve against paper state.
    #[must_use]
    pub fn reader(&self) -> &dyn Reader {
        self.broker.as_ref()
    }

    pub fn status(&self) -> Result<Status, Error> {
        let day = self.today();
        let usage = self.budget.usage(day)?;
        Ok(Status {
            mode: self.mode,
            killed: self.kill.engaged(),
            day,
            orders_today: usage.orders(),
            max_orders_per_day: self.limits.max_orders_per_day(),
            spent_today: usage.spent(),
            max_daily_spend: self.limits.max_daily_spend(),
            max_order_value: self.limits.max_order_value(),
            require_approval_above: self.limits.require_approval_above(),
            limit_only: self.limits.limit_only(),
            max_limit_deviation_pct: self.limits.max_limit_deviation_pct(),
            market_hours_only: self.limits.market_hours_only(),
        })
    }

    async fn screen_today(&self, order: &Order) -> Result<Assessment, Error> {
        let usage = self.budget.usage(self.today())?;
        self.screen(order, usage).await
    }

    /// The kill switch is checked first, so a halted guard makes no network calls.
    async fn screen(&self, order: &Order, usage: Usage) -> Result<Assessment, Error> {
        if self.kill.engaged() {
            return Err(Denial::Killed.into());
        }
        let quote = self.broker.quote(order.security()).await?;
        check::assess(self.mode, &self.limits, order, &quote, usage)
    }

    fn claim(&self, id: &TicketId) -> Result<Ticket, Error> {
        let ticket = self.ticket(id)?;
        if ticket.is_expired(self.clock.now()) {
            self.desk.take(id)?;
            return Err(Denial::Expired(id.clone()).into());
        }
        if ticket.approval() == Approval::Pending {
            return Err(Denial::NeedsApproval(id.clone()).into());
        }
        self.desk.take(id)?.ok_or_else(|| Denial::UnknownTicket(id.clone()).into())
    }

    async fn charge(&self, ticket: &Ticket) -> Result<(), Error> {
        let mut hold = self.budget.hold(self.today())?;
        let assessed = self.screen(ticket.order(), hold.usage()).await?;
        if assessed.needs_approval && ticket.approval() != Approval::Granted {
            return Err(Denial::Moved(ticket.id().clone()).into());
        }
        hold.charge(assessed.value, ticket.order().side() == Side::Buy)
    }

    fn settle<T>(&self, action: Action, order: Option<&Order>, result: Result<T, Error>) -> Result<T, Error> {
        if let Err(error) = &result {
            let verdict = if matches!(error, Error::Denied(_)) { Verdict::Denied } else { Verdict::Failed };
            let mut entry = Entry::new(self.clock.now(), action, verdict).because(error.to_string());
            if let Some(order) = order {
                entry = entry.for_order(order.clone());
            }
            self.record(&entry)?;
        }
        result
    }

    fn record(&self, entry: &Entry) -> Result<(), Error> {
        Ok(self.journal.record(entry)?)
    }

    fn today(&self) -> NaiveDate {
        exchange_day(self.clock.now())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approver {
    Terminal,
    Dashboard,
    /// The MCP client's own prompt. Weaker than the others, since the client could answer it without the user.
    Chat,
}

impl fmt::Display for Approver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Terminal => "approved in the terminal",
            Self::Dashboard => "approved on the dashboard",
            Self::Chat => "approved in the chat",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Receipt {
    pub order_id: OrderId,
    pub key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    mode: Mode,
    killed: bool,
    day: NaiveDate,
    orders_today: u32,
    max_orders_per_day: u32,
    spent_today: Money,
    max_daily_spend: Money,
    max_order_value: Money,
    require_approval_above: Money,
    limit_only: bool,
    max_limit_deviation_pct: Decimal,
    market_hours_only: bool,
}

impl Status {
    #[must_use]
    pub fn killed(&self) -> bool {
        self.killed
    }

    #[must_use]
    pub fn orders_today(&self) -> u32 {
        self.orders_today
    }

    #[must_use]
    pub fn spent_today(&self) -> Money {
        self.spent_today
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Duration, Utc};
    use rust_decimal::dec;
    use ws_audit::Memory;
    use ws_broker::Reader;
    use ws_common::fixtures::Scratch;
    use ws_core::fixtures::{cad, limit_buy as buy, noon};
    use ws_core::{
        Account, AccountId, Activity, IdempotencyKey, MarketStatus, Placed, Position, Quote, Report, Security,
        SecurityId,
    };

    use super::*;

    struct Fake {
        price: Mutex<Decimal>,
        placed: Mutex<Vec<(Order, IdempotencyKey)>>,
        reject: bool,
    }

    #[async_trait]
    impl Reader for Fake {
        async fn accounts(&self) -> Result<Vec<Account>, ws_broker::Error> {
            Ok(Vec::new())
        }
        async fn positions(&self, _: &AccountId) -> Result<Vec<Position>, ws_broker::Error> {
            Ok(Vec::new())
        }
        async fn activities(&self, _: &AccountId, _: usize) -> Result<Vec<Activity>, ws_broker::Error> {
            Ok(Vec::new())
        }
        async fn search(&self, _: &str) -> Result<Vec<Security>, ws_broker::Error> {
            Ok(Vec::new())
        }
        async fn quote(&self, security: &SecurityId) -> Result<Quote, ws_broker::Error> {
            let price = *self.price.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(Quote::new(security.clone(), cad(price), noon(), MarketStatus::Open))
        }
        async fn pending_orders(&self, _: &AccountId) -> Result<Vec<Placed>, ws_broker::Error> {
            Ok(Vec::new())
        }
        async fn order(&self, key: &IdempotencyKey) -> Result<Report, ws_broker::Error> {
            Err(ws_broker::Error::NotFound(key.to_string()))
        }
    }

    #[async_trait]
    impl Broker for Fake {
        async fn place(&self, order: &Order, key: &IdempotencyKey) -> Result<OrderId, ws_broker::Error> {
            if self.reject {
                return Err(ws_broker::Error::Rejected("insufficient funds".into()));
            }
            self.placed.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((order.clone(), key.clone()));
            OrderId::parse("order-1").map_err(|e| ws_broker::Error::Rejected(e.to_string()))
        }
        async fn cancel(&self, _: &IdempotencyKey) -> Result<(), ws_broker::Error> {
            Ok(())
        }
    }

    struct Ticking(Mutex<DateTime<Utc>>);

    impl Ticking {
        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += by;
        }
    }

    impl Clock for Ticking {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }

    struct Rig {
        guard: Guard,
        broker: Arc<Fake>,
        journal: Arc<Memory>,
        clock: Arc<Ticking>,
        scratch: Scratch,
    }

    impl Rig {
        fn new(mode: &str) -> Self {
            Self::with(mode, false)
        }

        fn with(mode: &str, reject: bool) -> Self {
            let scratch = Scratch::new();
            let config = Config::parse(&format!("mode = \"{mode}\"\n[limits]\nallowed_accounts = [\"tfsa-abc\"]\n"))
                .unwrap_or_else(|e| panic!("{e}"));
            let broker = Arc::new(Fake { price: Mutex::new(dec!(40)), placed: Mutex::default(), reject });
            let journal = Arc::new(Memory::default());
            let clock = Arc::new(Ticking(Mutex::new(noon())));
            let kill = Kill::new(scratch.path("KILL"));
            let guard = Guard::new(broker.clone(), &config, journal.clone(), kill, scratch.dir(), clock.clone());
            Self { guard, broker, journal, clock, scratch }
        }

        fn placed(&self) -> usize {
            self.broker.placed.lock().map(|p| p.len()).unwrap_or_default()
        }

        fn verdicts(&self) -> Vec<(Action, Verdict)> {
            self.journal.entries().iter().map(|e| (e.action(), e.verdict())).collect()
        }

        fn kill(&self) {
            ws_common::write(&self.scratch.path("KILL"), b"").unwrap_or_else(|e| panic!("{e}"));
        }
    }

    fn denied(result: Result<impl std::fmt::Debug, Error>) -> Denial {
        match result {
            Err(Error::Denied(denial)) => denial,
            other => panic!("expected a denial, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn previewed_ticket_places_once_with_its_key() {
        let rig = Rig::new("trade");
        let ticket = rig.guard.preview(buy(dec!(2), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ticket.value(), cad(dec!(80)));

        let placed = rig.guard.place(ticket.id()).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((placed.order_id.as_str(), &placed.key), ("order-1", ticket.key()));
        let sent = rig.broker.placed.lock().map(|p| p.clone()).unwrap_or_default();
        assert_eq!(sent, vec![(ticket.order().clone(), ticket.key().clone())]);

        assert_eq!(denied(rig.guard.place(ticket.id()).await), Denial::UnknownTicket(ticket.id().clone()));
        assert_eq!(rig.placed(), 1);
        let status = rig.guard.status().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((status.orders_today, status.spent_today), (1, cad(dec!(80))));
        assert_eq!(
            rig.verdicts(),
            vec![
                (Action::Preview, Verdict::Allowed),
                (Action::Place, Verdict::Allowed),
                (Action::Place, Verdict::Succeeded),
                (Action::Place, Verdict::Denied),
            ]
        );
    }

    #[tokio::test]
    async fn read_mode_refuses_and_records_why() {
        let rig = Rig::new("read");
        assert_eq!(denied(rig.guard.preview(buy(dec!(1), dec!(40)), None).await), Denial::ReadOnly);
        let entries = rig.journal.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].verdict(), Verdict::Denied);
        assert!(entries[0].reason().is_some_and(|r| r.contains("read mode")));
        assert!(entries[0].order().is_some());
    }

    #[tokio::test]
    async fn expired_tickets_cannot_be_placed() {
        let rig = Rig::new("trade");
        let ticket = rig.guard.preview(buy(dec!(1), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
        rig.clock.advance(Duration::seconds(61));
        assert_eq!(denied(rig.guard.place(ticket.id()).await), Denial::Expired(ticket.id().clone()));
        assert_eq!(rig.placed(), 0);
    }

    #[tokio::test]
    async fn kill_switch_stops_placement_after_preview() {
        let rig = Rig::new("trade");
        let ticket = rig.guard.preview(buy(dec!(1), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
        rig.kill();
        assert_eq!(denied(rig.guard.place(ticket.id()).await), Denial::Killed);
        assert_eq!(denied(rig.guard.cancel(&IdempotencyKey::fresh()).await), Denial::Killed);
        assert_eq!(rig.placed(), 0);
        assert!(rig.guard.status().is_ok_and(|s| s.killed));
    }

    #[tokio::test]
    async fn placing_rechecks_against_a_fresh_quote() {
        let rig = Rig::new("trade");
        let ticket = rig.guard.preview(buy(dec!(1), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
        *rig.broker.price.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = dec!(30);
        assert!(matches!(denied(rig.guard.place(ticket.id()).await), Denial::Deviation { .. }));
        assert_eq!(rig.placed(), 0);
        assert_eq!(rig.guard.status().map(|s| s.orders_today).ok(), Some(0));
    }

    #[tokio::test]
    async fn large_orders_wait_for_approval() {
        let rig = Rig::new("trade");
        let ticket = rig.guard.preview(buy(dec!(7), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ticket.approval(), Approval::Pending);
        assert_eq!(denied(rig.guard.place(ticket.id()).await), Denial::NeedsApproval(ticket.id().clone()));
        assert_eq!(rig.placed(), 0);
        rig.guard.preview(buy(dec!(1), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
        let waiting = rig.guard.awaiting_approval().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(waiting.iter().map(Ticket::id).collect::<Vec<_>>(), [ticket.id()]);

        rig.clock.advance(Duration::minutes(5));
        assert_eq!(denied(rig.guard.approve(ticket.id(), Approver::Chat)), Denial::ChatApprovalOff);
        assert_eq!(
            rig.guard.approve(ticket.id(), Approver::Terminal).map(|t| t.approval()).ok(),
            Some(Approval::Granted)
        );
        assert_eq!(rig.journal.entries().last().and_then(|e| e.reason()), Some("approved in the terminal"));
        assert!(rig.guard.awaiting_approval().is_ok_and(|w| w.is_empty()));
        assert!(rig.guard.place(ticket.id()).await.is_ok());
        assert_eq!(rig.placed(), 1);
    }

    #[tokio::test]
    async fn broker_failures_are_recorded_and_still_count() {
        let rig = Rig::with("paper", true);
        let ticket = rig.guard.preview(buy(dec!(1), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(rig.guard.place(ticket.id()).await, Err(Error::Broker(ws_broker::Error::Rejected(_)))));
        assert_eq!(rig.verdicts().last(), Some(&(Action::Place, Verdict::Failed)));
        assert_eq!(rig.guard.status().map(|s| s.orders_today).ok(), Some(1));
    }

    #[tokio::test]
    async fn daily_limits_hold_across_orders() {
        let rig = Rig::new("trade");
        for _ in 0..5 {
            let ticket = rig.guard.preview(buy(dec!(1), dec!(40)), None).await.unwrap_or_else(|e| panic!("{e}"));
            rig.guard.place(ticket.id()).await.unwrap_or_else(|e| panic!("{e}"));
        }
        assert_eq!(denied(rig.guard.preview(buy(dec!(1), dec!(40)), None).await), Denial::TooManyOrders { max: 5 });
    }
}
