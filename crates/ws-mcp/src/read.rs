use clap::{Args, Subcommand};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use ws_broker::Reader;
use ws_core::{Account, Activity, IdempotencyKey, Placed, Position, Quote, Report, Security};

use crate::action::{Action, Failure, perform};
use crate::resolve;

const DEFAULT_ACTIVITIES: usize = 20;
const MAX_ACTIVITIES: usize = 100;

#[derive(Debug, Subcommand)]
pub enum Read {
    #[command(about = Accounts::SUMMARY)]
    Accounts(Accounts),
    #[command(about = Positions::SUMMARY)]
    Positions(Positions),
    #[command(about = Activities::SUMMARY)]
    Activities(Activities),
    #[command(about = Search::SUMMARY)]
    Search(Search),
    #[command(about = QuoteOf::SUMMARY)]
    Quote(QuoteOf),
    #[command(about = PendingOrders::SUMMARY)]
    PendingOrders(PendingOrders),
    #[command(about = OrderStatus::SUMMARY)]
    OrderStatus(OrderStatus),
}

impl Read {
    pub async fn perform(self, reader: &(dyn Reader + 'static)) -> Result<String, Failure> {
        match self {
            Self::Accounts(action) => perform(action, reader).await,
            Self::Positions(action) => perform(action, reader).await,
            Self::Activities(action) => perform(action, reader).await,
            Self::Search(action) => perform(action, reader).await,
            Self::Quote(action) => perform(action, reader).await,
            Self::PendingOrders(action) => perform(action, reader).await,
            Self::OrderStatus(action) => perform(action, reader).await,
        }
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct Accounts {}

impl Action for Accounts {
    const SUMMARY: &'static str = "List open Wealthsimple accounts";
    type Context = dyn Reader;
    type Output = Vec<Account>;

    async fn run(self, reader: &Self::Context) -> Result<Self::Output, Failure> {
        Ok(reader.accounts().await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct Positions {
    /// Account nickname, type such as `tfsa`, or id, as listed by `accounts`.
    pub account: String,
}

impl Action for Positions {
    const SUMMARY: &'static str = "List holdings in one account, with market value and book cost";
    type Context = dyn Reader;
    type Output = Vec<Position>;

    async fn run(self, reader: &Self::Context) -> Result<Self::Output, Failure> {
        Ok(reader.positions(&resolve::account(reader, &self.account).await?).await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct Activities {
    /// Account nickname, type such as `tfsa`, or id, as listed by `accounts`.
    pub account: String,
    /// How many of the most recent activities to return (default 20, at most 100).
    #[arg(long)]
    pub limit: Option<usize>,
}

impl Action for Activities {
    const SUMMARY: &'static str = "List the most recent activities in one account, newest first";
    type Context = dyn Reader;
    type Output = Vec<Activity>;

    async fn run(self, reader: &Self::Context) -> Result<Self::Output, Failure> {
        let limit = self.limit.unwrap_or(DEFAULT_ACTIVITIES).clamp(1, MAX_ACTIVITIES);
        Ok(reader.activities(&resolve::account(reader, &self.account).await?, limit).await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct Search {
    /// Ticker or company name, such as `XEQT` or `Shopify`.
    pub query: String,
}

impl Action for Search {
    const SUMMARY: &'static str = "Search for securities by ticker or name";
    type Context = dyn Reader;
    type Output = Vec<Security>;

    async fn run(self, reader: &Self::Context) -> Result<Self::Output, Failure> {
        Ok(reader.search(&self.query).await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct QuoteOf {
    /// Ticker such as `XEQT`, ticker and exchange such as `SHOP:TSX`, or security id.
    pub security: String,
}

impl Action for QuoteOf {
    const SUMMARY: &'static str = "Get the latest price and market status for a security";
    type Context = dyn Reader;
    type Output = Quote;

    async fn run(self, reader: &Self::Context) -> Result<Self::Output, Failure> {
        Ok(reader.quote(&resolve::security(reader, &self.security).await?.id).await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct PendingOrders {
    /// Account nickname, type such as `tfsa`, or id, as listed by `accounts`.
    pub account: String,
}

impl Action for PendingOrders {
    const SUMMARY: &'static str = "List orders in one account that have not filled, been cancelled, or expired";
    type Context = dyn Reader;
    type Output = Vec<Placed>;

    async fn run(self, reader: &Self::Context) -> Result<Self::Output, Failure> {
        Ok(reader.pending_orders(&resolve::account(reader, &self.account).await?).await?)
    }
}

#[derive(Debug, Args, Deserialize, JsonSchema)]
pub struct OrderStatus {
    /// The order's key (starts with `order-`), as returned by `place_order` or listed by `pending_orders`.
    #[schemars(with = "String")]
    pub key: IdempotencyKey,
}

impl Action for OrderStatus {
    const SUMMARY: &'static str = "Look up one order by its key: open or finished, how much filled, and at what price";
    type Context = dyn Reader;
    type Output = Report;

    async fn run(self, reader: &Self::Context) -> Result<Self::Output, Failure> {
        Ok(reader.order(&self.key).await?)
    }
}
