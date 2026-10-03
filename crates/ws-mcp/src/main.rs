mod action;
mod clear;
mod cli;
mod config;
mod consent;
mod control;
mod dashboard;
#[cfg(test)]
mod fake;
mod home;
mod login;
mod prompt;
mod read;
mod resolve;
mod server;
mod setup;
mod status;
mod tool;
mod trade;
mod wiring;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Command};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")))
        .init();

    match Cli::parse().command.unwrap_or(Command::Serve) {
        Command::Setup => setup::run().await,
        Command::Status => status::show().await,
        Command::Config { key, value } => config::run(key.as_deref(), value.as_deref()),
        Command::Login => login::run().await,
        Command::Logout => clear::logout(),
        Command::Clear => clear::run(),
        Command::Serve => wiring::serve().await,
        Command::Dashboard => dashboard::run().await,
        Command::Approve { ticket_id } => control::approve(&ticket_id),
        Command::Kill => control::kill(),
        Command::Resume => control::resume(),
        Command::Read(action) => wiring::read(action).await,
        Command::Trade(action) => wiring::trade(action).await,
    }
}
