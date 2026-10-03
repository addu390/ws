use ws_auth::{Browser, Keyring, Store};

use crate::home::Home;

pub fn logout() -> anyhow::Result<()> {
    Keyring::default().clear()?;
    Browser::purge_leftovers()?;
    eprintln!(
        "Logged out. Wealthsimple may still list this login as an active session or remembered device. \
         Remove it in Wealthsimple's security settings to end it there too."
    );
    Ok(())
}

pub fn run() -> anyhow::Result<()> {
    let home = Home::locate()?;
    ws_common::remove(&home.audit())?;
    ws_common::remove_dir(&home.paper())?;
    ws_common::remove_dir(&home.trade())?;
    eprintln!("Removed the audit log, tickets, budgets, and paper ledger. Your config and kill switch are kept.");
    logout()
}
