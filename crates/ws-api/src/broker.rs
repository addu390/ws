use async_trait::async_trait;
use ws_broker::{Broker, Reader};
use ws_core::{
    Account, AccountId, Activity, IdempotencyKey, Order, OrderId, Placed, Position, Quote, Report, Security, SecurityId,
};

use crate::{Api, Error};

#[async_trait]
impl Reader for Api {
    async fn accounts(&self) -> Result<Vec<Account>, ws_broker::Error> {
        Ok(Api::accounts(self).await?)
    }

    async fn positions(&self, account: &AccountId) -> Result<Vec<Position>, ws_broker::Error> {
        Ok(Api::positions(self, account).await?)
    }

    async fn activities(&self, account: &AccountId, limit: usize) -> Result<Vec<Activity>, ws_broker::Error> {
        Ok(Api::activities(self, account, limit).await?)
    }

    async fn search(&self, query: &str) -> Result<Vec<Security>, ws_broker::Error> {
        Ok(Api::search(self, query).await?)
    }

    async fn quote(&self, security: &SecurityId) -> Result<Quote, ws_broker::Error> {
        Ok(Api::quote(self, security).await?)
    }

    async fn pending_orders(&self, account: &AccountId) -> Result<Vec<Placed>, ws_broker::Error> {
        Ok(Api::orders(self, account).await?)
    }

    async fn order(&self, key: &IdempotencyKey) -> Result<Report, ws_broker::Error> {
        Ok(Api::order(self, key).await?)
    }
}

#[async_trait]
impl Broker for Api {
    async fn place(&self, order: &Order, key: &IdempotencyKey) -> Result<OrderId, ws_broker::Error> {
        Ok(Api::place(self, order, key).await?)
    }

    async fn cancel(&self, key: &IdempotencyKey) -> Result<(), ws_broker::Error> {
        Ok(Api::cancel(self, key).await?)
    }
}

impl From<Error> for ws_broker::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Unauthorized => Self::Unauthorized,
            Error::NotFound(what) => Self::NotFound(what),
            Error::Rejected(reason) | Error::Unsupported(reason) => Self::Rejected(reason),
            other => Self::backend(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_keep_their_meaning() {
        assert!(matches!(ws_broker::Error::from(Error::Unauthorized), ws_broker::Error::Unauthorized));
        assert!(matches!(ws_broker::Error::from(Error::NotFound("x".into())), ws_broker::Error::NotFound(_)));
        assert!(matches!(ws_broker::Error::from(Error::Rejected("x".into())), ws_broker::Error::Rejected(_)));
        assert!(matches!(ws_broker::Error::from(Error::Unsupported("x".into())), ws_broker::Error::Rejected(_)));
    }
}
