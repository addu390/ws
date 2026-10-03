use std::sync::Arc;

use chrono::{Duration, SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use ws_auth::{Endpoints, Session, Store};
use ws_core::{
    Account, AccountId, Activity, IdempotencyKey, IdentityId, Order, OrderId, Placed, Position, Quote, Report,
    Security, SecurityId,
};
use ws_net::{Client, Headers};

use crate::convert::{self, Mismatch};
use crate::operation::{ACCOUNTS, ACTIVITIES, CANCEL, CREATE, ORDER, ORDERS, Operation, POSITIONS, QUOTE, SEARCH};
use crate::{Error, wire};

const PRODUCTION: &str = "https://my.wealthsimple.com/graphql";
const API_VERSION: &str = "12";
const MAX_PAGES: usize = 20;
const ACCOUNT_PAGE: usize = 25;
const POSITION_PAGE: usize = 50;
const ACTIVITY_PAGE: usize = 50;
const ORDER_PAGE: usize = 25;
const REPORTING_CURRENCY: &str = "CAD";
/// The `branchId` the web app sends when it looks up an order.
const BRANCH: &str = "TR";

pub struct Api {
    http: Client,
    endpoints: Endpoints,
    graphql: String,
    session: Mutex<Session>,
    store: Arc<dyn Store>,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api").field("graphql", &self.graphql).finish_non_exhaustive()
    }
}

impl Api {
    #[must_use]
    pub fn new(http: Client, session: Session, store: Arc<dyn Store>) -> Self {
        Self::with(http, Endpoints::production(), PRODUCTION.to_owned(), session, store)
    }

    #[must_use]
    pub fn at(base: &str, http: Client, session: Session, store: Arc<dyn Store>) -> Self {
        let base = base.trim_end_matches('/');
        Self::with(http, Endpoints::at(base), format!("{base}/graphql"), session, store)
    }

    fn with(http: Client, endpoints: Endpoints, graphql: String, session: Session, store: Arc<dyn Store>) -> Self {
        Self { http, endpoints, graphql, session: Mutex::new(session), store }
    }

    pub async fn identity(&self) -> IdentityId {
        self.session.lock().await.identity().clone()
    }

    pub async fn accounts(&self) -> Result<Vec<Account>, Error> {
        let identity = self.identity().await;
        let nodes = self
            .pages(
                &ACCOUNTS,
                |cursor| json!({"identityId": identity.as_str(), "pageSize": ACCOUNT_PAGE, "cursor": cursor}),
                |data| data.identity.map(|i| i.accounts).ok_or_else(|| Mismatch("no identity".to_owned())),
                usize::MAX,
            )
            .await?;
        Ok(keep(ACCOUNTS.name(), nodes, convert::account))
    }

    pub async fn positions(&self, account: &AccountId) -> Result<Vec<Position>, Error> {
        let identity = self.identity().await;
        let nodes = self
            .pages(
                &POSITIONS,
                |cursor| {
                    json!({
                        "identityId": identity.as_str(),
                        "currency": REPORTING_CURRENCY,
                        "first": POSITION_PAGE,
                        "cursor": cursor,
                        "accountIds": [account.as_str()],
                        "includeSecurity": true,
                    })
                },
                |data| {
                    data.identity
                        .map(|i| i.financials.current.positions)
                        .ok_or_else(|| Mismatch("no identity".to_owned()))
                },
                usize::MAX,
            )
            .await?;
        Ok(keep(POSITIONS.name(), nodes, |node| convert::position(account, node)))
    }

    pub async fn activities(&self, account: &AccountId, limit: usize) -> Result<Vec<Activity>, Error> {
        let end = (Utc::now() + Duration::days(1)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let nodes = self
            .pages(
                &ACTIVITIES,
                |cursor| {
                    json!({
                        "first": limit.min(ACTIVITY_PAGE),
                        "cursor": cursor,
                        "orderBy": "OCCURRED_AT_DESC",
                        "condition": {"accountIds": [account.as_str()], "endDate": end},
                    })
                },
                |data| Ok(data.activity_feed_items),
                limit,
            )
            .await?;
        Ok(keep(ACTIVITIES.name(), nodes, |node| convert::activity(node).map(Some)))
    }

    pub async fn search(&self, query: &str) -> Result<Vec<Security>, Error> {
        let data = self.run(&SEARCH, json!({"query": query})).await?;
        Ok(keep(SEARCH.name(), data.security_search.results, |node| convert::security(node).map(Some)))
    }

    pub async fn quote(&self, security: &SecurityId) -> Result<Quote, Error> {
        let data = self.run(&QUOTE, json!({"id": security.as_str(), "currency": null})).await?;
        let node = data.security.ok_or_else(|| Error::NotFound(security.to_string()))?;
        convert::quote(node).map_err(|Mismatch(detail)| Error::Shape { operation: QUOTE.name(), detail })
    }

    pub async fn orders(&self, account: &AccountId) -> Result<Vec<Placed>, Error> {
        let identity = self.identity().await;
        let statuses: Vec<&str> = convert::open_statuses().collect();
        let nodes = self
            .pages(
                &ORDERS,
                |cursor| json!({"identityId": identity.as_str(), "statuses": statuses, "first": ORDER_PAGE, "cursor": cursor}),
                |data| {
                    data.identity
                        .map(|i| i.order_service_extended_order_feed)
                        .ok_or_else(|| Mismatch("no identity".to_owned()))
                },
                usize::MAX,
            )
            .await?;
        let mine = nodes.into_iter().filter(|node| node.canonical_account_id == account.as_str()).collect();
        Ok(keep(ORDERS.name(), mine, |node| convert::placed(node).map(Some)))
    }

    pub async fn order(&self, key: &IdempotencyKey) -> Result<Report, Error> {
        let data = self.run(&ORDER, json!({"branchId": BRANCH, "externalId": key.as_str()})).await?;
        let node = data.so_orders_extended_order.ok_or_else(|| Error::NotFound(format!("order {key}")))?;
        convert::report(key.clone(), node).map_err(|Mismatch(detail)| Error::Shape { operation: ORDER.name(), detail })
    }

    /// Resends only when the first attempt was refused unprocessed (an expired session, or HTTP 429/503).
    pub async fn place(&self, order: &Order, key: &IdempotencyKey) -> Result<OrderId, Error> {
        let input = convert::create_input(order, key)?;
        let result = self.run(&CREATE, json!({"input": input})).await?.so_orders_create_order;
        if let Some(problems) = wire::OrderProblem::describe(result.errors) {
            return Err(Error::Rejected(problems));
        }
        let created = result
            .order
            .ok_or_else(|| Error::Shape { operation: CREATE.name(), detail: "no order and no errors".to_owned() })?;
        OrderId::parse(created.order_id).map_err(|e| Error::Shape { operation: CREATE.name(), detail: e.to_string() })
    }

    pub async fn cancel(&self, key: &IdempotencyKey) -> Result<(), Error> {
        let result = self.run(&CANCEL, json!({"cancelOrderRequest": {"externalId": key.as_str()}})).await?;
        match wire::OrderProblem::describe(result.order_service_cancel_order.errors) {
            Some(problems) => Err(Error::Rejected(problems)),
            None => Ok(()),
        }
    }

    async fn pages<T, N>(
        &self,
        operation: &Operation<T>,
        variables: impl Fn(Option<&str>) -> Value,
        connection: impl Fn(T) -> Result<wire::Connection<N>, Mismatch>,
        limit: usize,
    ) -> Result<Vec<N>, Error>
    where
        T: DeserializeOwned,
    {
        let mut nodes = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let data = self.run(operation, variables(cursor.as_deref())).await?;
            let page =
                connection(data).map_err(|Mismatch(detail)| Error::Shape { operation: operation.name(), detail })?;
            let next = page.next();
            tracing::debug!(operation = operation.name(), received = page.edges.len(), more = next.is_some(), "page");
            nodes.extend(page.edges.into_iter().map(|edge| edge.node));
            if nodes.len() >= limit {
                nodes.truncate(limit);
                break;
            }
            match next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        Ok(nodes)
    }

    async fn run<T: DeserializeOwned>(&self, operation: &Operation<T>, variables: Value) -> Result<T, Error> {
        let body = json!({"operationName": operation.name(), "query": operation.query(), "variables": variables});
        let mut refreshed = false;
        loop {
            let used = self.session.lock().await.clone();
            let response = self.http.post(&self.graphql, &headers(&used), &body).await?;
            let envelope = if response.is_success() { Some(response.json::<wire::Envelope>()?) } else { None };
            let (data, problems) =
                envelope.map_or((None, Vec::new()), |e| (Some(e.data), e.errors.unwrap_or_default()));

            if response.status() == 401 || problems.iter().any(wire::Problem::unauthenticated) {
                if refreshed {
                    return Err(Error::Unauthorized);
                }
                self.refresh(&used).await?;
                refreshed = true;
                continue;
            }
            let Some(data) = data else {
                return Err(Error::Http { operation: operation.name(), status: response.status() });
            };
            let messages = || problems.iter().map(|p| p.message.as_str()).collect::<Vec<_>>().join(". ");
            let data = match data {
                Some(data) if !data.is_null() => data,
                _ => return Err(Error::Graphql { operation: operation.name(), messages: messages() }),
            };
            if !problems.is_empty() {
                tracing::warn!(operation = operation.name(), messages = messages(), "partial GraphQL response");
            }
            return serde_json::from_value(data)
                .map_err(|e| Error::Shape { operation: operation.name(), detail: e.to_string() });
        }
    }

    async fn refresh(&self, rejected: &Session) -> Result<(), Error> {
        let mut session = self.session.lock().await;
        if *session != *rejected || self.adopt_stored(&mut session, rejected)? {
            return Ok(());
        }
        match session.refresh(&self.http, &self.endpoints).await {
            Ok(()) => {}
            Err(ws_auth::Error::Expired) if self.adopt_stored(&mut session, rejected)? => return Ok(()),
            Err(ws_auth::Error::Expired) => return Err(Error::Unauthorized),
            Err(other) => return Err(other.into()),
        }
        self.store.save(&session)?;
        tracing::info!("session refreshed");
        Ok(())
    }

    /// Refresh tokens rotate, so another process sharing the keychain entry may already hold the newer session.
    fn adopt_stored(&self, session: &mut Session, rejected: &Session) -> Result<bool, Error> {
        match self.store.load()? {
            Some(stored) if stored != *rejected => {
                *session = stored;
                tracing::info!("using the session another process refreshed");
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn headers(session: &Session) -> Headers {
    session
        .headers()
        .with("x-ws-api-version", API_VERSION)
        .with("x-ws-profile", "trade")
        .with("x-ws-locale", "en-CA")
        .with("x-platform-os", "web")
}

fn keep<N, D>(operation: &'static str, nodes: Vec<N>, convert: impl Fn(N) -> Result<Option<D>, Mismatch>) -> Vec<D> {
    nodes
        .into_iter()
        .filter_map(|node| match convert(node) {
            Ok(item) => item,
            Err(Mismatch(detail)) => {
                tracing::warn!(operation, detail, "skipping item");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[derive(Default)]
    struct Memory(StdMutex<Option<Session>>);

    impl Store for Memory {
        fn load(&self) -> Result<Option<Session>, ws_auth::Error> {
            Ok(self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone())
        }

        fn save(&self, session: &Session) -> Result<(), ws_auth::Error> {
            *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(session.clone());
            Ok(())
        }

        fn clear(&self) -> Result<(), ws_auth::Error> {
            *self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            Ok(())
        }
    }

    fn session(access: &str) -> Session {
        serde_json::from_value(json!({
            "device": {"device_id": "dev", "client_id": "cid"},
            "id": "sid",
            "tokens": {"access": access, "refresh": "r1"},
            "identity": "identity-abc",
            "scope": "read",
        }))
        .unwrap_or_else(|e| panic!("{e}"))
    }

    fn api(server: &MockServer, access: &str) -> (Api, Arc<Memory>) {
        let store = Arc::new(Memory::default());
        let http = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        (Api::at(&server.uri(), http, session(access), store.clone()), store)
    }

    fn account(id: &str) -> Value {
        json!({"node": {"id": id, "status": "open", "currency": "CAD", "unifiedAccountType": "SELF_DIRECTED_TFSA"}})
    }

    fn accounts_page(edges: &[Value], next: Option<&str>) -> Value {
        json!({"data": {"identity": {"accounts": {
            "edges": edges,
            "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next},
        }}}})
    }

    #[tokio::test]
    async fn follows_pages_with_the_web_app_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", "Bearer good"))
            .and(header("x-ws-profile", "trade"))
            .and(header("x-ws-api-version", "12"))
            .and(body_partial_json(
                json!({"operationName": "FetchAllAccountFinancials", "variables": {"cursor": "p2"}}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(accounts_page(&[account("b")], None)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(json!({"variables": {"identityId": "identity-abc", "cursor": null}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(accounts_page(&[account("a")], Some("p2"))))
            .mount(&server)
            .await;

        let (api, _) = api(&server, "good");
        let ids: Vec<String> =
            api.accounts().await.unwrap_or_else(|e| panic!("{e}")).iter().map(|a| a.id().to_string()).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[tokio::test]
    async fn refreshes_once_and_saves_the_session() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", "Bearer fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(accounts_page(&[account("a")], None)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": null,
                "errors": [{"message": "Not authorized", "extensions": {"code": "UNAUTHENTICATED"}}],
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"access_token": "fresh", "refresh_token": "r2"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (api, store) = api(&server, "stale");
        assert_eq!(api.accounts().await.map(|a| a.len()).ok(), Some(1));
        let saved = store.load().ok().flatten().unwrap_or_else(|| panic!("not saved"));
        assert_ne!(saved, session("stale"));
        assert_eq!(saved, *api.session.lock().await);
    }

    #[tokio::test]
    async fn uses_a_session_another_process_already_refreshed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", "Bearer fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(accounts_page(&[account("a")], None)))
            .mount(&server)
            .await;
        Mock::given(method("POST")).and(path("/graphql")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(400))
            .expect(0)
            .mount(&server)
            .await;

        let (api, store) = api(&server, "stale");
        store.save(&session("fresh")).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(api.accounts().await.map(|a| a.len()).ok(), Some(1));
        assert_eq!(*api.session.lock().await, session("fresh"));
    }

    #[tokio::test]
    async fn gives_up_when_the_refreshed_session_is_also_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/graphql")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"access_token": "fresh", "refresh_token": "r2"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (api, _) = api(&server, "stale");
        assert!(matches!(api.accounts().await, Err(Error::Unauthorized)));
    }

    #[tokio::test]
    async fn reports_graphql_errors_and_missing_securities() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(json!({"operationName": "FetchSecuritySearchResult"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"errors": [{"message": "boom"}]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(json!({"operationName": "FetchSecurityQuoteV2"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"security": null}})))
            .mount(&server)
            .await;

        let (api, _) = api(&server, "good");
        assert!(matches!(api.search("xeqt").await, Err(Error::Graphql { messages, .. }) if messages == "boom"));
        let missing = SecurityId::parse("sec-s-missing").unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(api.quote(&missing).await, Err(Error::NotFound(_))));
    }

    fn feed_row(account: &str, order: &str) -> Value {
        json!({"node": {
            "id": "order-2fb5c80f-0000-4000-8000-000000000000", "orderId": order, "canonicalAccountId": account,
            "createdAtUtc": "2026-10-03T13:50:17.241Z", "status": "PLACED", "side": "BUY",
            "submittedQuantity": "0.2058", "securityCurrency": "CAD", "securityId": "sec-s-abc", "symbol": "CCO",
        }})
    }

    #[tokio::test]
    async fn lists_open_orders_for_one_account() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(json!({
                "operationName": "OrderServiceExtendedOrderFeed",
                "variables": {"identityId": "identity-abc", "statuses": ["CANCEL_PENDING", "CONTINGENT", "NEW",
                    "PARTIALLY_FILLED", "PENDING_FUND_TRANSFER", "PENDING_REVIEW", "PENDING_SUBMISSION", "PLACED", "SUBMITTED"]},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"identity": {
                "orderServiceExtendedOrderFeed": {
                    "edges": [feed_row("rrsp-1", "order-a"), feed_row("tfsa-1", "order-b")],
                    "pageInfo": {"hasNextPage": false, "endCursor": null},
                },
            }}})))
            .mount(&server)
            .await;

        let (api, _) = api(&server, "good");
        let account = AccountId::parse("rrsp-1").unwrap_or_else(|e| panic!("{e}"));
        let orders = api.orders(&account).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(orders.iter().map(|o| o.id().as_str()).collect::<Vec<_>>(), ["order-a"]);
    }

    fn value_buy() -> Order {
        let account = AccountId::parse("rrsp-1").unwrap_or_else(|e| panic!("{e}"));
        let security = SecurityId::parse("sec-s-abc").unwrap_or_else(|e| panic!("{e}"));
        let value =
            ws_core::Money::new(rust_decimal::dec!(25), ws_core::Currency::Cad).unwrap_or_else(|e| panic!("{e}"));
        Order::value_buy(account, security, value).unwrap_or_else(|e| panic!("{e}"))
    }

    #[tokio::test]
    async fn places_with_the_key_as_external_id() {
        let server = MockServer::start().await;
        let key = IdempotencyKey::fresh();
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(json!({
                "operationName": "SoOrdersOrderCreate",
                "variables": {"input": {"externalId": key.as_str(), "orderType": "BUY_VALUE", "value": 25.0}},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"soOrdersCreateOrder": {
                "errors": null, "order": {"orderId": "order-xyz", "createdAt": "2026-10-03T13:50:17.241Z"},
            }}})))
            .expect(1)
            .mount(&server)
            .await;

        let (api, _) = api(&server, "good");
        assert_eq!(api.place(&value_buy(), &key).await.map(|id| id.to_string()).ok().as_deref(), Some("order-xyz"));
    }

    #[tokio::test]
    async fn reports_order_errors_and_sends_nothing_for_uncaptured_shapes() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"soOrdersCreateOrder": {
                "errors": [{"code": "INSUFFICIENT_FUNDS", "message": "Not enough cash"}], "order": null,
            }}})))
            .expect(1)
            .mount(&server)
            .await;

        let (api, _) = api(&server, "good");
        let rejected = api.place(&value_buy(), &IdempotencyKey::fresh()).await;
        assert!(matches!(rejected, Err(Error::Rejected(reason)) if reason == "INSUFFICIENT_FUNDS: Not enough cash"));
        let sell = ws_core::fixtures::limit_sell(rust_decimal::dec!(1), rust_decimal::dec!(1));
        assert!(matches!(api.place(&sell, &IdempotencyKey::fresh()).await, Err(Error::Unsupported(_))));
    }

    #[tokio::test]
    async fn cancels_by_external_id_and_reports_refusals() {
        let server = MockServer::start().await;
        let key = IdempotencyKey::fresh();
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(json!({
                "operationName": "SoOrdersOrderCancel",
                "variables": {"cancelOrderRequest": {"externalId": key.as_str()}},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"orderServiceCancelOrder": {
                "externalId": key.as_str(), "errors": null,
            }}})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"orderServiceCancelOrder": {
                "externalId": null, "errors": [{"code": "ORDER_NOT_CANCELLABLE", "message": null}],
            }}})))
            .mount(&server)
            .await;

        let (api, _) = api(&server, "good");
        assert!(api.cancel(&key).await.is_ok());
        let refused = api.cancel(&IdempotencyKey::fresh()).await;
        assert!(matches!(refused, Err(Error::Rejected(reason)) if reason == "ORDER_NOT_CANCELLABLE"));
    }

    #[tokio::test]
    async fn looks_up_one_order_by_external_id() {
        let server = MockServer::start().await;
        let key = IdempotencyKey::fresh();
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_partial_json(json!({
                "operationName": "FetchSoOrdersExtendedOrder",
                "variables": {"branchId": "TR", "externalId": key.as_str()},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"soOrdersExtendedOrder": {
                "canonicalAccountId": "tfsa-1", "securityId": "sec-s-abc", "status": "cancelled",
                "orderType": "BUY_QUANTITY", "securityCurrency": "CAD", "submittedQuantity": "1.0000",
                "limitPrice": "0.2000", "submittedAtUtc": "2026-10-03T14:22:13.358Z",
            }}})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"soOrdersExtendedOrder": null}})))
            .mount(&server)
            .await;

        let (api, _) = api(&server, "good");
        let report = api.order(&key).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((report.status(), report.account().as_str()), (ws_core::Status::Cancelled, "tfsa-1"));
        assert!(matches!(api.order(&IdempotencyKey::fresh()).await, Err(Error::NotFound(_))));
    }
}
