use rust_decimal::Decimal;
use ws_config::{Limits, Mode};
use ws_core::{Currency, Kind, MarketStatus, Money, Order, Quote, Side};

use crate::{Denial, Error, Usage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Assessment {
    pub(crate) value: Money,
    pub(crate) needs_approval: bool,
}

pub(crate) fn assess(
    mode: Mode,
    limits: &Limits,
    order: &Order,
    quote: &Quote,
    usage: Usage,
) -> Result<Assessment, Error> {
    if !mode.trades() {
        return Err(Denial::ReadOnly.into());
    }
    if !limits.allows(order.account()) {
        return Err(Denial::AccountNotAllowed(order.account().clone()).into());
    }
    if limits.blocks(order.security()) {
        return Err(Denial::Blocked(order.security().clone()).into());
    }
    if limits.limit_only() && order.kind() == Kind::Market {
        return Err(Denial::MarketOrder.into());
    }
    let price = quote.price();
    if price.currency() != Currency::Cad {
        return Err(Denial::Currency(price.currency()).into());
    }
    if limits.market_hours_only() && quote.market() != MarketStatus::Open {
        return Err(Denial::MarketClosed(quote.market()).into());
    }
    if let Kind::Limit { limit } | Kind::StopLimit { limit, .. } = order.kind() {
        deviation(order.side(), limit, price, limits.max_limit_deviation_pct())?;
    }

    let value = order.notional(quote)?;
    if value.currency() != Currency::Cad {
        return Err(Denial::Currency(value.currency()).into());
    }
    if value.amount() > limits.max_order_value().amount() {
        return Err(Denial::OrderTooLarge { value, max: limits.max_order_value() }.into());
    }
    if usage.orders() >= limits.max_orders_per_day() {
        return Err(Denial::TooManyOrders { max: limits.max_orders_per_day() }.into());
    }
    if order.side() == Side::Buy {
        let total = usage.spent().checked_add(value)?;
        if total.amount() > limits.max_daily_spend().amount() {
            return Err(Denial::DailySpend { spent: usage.spent(), value, max: limits.max_daily_spend() }.into());
        }
    }
    Ok(Assessment { value, needs_approval: value.amount() > limits.require_approval_above().amount() })
}

/// Only the costly direction counts: buying above the quote or selling below it.
fn deviation(side: Side, limit: Money, quote: Money, max_pct: Decimal) -> Result<(), Error> {
    if limit.currency() != quote.currency() {
        return Err(Denial::Currency(limit.currency()).into());
    }
    let worse = match side {
        Side::Buy => limit.amount() - quote.amount(),
        Side::Sell => quote.amount() - limit.amount(),
    };
    if worse * Decimal::ONE_HUNDRED > max_pct * quote.amount() {
        return Err(Denial::Deviation { limit, quote, max_pct }.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;
    use ws_config::Config;
    use ws_core::fixtures::{account_id, cad, limit_sell as sell, quantity, quote, security, usd};

    fn limits(extra: &str) -> Limits {
        let text = format!("[limits]\nallowed_accounts = [\"tfsa-abc\"]\n{extra}");
        Config::parse(&text).unwrap_or_else(|e| panic!("{e}")).limits().clone()
    }

    fn buy(account: &str, shares: Decimal, limit: Decimal) -> Order {
        Order::limit_buy(account_id(account), security(), quantity(shares), cad(limit))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn used(orders: u32, spent: Decimal) -> Usage {
        Usage::new(orders, cad(spent))
    }

    fn denial(limits: &Limits, order: &Order, quote: &Quote, usage: Usage) -> Option<Denial> {
        match assess(Mode::Trade, limits, order, quote, usage) {
            Err(Error::Denied(denial)) => Some(denial),
            _ => None,
        }
    }

    #[test]
    fn allows_an_ordinary_limit_buy() {
        let assessed = assess(
            Mode::Paper,
            &limits(""),
            &buy("tfsa-abc", dec!(2), dec!(40)),
            &quote(dec!(40), MarketStatus::Open),
            used(0, dec!(0)),
        );
        assert_eq!(assessed.ok(), Some(Assessment { value: cad(dec!(80)), needs_approval: false }));
    }

    #[test]
    fn read_mode_denies_everything() {
        let result = assess(
            Mode::Read,
            &limits(""),
            &buy("tfsa-abc", dec!(1), dec!(40)),
            &quote(dec!(40), MarketStatus::Open),
            used(0, dec!(0)),
        );
        assert!(matches!(result, Err(Error::Denied(Denial::ReadOnly))));
    }

    #[test]
    fn denies_accounts_securities_and_order_kinds_outside_the_rules() {
        let open = quote(dec!(40), MarketStatus::Open);
        let none = used(0, dec!(0));
        assert!(matches!(
            denial(&limits(""), &buy("rrsp-x", dec!(1), dec!(40)), &open, none),
            Some(Denial::AccountNotAllowed(_))
        ));
        let blocked = limits("blocked_securities = [\"sec-s-xeqt\"]\n");
        assert!(matches!(denial(&blocked, &buy("tfsa-abc", dec!(1), dec!(40)), &open, none), Some(Denial::Blocked(_))));
        let market = Order::market_buy(account_id("tfsa-abc"), security(), quantity(dec!(1)));
        assert_eq!(denial(&limits(""), &market, &open, none), Some(Denial::MarketOrder));
        assert!(denial(&limits("limit_only = false\n"), &market, &open, none).is_none());
    }

    #[test]
    fn denies_when_the_market_is_closed_unless_allowed() {
        let order = buy("tfsa-abc", dec!(1), dec!(40));
        let closed = quote(dec!(40), MarketStatus::Closed);
        assert_eq!(
            denial(&limits(""), &order, &closed, used(0, dec!(0))),
            Some(Denial::MarketClosed(MarketStatus::Closed))
        );
        assert!(denial(&limits("market_hours_only = false\n"), &order, &closed, used(0, dec!(0))).is_none());
    }

    #[test]
    fn denies_usd_securities() {
        let quote = Quote::new(security(), usd(dec!(40)), ws_core::fixtures::noon(), MarketStatus::Open);
        assert_eq!(
            denial(&limits(""), &buy("tfsa-abc", dec!(1), dec!(40)), &quote, used(0, dec!(0))),
            Some(Denial::Currency(Currency::Usd))
        );
    }

    #[test]
    fn deviation_only_blocks_the_costly_direction() {
        let open = quote(dec!(100), MarketStatus::Open);
        let none = used(0, dec!(0));
        let lim = limits("");
        assert!(denial(&lim, &buy("tfsa-abc", dec!(1), dec!(103)), &open, none).is_none());
        assert!(matches!(
            denial(&lim, &buy("tfsa-abc", dec!(1), dec!(103.01)), &open, none),
            Some(Denial::Deviation { .. })
        ));
        assert!(denial(&lim, &buy("tfsa-abc", dec!(1), dec!(50)), &open, none).is_none(), "a low bid cannot overpay");
        assert!(denial(&lim, &sell(dec!(1), dec!(97)), &open, none).is_none());
        assert!(matches!(denial(&lim, &sell(dec!(1), dec!(96.99)), &open, none), Some(Denial::Deviation { .. })));
        assert!(denial(&lim, &sell(dec!(1), dec!(150)), &open, none).is_none(), "a high ask cannot undersell");
    }

    #[test]
    fn enforces_order_value_count_and_daily_spend() {
        let open = quote(dec!(100), MarketStatus::Open);
        let lim = limits("");
        assert!(matches!(
            denial(&lim, &buy("tfsa-abc", dec!(6), dec!(100)), &open, used(0, dec!(0))),
            Some(Denial::OrderTooLarge { .. })
        ));
        assert_eq!(
            denial(&lim, &buy("tfsa-abc", dec!(1), dec!(100)), &open, used(5, dec!(0))),
            Some(Denial::TooManyOrders { max: 5 })
        );
        assert!(matches!(
            denial(&lim, &buy("tfsa-abc", dec!(2), dec!(100)), &open, used(1, dec!(900))),
            Some(Denial::DailySpend { .. })
        ));
        assert!(denial(&lim, &sell(dec!(2), dec!(100)), &open, used(1, dec!(900))).is_none(), "sells do not spend");
    }

    #[test]
    fn value_buys_are_market_orders_charged_at_their_amount() {
        let open = quote(dec!(121.4), MarketStatus::Open);
        let value =
            |amount| Order::value_buy(account_id("tfsa-abc"), security(), amount).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(denial(&limits(""), &value(cad(dec!(25))), &open, used(0, dec!(0))), Some(Denial::MarketOrder));
        let market = limits("limit_only = false\n");
        let assessed = assess(Mode::Trade, &market, &value(cad(dec!(25))), &open, used(0, dec!(0)));
        assert_eq!(assessed.ok(), Some(Assessment { value: cad(dec!(25)), needs_approval: false }));
        assert_eq!(
            denial(&market, &value(usd(dec!(25))), &open, used(0, dec!(0))),
            Some(Denial::Currency(Currency::Usd))
        );
    }

    #[test]
    fn large_orders_need_approval() {
        let open = quote(dec!(100), MarketStatus::Open);
        let lim = limits("");
        let small = assess(Mode::Trade, &lim, &buy("tfsa-abc", dec!(2.5), dec!(100)), &open, used(0, dec!(0)));
        assert_eq!(small.ok().map(|a| a.needs_approval), Some(false));
        let large = assess(Mode::Trade, &lim, &buy("tfsa-abc", dec!(3), dec!(100)), &open, used(0, dec!(0)));
        assert_eq!(large.ok().map(|a| a.needs_approval), Some(true));
    }
}
