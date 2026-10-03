use anyhow::bail;
use ws_auth::{Keyring, Store};
use ws_config::Mode;
use ws_core::{Account, AccountId};

use crate::home::Home;
use crate::prompt::ask;
use crate::{config, login, resolve, status, wiring};

const MODES: [(Mode, &str); 3] = [
    (Mode::Read, "account data only, the agent cannot place orders"),
    (Mode::Paper, "orders are simulated against live prices, nothing reaches Wealthsimple"),
    (Mode::Trade, "real orders, only in the accounts you pick and within your limits"),
];

pub async fn run() -> anyhow::Result<()> {
    if Keyring::default().load()?.is_none() {
        login::run().await?;
    }

    eprintln!("Mode:");
    for (n, (mode, about)) in MODES.iter().enumerate() {
        eprintln!("  {}. {:<5}  {about}", n + 1, mode.to_string());
    }
    let mode = loop {
        if let Some(&[n]) = numbers(&ask("Choose 1-3:")?, MODES.len()).as_deref() {
            break MODES[n].0;
        }
        eprintln!("Type one number from 1 to {}.", MODES.len());
    };

    let allowed = if mode == Mode::Read { Vec::new() } else { pick_accounts().await? };
    let allowed = allowed.iter().map(AccountId::as_str).collect::<Vec<_>>().join(",");
    config::set(&Home::locate()?, &[("mode", &mode.to_string()), ("allowed_accounts", &allowed)])?;
    eprintln!("Saved. Change any limit with `ws-mcp config <setting> <value>`.\n");
    status::show().await
}

async fn pick_accounts() -> anyhow::Result<Vec<AccountId>> {
    let accounts: Vec<Account> = wiring::api()?.accounts().await?.into_iter().filter(Account::tradable).collect();
    if accounts.is_empty() {
        bail!("none of your accounts can trade through ws-mcp. Only self-directed accounts can");
    }
    eprintln!("Accounts that can trade:");
    for (n, account) in accounts.iter().enumerate() {
        eprintln!("  {}. {}", n + 1, resolve::label(account));
    }
    let picked = loop {
        let answer = ask("Which may the agent trade in? Numbers separated by commas, or blank for none:")?;
        if let Some(picked) = numbers(&answer, accounts.len()) {
            break picked;
        }
        eprintln!("Type numbers from 1 to {}, like 1,3.", accounts.len());
    };
    Ok(picked.into_iter().map(|n| accounts[n].id().clone()).collect())
}

fn numbers(answer: &str, count: usize) -> Option<Vec<usize>> {
    let mut picked = Vec::new();
    for part in answer.split([',', ' ']).filter(|p| !p.is_empty()) {
        let n: usize = part.parse().ok()?;
        if !(1..=count).contains(&n) {
            return None;
        }
        if !picked.contains(&(n - 1)) {
            picked.push(n - 1);
        }
    }
    Some(picked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbered_choices() {
        assert_eq!(numbers("1, 3", 3), Some(vec![0, 2]));
        assert_eq!(numbers("2 2", 3), Some(vec![1]));
        assert_eq!(numbers("", 3), Some(vec![]));
        assert_eq!(numbers("4", 3), None);
        assert_eq!(numbers("0", 3), None);
        assert_eq!(numbers("tfsa", 3), None);
    }
}
