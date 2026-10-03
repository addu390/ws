use std::fmt::Display;

use ws_auth::{Endpoints, Keyring, Store};
use ws_net::Client;
use ws_policy::Kill;

use crate::home::Home;
use crate::wiring::Wiring;
use crate::{config, resolve};

pub async fn show() -> anyhow::Result<()> {
    let home = Home::locate()?;
    let config = config::load(&home)?;
    let store = Keyring::default();
    let Some(mut session) = store.load()? else {
        row("Login", "not logged in. Run `ws-mcp login`");
        row("Mode", config.mode());
        return Ok(());
    };
    let (client, endpoints) = (Client::chrome()?, Endpoints::production());
    if session.ensure_fresh(&client, &endpoints).await? {
        store.save(&session)?;
    }
    let minutes = session.introspect(&client, &endpoints).await?.and_then(|i| i.expires_in()).map(|s| s / 60);
    row("Login", minutes.map_or_else(|| "logged in".to_owned(), |m| format!("logged in, access token good for {m} min")));
    row("Mode", config.mode());

    let wiring = Wiring::build(&home)?;
    let limits = config.limits();
    let allowed: Vec<String> =
        wiring.reader.accounts().await?.iter().filter(|a| limits.allows(a.id())).map(resolve::label).collect();
    row("Trading in", if allowed.is_empty() { "no accounts. Run `ws-mcp setup` to pick some".to_owned() } else { allowed.join(", ") });

    let Some(guard) = wiring.guard else {
        row("Kill switch", if Kill::new(home.kill()).engaged() { "on" } else { "off" });
        return Ok(());
    };
    let status = guard.status()?;
    row("Kill switch", if status.killed() { "on. Run `ws-mcp resume` to trade again" } else { "off" });
    row(
        "Today",
        format!(
            "{} of {} orders, {} of {} spent",
            status.orders_today(),
            limits.max_orders_per_day(),
            status.spent_today(),
            limits.max_daily_spend()
        ),
    );
    row(
        "Approval",
        format!(
            "needed above {}, {} waiting{}",
            limits.require_approval_above(),
            guard.awaiting_approval()?.len(),
            if limits.approve_in_chat() { ", can be approved in the chat" } else { "" }
        ),
    );
    let mut rules = vec![format!("at most {} per order", limits.max_order_value())];
    if limits.limit_only() {
        rules.push("limit orders only".to_owned());
    }
    rules.push(format!("limit prices at most {}% worse than market", limits.max_limit_deviation_pct()));
    if limits.market_hours_only() {
        rules.push("market hours only".to_owned());
    }
    row("Rules", rules.join(", "));
    Ok(())
}

fn row(label: &str, value: impl Display) {
    println!("{label:<12} {value}");
}
