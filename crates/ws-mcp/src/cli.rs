//! Command-line entry points.

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ws-mcp", version, about = "Unofficial Wealthsimple MCP server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log in and store the session in the system keychain.
    Login {
        /// Request write scope, needed to place orders. Read-only otherwise.
        #[arg(long)]
        write: bool,
        /// Sign in through a Chrome window instead of the terminal. Supports every 2FA method,
        /// but the session always has full read+write access.
        #[arg(long)]
        browser: bool,
    },
    /// Show who is logged in, refreshing the session if needed.
    Status,
    /// Delete everything ws-mcp stores on this machine, as if you never logged in.
    #[command(alias = "logout")]
    Clear,
    /// Run the MCP server on stdio (default).
    Serve,
}
