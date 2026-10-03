use std::marker::PhantomData;

use crate::wire;

pub(crate) struct Operation<T> {
    name: &'static str,
    query: &'static str,
    data: PhantomData<fn() -> T>,
}

impl<T> Operation<T> {
    const fn new(name: &'static str, query: &'static str) -> Self {
        Self { name, query, data: PhantomData }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn query(&self) -> &'static str {
        self.query
    }
}

pub(crate) const ACCOUNTS: Operation<wire::AccountsData> =
    Operation::new("FetchAllAccountFinancials", include_str!("../queries/accounts.graphql"));

pub(crate) const POSITIONS: Operation<wire::PositionsData> =
    Operation::new("FetchIdentityPositions", include_str!("../queries/positions.graphql"));

pub(crate) const ACTIVITIES: Operation<wire::ActivitiesData> =
    Operation::new("FetchActivityFeedItems", include_str!("../queries/activities.graphql"));

pub(crate) const SEARCH: Operation<wire::SearchData> =
    Operation::new("FetchSecuritySearchResult", include_str!("../queries/search.graphql"));

pub(crate) const QUOTE: Operation<wire::QuoteData> =
    Operation::new("FetchSecurityQuoteV2", include_str!("../queries/quote.graphql"));

pub(crate) const ORDERS: Operation<wire::OrdersData> =
    Operation::new("OrderServiceExtendedOrderFeed", include_str!("../queries/orders.graphql"));

pub(crate) const ORDER: Operation<wire::OrderData> =
    Operation::new("FetchSoOrdersExtendedOrder", include_str!("../queries/order.graphql"));

pub(crate) const CREATE: Operation<wire::CreateData> =
    Operation::new("SoOrdersOrderCreate", include_str!("../queries/create.graphql"));

pub(crate) const CANCEL: Operation<wire::CancelData> =
    Operation::new("SoOrdersOrderCancel", include_str!("../queries/cancel.graphql"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_query_declares_its_operation_name() {
        let queries = [
            (ACCOUNTS.name(), ACCOUNTS.query()),
            (POSITIONS.name(), POSITIONS.query()),
            (ACTIVITIES.name(), ACTIVITIES.query()),
            (SEARCH.name(), SEARCH.query()),
            (QUOTE.name(), QUOTE.query()),
            (ORDERS.name(), ORDERS.query()),
            (ORDER.name(), ORDER.query()),
        ];
        for (name, query) in queries {
            assert!(query.starts_with(&format!("query {name}(")), "{name}");
        }
        for (name, query) in [(CREATE.name(), CREATE.query()), (CANCEL.name(), CANCEL.query())] {
            assert!(query.starts_with(&format!("mutation {name}(")), "{name}");
        }
    }
}
