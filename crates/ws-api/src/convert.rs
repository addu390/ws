use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::{Value, json};
use ws_core::{
    Account, AccountId, Activity, Currency, IdempotencyKey, Kind, Management, MarketStatus, Money, Order, OrderId,
    Placed, Position, Quantity, Quote, Registration, Report, Security, SecurityId, Side, Size, Status, Tif, Valuation,
};

use crate::{Error, wire};

const REPORTED_SCALE: u32 = 4;

#[derive(Debug)]
pub(crate) struct Mismatch(pub String);

impl From<ws_core::Error> for Mismatch {
    fn from(error: ws_core::Error) -> Self {
        Self(error.to_string())
    }
}

fn missing(field: &str) -> Mismatch {
    Mismatch(format!("missing {field}"))
}

fn currency(code: Option<&str>) -> Result<Currency, Mismatch> {
    Ok(code.ok_or_else(|| missing("currency"))?.parse()?)
}

fn money(amount: Decimal, code: Option<&str>) -> Result<Money, Mismatch> {
    let rounded = amount.round_dp_with_strategy(REPORTED_SCALE, RoundingStrategy::MidpointAwayFromZero);
    Ok(Money::new(rounded, currency(code)?)?)
}

fn amount(wire: &wire::Amount) -> Result<Money, Mismatch> {
    money(wire.amount, Some(&wire.currency))
}

const MANAGEMENT: [(&str, Management); 4] = [
    ("SELF_DIRECTED_", Management::SelfDirected),
    ("MANAGED_PORTFOLIO_", Management::Managed),
    ("MANAGED_", Management::Managed),
    ("CUSTOM_PORTFOLIO_", Management::Automated),
];

pub(crate) fn account(wire: wire::Account) -> Result<Option<Account>, Mismatch> {
    if wire.status.as_deref() != Some("open") {
        return Ok(None);
    }
    let kind = wire.unified_account_type.ok_or_else(|| missing("account type"))?;
    let (management, base) = MANAGEMENT
        .iter()
        .find_map(|(prefix, management)| kind.strip_prefix(prefix).map(|base| (*management, base)))
        .unwrap_or((Management::Neither, kind.as_str()));
    let registration = match base {
        "TFSA" => Registration::Tfsa,
        "RRSP" => Registration::Rrsp,
        "SPOUSAL_RRSP" => Registration::SpousalRrsp,
        "FHSA" => Registration::Fhsa,
        "RESP" | "FAMILY_RESP" | "INDIVIDUAL_RESP" => Registration::Resp,
        "RRIF" | "SPOUSAL_RRIF" => Registration::Rrif,
        "LIRA" => Registration::Lira,
        "NON_REGISTERED" | "JOINT_NON_REGISTERED" | "JOINT" => Registration::NonRegistered,
        "NON_REGISTERED_MARGIN" => Registration::Margin,
        "CASH" => Registration::Cash,
        "CRYPTO" => Registration::Crypto,
        "CREDIT_CARD" => Registration::CreditCard,
        "PORTFOLIO_LINE_OF_CREDIT" => Registration::LineOfCredit,
        _ => Registration::Other(kind.clone()),
    };
    let mut account = Account::new(AccountId::parse(wire.id)?, registration, management, currency(wire.currency.as_deref())?);
    if let Some(nickname) = wire.nickname.filter(|n| !n.is_empty()) {
        account = account.named(nickname);
    }
    match wire.financials.and_then(|f| f.current_combined).map(valuation).transpose() {
        Ok(Some(valuation)) => account = account.valued(valuation),
        Ok(None) => {}
        Err(Mismatch(detail)) => tracing::warn!(account = %account.id(), detail, "skipping valuation"),
    }
    Ok(Some(account))
}

fn valuation(wire: wire::CurrentFinancials) -> Result<Valuation, Mismatch> {
    let mut valuation = Valuation::new(amount(wire.net_liquidation_value.as_ref().ok_or_else(|| missing("value"))?)?);
    if let Some(deposits) = &wire.net_deposits {
        valuation = valuation.with_net_deposits(amount(deposits)?);
    }
    if let Some(returns) = wire.simple_returns
        && let Some(gain) = &returns.amount
    {
        valuation = valuation.with_return(amount(gain)?, returns.rate);
    }
    Ok(valuation)
}

pub(crate) fn security(wire: wire::Security) -> Result<Security, Mismatch> {
    let stock = wire.stock.ok_or_else(|| missing("stock details"))?;
    let symbol = stock.symbol.ok_or_else(|| missing("symbol"))?;
    let name = stock.name.unwrap_or_else(|| symbol.clone());
    let mut security = Security::new(SecurityId::parse(wire.id)?, symbol, name);
    if let Some(exchange) = stock.primary_exchange {
        security = security.on(exchange);
    }
    if wire.buyable == Some(false) {
        security = security.unbuyable();
    }
    Ok(security)
}

pub(crate) fn position(account: &AccountId, wire: wire::Position) -> Result<Option<Position>, Mismatch> {
    if wire.quantity <= Decimal::ZERO {
        return Ok(None);
    }
    let value = amount(wire.total_value.as_ref().ok_or_else(|| missing("total value"))?)?;
    let position = Position::new(account.clone(), security(wire.security)?, Quantity::new(wire.quantity)?, value);
    Ok(Some(match &wire.book_value {
        Some(cost) => position.with_cost(amount(cost)?),
        None => position,
    }))
}

pub(crate) fn quote(wire: wire::Security) -> Result<Quote, Mismatch> {
    let id = SecurityId::parse(wire.id)?;
    let quote = wire.quote_v2.ok_or_else(|| missing("quote"))?;
    let price = money(quote.price.ok_or_else(|| missing("price"))?, quote.currency.as_deref())?;
    let at = quote.quoted_as_of.ok_or_else(|| missing("quote time"))?;
    Ok(Quote::new(id, price, at, market(quote.market_status.as_deref())))
}

fn market(status: Option<&str>) -> MarketStatus {
    match status.map(str::to_ascii_uppercase).as_deref() {
        Some("OPEN") => MarketStatus::Open,
        Some("CLOSED") => MarketStatus::Closed,
        Some("PRE_MARKET" | "PREMARKET") => MarketStatus::PreMarket,
        Some("AFTER_HOURS" | "POST_MARKET" | "AFTERHOURS") => MarketStatus::AfterHours,
        _ => MarketStatus::Unknown,
    }
}

pub(crate) fn activity(wire: wire::Activity) -> Result<Activity, Mismatch> {
    let kind = wire.kind.unwrap_or_else(|| "UNKNOWN".to_owned());
    let mut activity = Activity::new(wire.canonical_id, AccountId::parse(wire.account_id)?, kind, wire.occurred_at);
    if let Some(subkind) = wire.sub_type {
        activity = activity.with_subkind(subkind);
    }
    if let Some(status) = wire.status {
        activity = activity.with_status(status);
    }
    if let (Some(value), Some(code)) = (wire.amount, wire.currency.as_deref()) {
        let signed = if wire.amount_sign.as_deref() == Some("negative") { -value.abs() } else { value };
        match money(signed, Some(code)) {
            Ok(amount) => activity = activity.with_amount(amount),
            Err(Mismatch(detail)) => tracing::warn!(activity = %activity.id(), detail, "skipping activity amount"),
        }
    }
    if let Some(symbol) = wire.asset_symbol {
        activity = activity.with_asset(symbol, wire.asset_quantity);
    }
    Ok(activity)
}

const STATUSES: [(&str, Status); 13] = [
    ("CANCEL_PENDING", Status::CancelPending),
    ("CONTINGENT", Status::Contingent),
    ("NEW", Status::New),
    ("PARTIALLY_FILLED", Status::PartiallyFilled),
    ("PENDING_FUND_TRANSFER", Status::PendingFundTransfer),
    ("PENDING_REVIEW", Status::PendingReview),
    ("PENDING_SUBMISSION", Status::PendingSubmission),
    ("PLACED", Status::Placed),
    ("SUBMITTED", Status::Submitted),
    ("FILLED", Status::Filled),
    ("CANCELLED", Status::Cancelled),
    ("REJECTED", Status::Rejected),
    ("EXPIRED", Status::Expired),
];

pub(crate) fn open_statuses() -> impl Iterator<Item = &'static str> {
    STATUSES.iter().filter(|(_, status)| status.is_open()).map(|(name, _)| *name)
}

fn status(name: &str) -> Result<Status, Mismatch> {
    STATUSES
        .iter()
        .find_map(|(known, status)| known.eq_ignore_ascii_case(name).then_some(*status))
        .ok_or_else(|| Mismatch(format!("unknown order status {name:?}")))
}

fn side(name: &str) -> Result<Side, Mismatch> {
    match name {
        "BUY" => Ok(Side::Buy),
        "SELL" => Ok(Side::Sell),
        other => Err(Mismatch(format!("unknown order side {other:?}"))),
    }
}

pub(crate) fn placed(wire: wire::FeedOrder) -> Result<Placed, Mismatch> {
    let status = status(&wire.status)?;
    if !status.is_open() {
        return Err(Mismatch(format!("finished order in the open-order feed: {:?}", wire.status)));
    }
    let mut placed = Placed::new(
        OrderId::parse(wire.order_id)?,
        AccountId::parse(wire.canonical_account_id)?,
        SecurityId::parse(wire.security_id)?,
        side(&wire.side)?,
        status,
        wire.created_at_utc,
    );
    if let Ok(key) = IdempotencyKey::parse(wire.id) {
        placed = placed.with_key(key);
    }
    if let Some(symbol) = wire.symbol {
        placed = placed.with_symbol(symbol);
    }
    if let Some(quantity) = wire.submitted_quantity.filter(|q| *q > Decimal::ZERO) {
        placed = placed.with_quantity(Quantity::new(quantity)?);
    }
    let currency = wire.security_currency.as_deref();
    if let Some(limit) = wire.limit_price {
        placed = placed.with_limit(money(limit, currency)?);
    }
    if let Some(stop) = wire.stop_price {
        placed = placed.with_stop(money(stop, currency)?);
    }
    Ok(placed)
}

pub(crate) fn report(key: IdempotencyKey, wire: wire::ExtendedOrder) -> Result<Report, Mismatch> {
    let side = side(wire.order_type.split('_').next().unwrap_or_default())?;
    let mut report = Report::new(
        key,
        AccountId::parse(wire.canonical_account_id)?,
        SecurityId::parse(wire.security_id)?,
        side,
        status(&wire.status)?,
    );
    let currency = wire.security_currency.as_deref();
    if let Some(quantity) = wire.submitted_quantity.filter(|q| *q > Decimal::ZERO) {
        report = report.with_quantity(Quantity::new(quantity)?);
    }
    if let (Some(filled), Some(average)) = (wire.filled_quantity.filter(|q| *q > Decimal::ZERO), wire.average_filled_price) {
        report = report.with_fill(Quantity::new(filled)?, money(average, currency)?);
    }
    if let Some(limit) = wire.limit_price {
        report = report.with_limit(money(limit, currency)?);
    }
    if let Some(stop) = wire.stop_price {
        report = report.with_stop(money(stop, currency)?);
    }
    if let Some(at) = wire.submitted_at_utc {
        report = report.with_submitted(at);
    }
    if let Some(at) = wire.expired_at_utc {
        report = report.with_expiry(at);
    }
    let rejection = [wire.rejection_code, wire.rejection_cause].into_iter().flatten().collect::<Vec<_>>().join(": ");
    if !rejection.is_empty() {
        report = report.with_rejection(rejection);
    }
    Ok(report)
}

/// Only shapes captured from the web app's own requests are built. Anything else is refused, not guessed.
pub(crate) fn create_input(order: &Order, key: &IdempotencyKey) -> Result<Value, Error> {
    match (order.side(), order.size(), order.kind()) {
        (Side::Buy, Size::Value(value), Kind::Market) => Ok(json!({
            "canonicalAccountId": order.account().as_str(),
            "externalId": key.as_str(),
            "executionType": "FRACTIONAL",
            "orderType": "BUY_VALUE",
            "value": number(value)?,
            "securityId": order.security().as_str(),
            "timeInForce": null,
            "valueCurrency": value.currency().code(),
        })),
        (Side::Buy, Size::Shares(quantity), Kind::Market) if order.tif() == Tif::Day => Ok(json!({
            "canonicalAccountId": order.account().as_str(),
            "externalId": key.as_str(),
            "executionType": "MARKET",
            "orderType": "BUY_QUANTITY",
            "quantity": whole(quantity)?,
            "securityId": order.security().as_str(),
            "timeInForce": "DAY",
        })),
        (Side::Buy, Size::Shares(quantity), Kind::Limit { limit }) if order.tif() == Tif::Day => Ok(json!({
            "canonicalAccountId": order.account().as_str(),
            "externalId": key.as_str(),
            "executionType": "LIMIT",
            "orderType": "BUY_QUANTITY",
            "quantity": whole(quantity)?,
            "limitPrice": number(limit)?,
            "securityId": order.security().as_str(),
            "timeInForce": "DAY",
            "tradingSession": "REGULAR",
        })),
        _ => Err(Error::Unsupported(
            "only dollar-amount market buys and whole-share day market or limit buys can be placed live until other order shapes are captured"
                .to_owned(),
        )),
    }
}

/// Wealthsimple takes order amounts as JSON numbers, not strings.
fn number(money: Money) -> Result<f64, Error> {
    money.amount().to_f64().ok_or_else(|| Error::Unsupported(format!("amount {money}")))
}

fn whole(quantity: Quantity) -> Result<u64, Error> {
    let value = quantity.value();
    value
        .fract()
        .is_zero()
        .then(|| value.to_u64())
        .flatten()
        .ok_or_else(|| Error::Unsupported(format!("fractional share count {value}")))
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;
    use serde_json::json;

    use super::*;

    fn from<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn classifies_account_types() {
        let open = |kind: &str| {
            account(from(json!({"id": "tfsa-1", "status": "open", "currency": "CAD", "unifiedAccountType": kind})))
                .unwrap_or_else(|e| panic!("{e:?}"))
                .unwrap_or_else(|| panic!("dropped"))
        };
        let tfsa = open("SELF_DIRECTED_TFSA");
        assert_eq!((tfsa.registration(), tfsa.management()), (&Registration::Tfsa, Management::SelfDirected));
        assert!(tfsa.tradable());
        let rrsp = open("MANAGED_RRSP");
        assert_eq!((rrsp.registration(), rrsp.management()), (&Registration::Rrsp, Management::Managed));
        assert!(!rrsp.tradable());
        let portfolio = open("MANAGED_PORTFOLIO_RRSP");
        assert_eq!((portfolio.registration(), portfolio.management()), (&Registration::Rrsp, Management::Managed));
        assert_eq!(open("MANAGED_PORTFOLIO_NON_REGISTERED").registration(), &Registration::NonRegistered);
        assert_eq!(open("CASH").management(), Management::Neither);
        let direct = open("CUSTOM_PORTFOLIO_NON_REGISTERED");
        assert_eq!((direct.registration(), direct.management()), (&Registration::NonRegistered, Management::Automated));
        assert!(!direct.tradable());
        let credit = open("PORTFOLIO_LINE_OF_CREDIT");
        assert_eq!((credit.registration(), credit.management()), (&Registration::LineOfCredit, Management::Neither));
        assert_eq!(open("SOMETHING_NEW").registration(), &Registration::Other("SOMETHING_NEW".to_owned()));
    }

    #[test]
    fn values_managed_accounts_from_their_financials() {
        let wire = from(json!({
            "id": "rrsp-1", "status": "open", "currency": "CAD", "unifiedAccountType": "MANAGED_PORTFOLIO_RRSP",
            "financials": {"currentCombined": {
                "netLiquidationValue": {"amount": "1416.3344918112", "currency": "CAD"},
                "netDeposits": {"amount": "1250", "currency": "CAD"},
                "simpleReturns": {"amount": {"amount": "166.3344918112", "currency": "CAD"}, "rate": "0.133068"},
            }},
        }));
        let account = account(wire).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("dropped"));
        let valuation = account.valuation().unwrap_or_else(|| panic!("no valuation"));
        assert_eq!(valuation.value().amount(), dec!(1416.3345));
        assert_eq!(valuation.net_deposits().map(|m| m.amount()), Some(dec!(1250)));
        assert_eq!(valuation.gain().map(|m| m.amount()), Some(dec!(166.3345)));
        assert_eq!(valuation.rate(), Some(dec!(0.133068)));
    }

    #[test]
    fn keeps_accounts_whose_valuation_is_malformed() {
        let wire = from(json!({
            "id": "a", "status": "open", "currency": "CAD", "unifiedAccountType": "CASH",
            "financials": {"currentCombined": {"netLiquidationValue": {"amount": "1", "currency": "XYZ"}}},
        }));
        let account = account(wire).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("dropped"));
        assert!(account.valuation().is_none());
    }

    #[test]
    fn drops_closed_accounts() {
        let closed = account(from(json!({"id": "a", "status": "closed", "currency": "CAD", "unifiedAccountType": "CASH"})));
        assert!(matches!(closed, Ok(None)));
    }

    #[test]
    fn reads_positions_with_string_or_number_amounts() {
        let wire = from(json!({
            "quantity": "2.5",
            "totalValue": {"amount": "250.123456", "currency": "CAD"},
            "bookValue": {"amount": 200, "currency": "CAD"},
            "security": {"id": "sec-s-abc", "stock": {"symbol": "XEQT", "name": "iShares", "primaryExchange": "TSX"}},
        }));
        let account = AccountId::parse("tfsa-1").unwrap_or_else(|e| panic!("{e}"));
        let position = position(&account, wire).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("dropped"));
        assert_eq!(position.quantity().value(), dec!(2.5));
        assert_eq!(position.value().amount(), dec!(250.1235));
        assert_eq!(position.cost().map(|c| c.amount()), Some(dec!(200)));
        assert_eq!(position.security().exchange(), Some("TSX"));
    }

    #[test]
    fn drops_flat_positions() {
        let wire = from(json!({"quantity": "0", "security": {"id": "sec-s-abc"}}));
        let account = AccountId::parse("tfsa-1").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(position(&account, wire), Ok(None)));
    }

    #[test]
    fn reads_quotes() {
        let wire = from(json!({
            "id": "sec-s-abc",
            "quoteV2": {"price": "31.42", "currency": "CAD", "quotedAsOf": "2026-09-30T14:00:00Z", "marketStatus": "OPEN"},
        }));
        let quote = quote(wire).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(quote.price().amount(), dec!(31.42));
        assert_eq!(quote.market(), MarketStatus::Open);
    }

    #[test]
    fn signs_activity_amounts() {
        let wire = from(json!({
            "canonicalId": "act-1",
            "accountId": "tfsa-1",
            "type": "DIY_BUY",
            "subType": "MARKET_ORDER",
            "status": "FILLED",
            "occurredAt": "2026-09-29T15:30:00.000Z",
            "amount": "100.50",
            "amountSign": "negative",
            "currency": "CAD",
            "assetSymbol": "XEQT",
            "assetQuantity": "3",
        }));
        let activity = activity(wire).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(activity.amount().map(|m| m.amount()), Some(dec!(-100.50)));
        assert_eq!((activity.symbol(), activity.quantity()), (Some("XEQT"), Some(dec!(3))));
        assert_eq!(activity.status(), Some("FILLED"));
    }

    #[test]
    fn keeps_activities_whose_amount_has_no_usable_currency() {
        for currency in [None, Some("XYZ")] {
            let wire = from(json!({
                "canonicalId": "act-1", "accountId": "tfsa-1", "type": "DIY_BUY", "status": "PENDING",
                "occurredAt": "2026-09-29T15:30:00.000Z", "amount": "100.50", "currency": currency,
            }));
            let activity = activity(wire).unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!((activity.kind(), activity.amount()), ("DIY_BUY", None));
        }
    }

    #[test]
    fn reads_open_orders_from_the_feed() {
        let row = |status: &str| {
            from::<wire::FeedOrder>(json!({
                "id": "order-2fb5c80f-0000-4000-8000-000000000000", "orderId": "order-abc",
                "canonicalAccountId": "rrsp-1", "createdAtUtc": "2026-10-03T13:50:17.241Z", "status": status,
                "orderClass": "EQUITY", "side": "BUY", "executionType": "POOLED_MARKET",
                "submittedQuantity": "0.2058", "limitPrice": null, "stopPrice": null,
                "securityCurrency": "CAD", "securityId": "sec-s-abc", "symbol": "CCO",
            }))
        };
        let order = placed(row("PLACED")).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!((order.id().as_str(), order.status(), order.side()), ("order-abc", Status::Placed, Side::Buy));
        assert_eq!(order.key().map(IdempotencyKey::as_str), Some("order-2fb5c80f-0000-4000-8000-000000000000"));
        assert_eq!((order.symbol(), order.quantity().map(|q| q.value()), order.limit()), (Some("CCO"), Some(dec!(0.2058)), None));
        assert!(placed(row("FILLED")).is_err());
    }

    #[test]
    fn reports_one_order_open_or_finished() {
        let lookup = |status: &str, filled: Option<&str>, average: Option<&str>| {
            from::<wire::ExtendedOrder>(json!({
                "averageFilledPrice": average, "filledQuantity": filled, "limitPrice": "0.4050",
                "orderType": "BUY_QUANTITY", "rejectionCause": null, "rejectionCode": null, "securityCurrency": "CAD",
                "status": status, "stopPrice": null, "submittedAtUtc": "2026-10-03T14:22:13.358Z",
                "submittedQuantity": "1.0000", "timeInForce": "DAY", "accountId": "ABC123", "canonicalAccountId": "tfsa-1",
                "tradingSession": "REGULAR", "expiredAtUtc": "2026-10-05T20:00:00.000Z", "securityId": "sec-s-abc",
            }))
        };
        let key = IdempotencyKey::fresh();
        let open = report(key.clone(), lookup("pending_submission", None, None)).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!((open.key(), open.status(), open.side()), (&key, Status::PendingSubmission, Side::Buy));
        assert_eq!((open.quantity().map(|q| q.value()), open.limit().map(|m| m.amount())), (Some(dec!(1)), Some(dec!(0.405))));
        assert!(open.expires().is_some() && open.filled().is_none());

        let filled = report(key.clone(), lookup("filled", Some("1.0000"), Some("0.4000"))).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(filled.status(), Status::Filled);
        assert_eq!((filled.filled().map(|q| q.value()), filled.average().map(|m| m.amount())), (Some(dec!(1)), Some(dec!(0.4))));
        assert!(report(key, lookup("teleported", None, None)).is_err());
    }

    #[test]
    fn builds_the_captured_buys_and_refuses_other_shapes() {
        let key = IdempotencyKey::fresh();
        let account = AccountId::parse("rrsp-1").unwrap_or_else(|e| panic!("{e}"));
        let security = SecurityId::parse("sec-s-abc").unwrap_or_else(|e| panic!("{e}"));
        let cad = |amount| Money::new(amount, Currency::Cad).unwrap_or_else(|e| panic!("{e}"));
        let buy = Order::value_buy(account.clone(), security.clone(), cad(dec!(25))).unwrap_or_else(|e| panic!("{e}"));
        let input = create_input(&buy, &key).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            input,
            json!({
                "canonicalAccountId": "rrsp-1", "externalId": key.as_str(), "executionType": "FRACTIONAL",
                "orderType": "BUY_VALUE", "value": 25.0, "securityId": "sec-s-abc", "timeInForce": null,
                "valueCurrency": "CAD",
            })
        );
        let shares = Quantity::new(dec!(1)).unwrap_or_else(|e| panic!("{e}"));
        let limit_buy = |quantity| {
            Order::limit_buy(account.clone(), security.clone(), quantity, cad(dec!(0.20))).unwrap_or_else(|e| panic!("{e}"))
        };
        assert_eq!(
            create_input(&limit_buy(shares), &key).unwrap_or_else(|e| panic!("{e}")),
            json!({
                "canonicalAccountId": "rrsp-1", "externalId": key.as_str(), "executionType": "LIMIT",
                "orderType": "BUY_QUANTITY", "quantity": 1, "limitPrice": 0.2, "securityId": "sec-s-abc",
                "timeInForce": "DAY", "tradingSession": "REGULAR",
            })
        );

        let market = Order::market_buy(account.clone(), security.clone(), shares);
        assert_eq!(
            create_input(&market, &key).unwrap_or_else(|e| panic!("{e}")),
            json!({
                "canonicalAccountId": "rrsp-1", "externalId": key.as_str(), "executionType": "MARKET",
                "orderType": "BUY_QUANTITY", "quantity": 1, "securityId": "sec-s-abc", "timeInForce": "DAY",
            })
        );
        let half = Quantity::new(dec!(0.5)).unwrap_or_else(|e| panic!("{e}"));
        let refused = [
            Order::market_buy(account.clone(), security.clone(), half),
            Order::market_buy(account.clone(), security.clone(), shares).good_till_cancelled(),
            limit_buy(half),
            limit_buy(shares).good_till_cancelled(),
            Order::limit_sell(account.clone(), security.clone(), shares, cad(dec!(9))).unwrap_or_else(|e| panic!("{e}")),
            Order::market_sell(account, security, shares),
        ];
        for order in refused {
            assert!(matches!(create_input(&order, &key), Err(Error::Unsupported(_))), "{order:?}");
        }
    }
}
