use std::time::Duration;

use rmcp::schemars::{self, JsonSchema};
use rmcp::service::ElicitationError;
use rmcp::{Peer, RoleServer};
use rust_decimal::Decimal;
use serde::Deserialize;
use ws_core::{Kind, Side, Size, Tif};
use ws_policy::{Approval, Approver, Guard, Ticket, TicketId};

use crate::action::Failure;
use crate::resolve;

const WAIT: Duration = Duration::from_mins(5);

#[derive(Deserialize, JsonSchema)]
struct Consent {
    /// Tick to approve this order.
    approve: bool,
}

rmcp::elicit_safe!(Consent);

/// Asks the user through the client's own prompt when a ticket waits for approval and `approve_in_chat` is on.
/// In every other case nothing is approved, and placing an unapproved ticket is refused as before.
pub async fn ask(guard: &Guard, peer: &Peer<RoleServer>, id: &TicketId) -> Result<(), Failure> {
    if !guard.approves_in_chat() {
        return Ok(());
    }
    let Ok(ticket) = guard.ticket(id) else { return Ok(()) };
    if ticket.approval() != Approval::Pending {
        return Ok(());
    }
    let account = guard.reader().accounts().await?.into_iter().find(|a| a.id() == ticket.order().account());
    let account = account.map_or_else(|| ticket.order().account().to_string(), |a| resolve::label(&a));
    let question = format!("Approve this order?\n\n{}\nAccount: {account}\nAbout {}", describe(&ticket), ticket.value());
    match peer.elicit_with_timeout::<Consent>(question, Some(WAIT)).await {
        Ok(Some(Consent { approve: true })) => {
            guard.approve(id, Approver::Chat)?;
            Ok(())
        }
        Err(ElicitationError::CapabilityNotSupported) => Ok(()),
        Ok(_) | Err(ElicitationError::UserDeclined | ElicitationError::UserCancelled) => {
            Err(Failure::Invalid("the user did not approve the order. Do not place it again unless they ask"))
        }
        Err(e) => Err(Failure::Unresolved(format!("could not ask for approval in the chat: {e}"))),
    }
}

/// Like `Buy 7 shares of XEQT:TSX, limit $40.00`.
pub fn describe(ticket: &Ticket) -> String {
    let order = ticket.order();
    let security = ticket.symbol().map_or_else(|| order.security().to_string(), str::to_owned);
    let verb = match order.side() {
        Side::Buy => "Buy",
        Side::Sell => "Sell",
    };
    let size = match order.size() {
        Size::Shares(quantity) if quantity.value() == Decimal::ONE => "1 share".to_owned(),
        Size::Shares(quantity) => format!("{} shares", quantity.value()),
        Size::Value(value) => value.to_string(),
    };
    let price = match order.kind() {
        Kind::Market => "at market".to_owned(),
        Kind::Limit { limit } => format!("limit {limit}"),
        Kind::StopLimit { stop, limit } => format!("stop {stop}, limit {limit}"),
    };
    let until = if order.tif() == Tif::Gtc { ", until cancelled" } else { "" };
    format!("{verb} {size} of {security}, {price}{until}")
}
