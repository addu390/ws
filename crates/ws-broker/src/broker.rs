use async_trait::async_trait;
use ws_core::{
    Account, AccountId, Activity, IdempotencyKey, Order, OrderId, Placed, Position, Quote, Report, Security, SecurityId,
};

use crate::Error;

#[async_trait]
pub trait Reader: Send + Sync {
    async fn accounts(&self) -> Result<Vec<Account>, Error>;

    async fn positions(&self, account: &AccountId) -> Result<Vec<Position>, Error>;

    async fn activities(&self, account: &AccountId, limit: usize) -> Result<Vec<Activity>, Error>;

    async fn search(&self, query: &str) -> Result<Vec<Security>, Error>;

    async fn quote(&self, security: &SecurityId) -> Result<Quote, Error>;

    async fn pending_orders(&self, account: &AccountId) -> Result<Vec<Placed>, Error>;

    async fn order(&self, key: &IdempotencyKey) -> Result<Report, Error>;
}

/// Only `ws-policy`'s `Guard` may hold one. Everything else reads through `Reader`.
#[async_trait]
pub trait Broker: Reader {
    /// Placing twice with the same `key` must not create a second order.
    async fn place(&self, order: &Order, key: &IdempotencyKey) -> Result<OrderId, Error>;

    async fn cancel(&self, key: &IdempotencyKey) -> Result<(), Error>;
}
