use rust_decimal::Decimal;
use ws_core::{AccountId, Currency, MarketStatus, Money, SecurityId};

use crate::TicketId;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Denial {
    #[error("trading is halted by the kill switch")]
    Killed,
    #[error("ws-mcp is in read mode. Set mode = \"paper\" or \"trade\" in config.toml to place orders")]
    ReadOnly,
    #[error("account {0} is not in allowed_accounts")]
    AccountNotAllowed(AccountId),
    #[error("security {0} is in blocked_securities")]
    Blocked(SecurityId),
    #[error("market orders are disabled (limit_only = true). Use a limit price")]
    MarketOrder,
    #[error("only CAD-priced securities can be traded, and this one trades in {0}")]
    Currency(Currency),
    #[error("orders are only allowed while the market is open. It is {0:?}")]
    MarketClosed(MarketStatus),
    #[error("limit {limit} is more than {max_pct}% worse than the quote {quote}")]
    Deviation { limit: Money, quote: Money, max_pct: Decimal },
    #[error("order value {value} exceeds max_order_value {max}")]
    OrderTooLarge { value: Money, max: Money },
    #[error("already placed {max} orders today (max_orders_per_day)")]
    TooManyOrders { max: u32 },
    #[error("order value {value} on top of {spent} spent today exceeds max_daily_spend {max}")]
    DailySpend { spent: Money, value: Money, max: Money },
    #[error(
        "ticket {0} needs approval. The user must run `ws-mcp approve {0}` in a terminal or approve it on `ws-mcp dashboard`"
    )]
    NeedsApproval(TicketId),
    #[error("approving in the chat is off. The user can turn it on with `ws-mcp config approve_in_chat true`")]
    ChatApprovalOff,
    #[error("ticket {0} does not exist or was already used")]
    UnknownTicket(TicketId),
    #[error("ticket {0} expired. Preview the order again")]
    Expired(TicketId),
    #[error("the price moved since ticket {0} was previewed and the order now needs approval. Preview it again")]
    Moved(TicketId),
    #[error("another order is being placed right now. Try again")]
    Busy,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Denied(#[from] Denial),
    #[error(transparent)]
    Broker(#[from] ws_broker::Error),
    #[error(transparent)]
    Audit(#[from] ws_audit::Error),
    #[error("policy state: {0}")]
    State(#[from] ws_common::Error),
    #[error(transparent)]
    Domain(#[from] ws_core::Error),
}
