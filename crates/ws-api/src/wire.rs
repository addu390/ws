use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Envelope {
    pub data: Option<serde_json::Value>,
    pub errors: Option<Vec<Problem>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Problem {
    pub message: String,
    pub extensions: Option<Extensions>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Extensions {
    pub code: Option<String>,
}

impl Problem {
    pub fn unauthenticated(&self) -> bool {
        self.extensions.as_ref().and_then(|e| e.code.as_deref()) == Some("UNAUTHENTICATED")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Connection<T> {
    pub edges: Vec<Edge<T>>,
    pub page_info: Option<PageInfo>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Edge<T> {
    pub node: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

impl<T> Connection<T> {
    pub fn next(&self) -> Option<String> {
        self.page_info.as_ref().filter(|p| p.has_next_page).and_then(|p| p.end_cursor.clone())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Amount {
    pub amount: Decimal,
    pub currency: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AccountsData {
    pub identity: Option<AccountsIdentity>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AccountsIdentity {
    pub accounts: Connection<Account>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Account {
    pub id: String,
    pub status: Option<String>,
    pub currency: Option<String>,
    pub nickname: Option<String>,
    pub unified_account_type: Option<String>,
    pub financials: Option<AccountFinancials>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountFinancials {
    pub current_combined: Option<CurrentFinancials>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentFinancials {
    pub net_liquidation_value: Option<Amount>,
    pub net_deposits: Option<Amount>,
    pub simple_returns: Option<SimpleReturns>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SimpleReturns {
    pub amount: Option<Amount>,
    pub rate: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PositionsData {
    pub identity: Option<PositionsIdentity>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PositionsIdentity {
    pub financials: PositionsFinancials,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PositionsFinancials {
    pub current: PositionsCurrent,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PositionsCurrent {
    pub positions: Connection<Position>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Position {
    pub quantity: Decimal,
    pub total_value: Option<Amount>,
    pub book_value: Option<Amount>,
    pub security: Security,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Security {
    pub id: String,
    pub buyable: Option<bool>,
    pub stock: Option<Stock>,
    pub quote_v2: Option<Quote>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Stock {
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub primary_exchange: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Quote {
    pub price: Option<Decimal>,
    pub currency: Option<String>,
    pub quoted_as_of: Option<DateTime<Utc>>,
    pub market_status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchData {
    pub security_search: SearchResults,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchResults {
    pub results: Vec<Security>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct QuoteData {
    pub security: Option<Security>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivitiesData {
    pub activity_feed_items: Connection<Activity>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Activity {
    pub canonical_id: String,
    pub account_id: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub sub_type: Option<String>,
    pub status: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub amount: Option<Decimal>,
    pub amount_sign: Option<String>,
    pub currency: Option<String>,
    pub asset_symbol: Option<String>,
    pub asset_quantity: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrdersData {
    pub identity: Option<OrdersIdentity>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrdersIdentity {
    pub order_service_extended_order_feed: Connection<FeedOrder>,
}

/// `id` is the `externalId` the order was created with. `orderId` is Wealthsimple's own id.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedOrder {
    pub id: String,
    pub order_id: String,
    pub canonical_account_id: String,
    pub created_at_utc: DateTime<Utc>,
    pub status: String,
    pub side: String,
    pub submitted_quantity: Option<Decimal>,
    pub limit_price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub security_currency: Option<String>,
    pub security_id: String,
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrderData {
    pub so_orders_extended_order: Option<ExtendedOrder>,
}

/// `status` is lowercase here, unlike the feed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtendedOrder {
    pub canonical_account_id: String,
    pub security_id: String,
    pub status: String,
    pub order_type: String,
    pub security_currency: Option<String>,
    pub submitted_quantity: Option<Decimal>,
    pub filled_quantity: Option<Decimal>,
    pub average_filled_price: Option<Decimal>,
    pub limit_price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub submitted_at_utc: Option<DateTime<Utc>>,
    pub expired_at_utc: Option<DateTime<Utc>>,
    pub rejection_code: Option<String>,
    pub rejection_cause: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateData {
    pub so_orders_create_order: CreateResult,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateResult {
    pub errors: Option<Vec<OrderProblem>>,
    pub order: Option<Created>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OrderProblem {
    pub code: Option<String>,
    pub message: Option<String>,
}

impl OrderProblem {
    pub fn describe(problems: Option<Vec<Self>>) -> Option<String> {
        let described: Vec<String> = problems
            .unwrap_or_default()
            .into_iter()
            .map(|p| [p.code, p.message].into_iter().flatten().collect::<Vec<_>>().join(": "))
            .collect();
        (!described.is_empty()).then(|| described.join(". "))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Created {
    pub order_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CancelData {
    pub order_service_cancel_order: CancelResult,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CancelResult {
    pub errors: Option<Vec<OrderProblem>>,
}
