use ws_core::{Kind, MarketStatus, Money, Order, Quote, Side};

pub(crate) fn price(order: &Order, quote: &Quote) -> Option<Money> {
    let price = quote.price();
    if quote.market() != MarketStatus::Open || quote.security() != order.security() {
        return None;
    }
    let crosses = |limit: Money| {
        limit.currency() == price.currency()
            && match order.side() {
                Side::Buy => price.amount() <= limit.amount(),
                Side::Sell => price.amount() >= limit.amount(),
            }
    };
    let fills = match order.kind() {
        Kind::Market => true,
        Kind::Limit { limit } => crosses(limit),
        Kind::StopLimit { stop, limit } => {
            let triggered = match order.side() {
                Side::Buy => price.amount() >= stop.amount(),
                Side::Sell => price.amount() <= stop.amount(),
            };
            triggered && crosses(limit)
        }
    };
    fills.then_some(price)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;
    use ws_core::fixtures::{account, cad, limit_buy, limit_sell, quantity, quote, security};

    #[test]
    fn market_orders_fill_at_the_quote_while_open() {
        let order = Order::market_buy(account(), security(), quantity(dec!(1)));
        assert_eq!(price(&order, &quote(dec!(40), MarketStatus::Open)), Some(cad(dec!(40))));
        assert_eq!(price(&order, &quote(dec!(40), MarketStatus::Closed)), None);
    }

    #[test]
    fn limits_fill_only_when_the_quote_crosses() {
        let buy = limit_buy(dec!(1), dec!(40));
        assert_eq!(price(&buy, &quote(dec!(39), MarketStatus::Open)), Some(cad(dec!(39))));
        assert_eq!(price(&buy, &quote(dec!(41), MarketStatus::Open)), None);
        let sell = limit_sell(dec!(1), dec!(40));
        assert_eq!(price(&sell, &quote(dec!(41), MarketStatus::Open)), Some(cad(dec!(41))));
        assert_eq!(price(&sell, &quote(dec!(39), MarketStatus::Open)), None);
    }

    #[test]
    fn stop_limit_sells_trigger_at_the_stop_and_respect_the_limit() {
        let order = Order::stop_limit_sell(account(), security(), quantity(dec!(1)), cad(dec!(38)), cad(dec!(37)))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(price(&order, &quote(dec!(39), MarketStatus::Open)), None, "not triggered");
        assert_eq!(price(&order, &quote(dec!(37.5), MarketStatus::Open)), Some(cad(dec!(37.5))));
        assert_eq!(price(&order, &quote(dec!(36), MarketStatus::Open)), None, "below the limit");
    }
}
