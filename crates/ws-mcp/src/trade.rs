use clap::{Args, Subcommand, ValueEnum};
use rmcp::schemars::{self, JsonSchema};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ws_core::{AccountId, Currency, IdempotencyKey, Money, Order, Quantity, SecurityId};
use ws_policy::{Guard, Receipt, Status, Ticket, TicketId};

use crate::action::{Action, Failure, perform};
use crate::resolve;

#[derive(Debug, Subcommand)]
pub enum Trade {
    #[command(name = "preview-order", about = PreviewOrder::SUMMARY)]
    Preview(PreviewOrder),
    #[command(name = "place-order", about = PlaceOrder::SUMMARY)]
    Place(PlaceOrder),
    #[command(name = "cancel-order", about = CancelOrder::SUMMARY)]
    Cancel(CancelOrder),
}

impl Trade {
    pub async fn perform(self, guard: &Guard) -> Result<String, Failure> {
        match self {
            Self::Preview(action) => perform(action, guard).await,
            Self::Place(action) => perform(action, guard).await,
            Self::Cancel(action) => perform(action, guard).await,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct PreviewOrder {
    /// Account nickname, type such as `tfsa`, or id, as listed by `accounts`.
    pub account: String,
    /// Ticker such as `XEQT`, ticker and exchange such as `SHOP:TSX`, or security id.
    pub security: String,
    /// `buy` or `sell`.
    #[arg(value_enum)]
    pub side: Side,
    /// Number of shares, as a decimal string. Give this or `amount`, not both.
    #[arg(long)]
    #[schemars(with = "Option<String>")]
    pub shares: Option<Decimal>,
    /// Dollar amount in CAD, as a decimal string, for a market buy of fractional shares.
    #[arg(long)]
    #[schemars(with = "Option<String>")]
    pub amount: Option<Decimal>,
    /// Limit price in CAD, as a decimal string. Without one the order is a market order.
    #[arg(long)]
    #[schemars(with = "Option<String>")]
    pub limit: Option<Decimal>,
    /// Stop price in CAD, for a stop-limit sell. Needs `limit` too.
    #[arg(long)]
    #[schemars(with = "Option<String>")]
    pub stop: Option<Decimal>,
    /// Keep the order open until cancelled, instead of for today only.
    #[arg(long)]
    #[serde(default)]
    pub good_till_cancelled: bool,
}

impl PreviewOrder {
    fn order(self, account: AccountId, security: SecurityId) -> Result<Order, Failure> {
        let cad = |amount| Money::new(amount, Currency::Cad);
        let order = match (self.shares, self.amount) {
            (Some(_), Some(_)) | (None, None) => return Err(Failure::Invalid("give exactly one of shares or amount")),
            (None, Some(amount)) => match (self.side, self.limit, self.stop) {
                (Side::Buy, None, None) => Order::value_buy(account, security, cad(amount)?)?,
                _ => return Err(Failure::Invalid("amount is only for market buys. Use shares for sells and limit orders")),
            },
            (Some(shares), None) => {
                let shares = Quantity::new(shares)?;
                match (self.side, self.limit, self.stop) {
                    (Side::Buy, None, None) => Order::market_buy(account, security, shares),
                    (Side::Sell, None, None) => Order::market_sell(account, security, shares),
                    (Side::Buy, Some(limit), None) => Order::limit_buy(account, security, shares, cad(limit)?)?,
                    (Side::Sell, Some(limit), None) => Order::limit_sell(account, security, shares, cad(limit)?)?,
                    (Side::Sell, Some(limit), Some(stop)) => {
                        Order::stop_limit_sell(account, security, shares, cad(stop)?, cad(limit)?)?
                    }
                    (Side::Buy, _, Some(_)) => return Err(Failure::Invalid("stop prices are only for sells")),
                    (Side::Sell, None, Some(_)) => return Err(Failure::Invalid("a stop price needs a limit price too")),
                }
            }
        };
        Ok(if self.good_till_cancelled { order.good_till_cancelled() } else { order })
    }
}

impl Action for PreviewOrder {
    const SUMMARY: &'static str = "Check an order against the limits and get a single-use ticket to place it. Nothing is sent. \
        Show the user the order and its estimated value before placing it";
    type Context = Guard;
    type Output = Ticket;

    async fn run(self, guard: &Self::Context) -> Result<Self::Output, Failure> {
        let account = resolve::account(guard.reader(), &self.account).await?;
        let security = resolve::security(guard.reader(), &self.security).await?;
        Ok(guard.preview(self.order(account, security.id)?, security.listing).await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct PlaceOrder {
    /// Ticket id (starts with `ticket-`), as returned by `preview_order`.
    #[schemars(with = "String")]
    pub ticket_id: TicketId,
}

impl Action for PlaceOrder {
    const SUMMARY: &'static str = "Place a previewed order. Each ticket works once and expires. \
        Only use this after the user confirmed the previewed order";
    type Context = Guard;
    type Output = Receipt;

    async fn run(self, guard: &Self::Context) -> Result<Self::Output, Failure> {
        Ok(guard.place(&self.ticket_id).await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct CancelOrder {
    /// The order's key (starts with `order-`), as returned by `place_order` or listed by `pending_orders`.
    #[schemars(with = "String")]
    pub key: IdempotencyKey,
}

#[derive(Debug, Serialize)]
pub struct Cancelled {
    cancelled: IdempotencyKey,
}

impl Action for CancelOrder {
    const SUMMARY: &'static str = "Cancel an open order by its key";
    type Context = Guard;
    type Output = Cancelled;

    async fn run(self, guard: &Self::Context) -> Result<Self::Output, Failure> {
        guard.cancel(&self.key).await?;
        Ok(Cancelled { cancelled: self.key })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LimitsStatus {}

impl Action for LimitsStatus {
    const SUMMARY: &'static str = "Show the mode, the kill switch, the configured limits, and how much of today's budget is used";
    type Context = Guard;
    type Output = Status;

    fn run(self, guard: &Self::Context) -> impl Future<Output = Result<Self::Output, Failure>> + Send {
        std::future::ready(guard.status().map_err(Failure::from))
    }
}
