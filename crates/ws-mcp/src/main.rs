//! Wealthsimple MCP server and CLI.

mod clear;
mod cli;
mod login;
mod read;
mod server;
mod status;
mod trade;
mod wiring;

use anyhow::bail;
use clap::Parser;
use tracing_subscriber::EnvFilter;
use ws_auth::Scope;

use crate::cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .init();

    match Cli::parse().command.unwrap_or(Command::Serve) {
        Command::Login { write, browser } => {
            let scope = if write { Scope::Write } else { Scope::Read };
            if browser { login::browser(scope).await } else { login::terminal(scope).await }
        }
        Command::Status => status::show().await,
        Command::Clear => clear::run(),
        Command::Serve => bail!("the MCP server is not implemented yet"),
    }
}
