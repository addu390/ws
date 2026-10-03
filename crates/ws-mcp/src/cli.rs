use clap::{Parser, Subcommand};

use ws_policy::TicketId;

use crate::read::Read;
use crate::trade::Trade;

#[derive(Debug, Parser)]
#[command(name = "ws-mcp", version, about = "Unofficial Wealthsimple MCP server and CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log in, pick the mode and the accounts the agent may trade in, then show the status.
    Setup,
    /// Show the login, mode, accounts, kill switch, and today's limits.
    Status,
    /// Show every setting, or change one, like `ws-mcp config market_hours_only false`.
    Config {
        #[arg(requires = "value")]
        key: Option<String>,
        value: Option<String>,
    },
    /// Log in through a Chrome window and keep the session in the system keychain.
    Login,
    /// Remove the stored session.
    Logout,
    /// Remove the session, the audit log, tickets, budgets, and the paper ledger. Keeps your config.
    Clear,
    /// Run the MCP server on stdio (default).
    Serve,
    /// Open a dashboard in your browser.
    Dashboard,
    /// Approve a ticket that needs it, after showing what it would place. Asks you to confirm.
    Approve { ticket_id: TicketId },
    /// Engage the kill switch: no order can be previewed, placed, or cancelled until `resume`.
    Kill,
    /// Lift the kill switch. Asks you to confirm.
    Resume,
    #[command(flatten)]
    Read(Read),
    #[command(flatten)]
    Trade(Trade),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Option<Command> {
        Cli::try_parse_from(std::iter::once("ws-mcp").chain(args.iter().copied())).ok()?.command
    }

    #[test]
    fn read_commands_take_the_same_fields_as_the_tools() {
        assert!(matches!(
            parse(&["activities", "tfsa-1", "--limit", "5"]),
            Some(Command::Read(Read::Activities(a))) if a.account == "tfsa-1" && a.limit == Some(5)
        ));
        assert!(matches!(parse(&["pending-orders", "tfsa-1"]), Some(Command::Read(Read::PendingOrders(_)))));
        assert!(matches!(parse(&["search", "XEQT"]), Some(Command::Read(Read::Search(s))) if s.query == "XEQT"));
    }

    #[test]
    fn takes_names_but_rejects_malformed_ticket_ids() {
        assert!(matches!(parse(&["quote", "XEQT"]), Some(Command::Read(Read::Quote(q))) if q.security == "XEQT"));
        assert!(parse(&["approve", "../ticket-1"]).is_none());
    }

    #[test]
    fn order_commands_take_the_same_fields_as_the_tools() {
        assert!(matches!(
            parse(&["preview-order", "tfsa", "XEQT", "buy", "--shares", "1", "--limit", "0.20"]),
            Some(Command::Trade(Trade::Preview(p))) if p.shares == Some(rust_decimal::dec!(1)) && !p.good_till_cancelled
        ));
        assert!(matches!(parse(&["place-order", "ticket-1"]), Some(Command::Trade(Trade::Place(_)))));
        assert!(matches!(parse(&["approve", "ticket-1"]), Some(Command::Approve { .. })));
    }
}
