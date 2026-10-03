use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::RequestContext;
use rmcp::{RoleServer, tool, tool_router};

use crate::action::Action;
use crate::read::{Accounts, Activities, OrderStatus, PendingOrders, Positions, QuoteOf, Search};
use crate::server::Server;
use crate::trade::{CancelOrder, LimitsStatus, PlaceOrder, PreviewOrder};

#[tool_router(router = read_router, vis = "pub(crate)")]
impl Server {
    #[tool(description = Accounts::SUMMARY, annotations(read_only_hint = true))]
    async fn accounts(&self, Parameters(action): Parameters<Accounts>) -> Result<String, String> {
        self.answer(action).await
    }

    #[tool(description = Positions::SUMMARY, annotations(read_only_hint = true))]
    async fn positions(&self, Parameters(action): Parameters<Positions>) -> Result<String, String> {
        self.answer(action).await
    }

    #[tool(description = Activities::SUMMARY, annotations(read_only_hint = true))]
    async fn activities(&self, Parameters(action): Parameters<Activities>) -> Result<String, String> {
        self.answer(action).await
    }

    #[tool(description = Search::SUMMARY, annotations(read_only_hint = true))]
    async fn search(&self, Parameters(action): Parameters<Search>) -> Result<String, String> {
        self.answer(action).await
    }

    #[tool(description = QuoteOf::SUMMARY, annotations(read_only_hint = true))]
    async fn quote(&self, Parameters(action): Parameters<QuoteOf>) -> Result<String, String> {
        self.answer(action).await
    }

    #[tool(description = PendingOrders::SUMMARY, annotations(read_only_hint = true))]
    async fn pending_orders(&self, Parameters(action): Parameters<PendingOrders>) -> Result<String, String> {
        self.answer(action).await
    }

    #[tool(description = OrderStatus::SUMMARY, annotations(read_only_hint = true))]
    async fn order_status(&self, Parameters(action): Parameters<OrderStatus>) -> Result<String, String> {
        self.answer(action).await
    }
}

#[tool_router(router = trade_router, vis = "pub(crate)")]
impl Server {
    #[tool(description = PreviewOrder::SUMMARY, annotations(read_only_hint = false, destructive_hint = false))]
    async fn preview_order(&self, Parameters(action): Parameters<PreviewOrder>) -> Result<String, String> {
        self.act(action).await
    }

    #[tool(description = PlaceOrder::SUMMARY, annotations(read_only_hint = false, destructive_hint = true))]
    async fn place_order(
        &self,
        Parameters(action): Parameters<PlaceOrder>,
        context: RequestContext<RoleServer>,
    ) -> Result<String, String> {
        self.consent(&context.peer, &action.ticket_id).await?;
        self.act(action).await
    }

    #[tool(description = CancelOrder::SUMMARY, annotations(read_only_hint = false, destructive_hint = true))]
    async fn cancel_order(&self, Parameters(action): Parameters<CancelOrder>) -> Result<String, String> {
        self.act(action).await
    }

    #[tool(description = LimitsStatus::SUMMARY, annotations(read_only_hint = true))]
    async fn limits_status(&self, Parameters(action): Parameters<LimitsStatus>) -> Result<String, String> {
        self.act(action).await
    }
}
