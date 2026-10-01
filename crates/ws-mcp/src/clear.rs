//! Removes everything ws-mcp stores on this machine.

use ws_auth::{Browser, Keyring, Store};

pub fn run() -> anyhow::Result<()> {
    Keyring::default().clear()?;
    eprintln!("Removed the stored session from the system keychain.");

    match Browser::purge_leftovers()? {
        0 => {}
        n => eprintln!("Removed {n} leftover browser login profile(s)."),
    }

    eprintln!(
        "Nothing from ws-mcp remains on this machine. Wealthsimple may still list this login as an active \
         session or remembered device; remove it in Wealthsimple's security settings to end it there too."
    );
    Ok(())
}
