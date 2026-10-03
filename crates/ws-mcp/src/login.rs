use ws_auth::{Browser, Endpoints, Keyring, Store};
use ws_net::Client;

pub async fn run() -> anyhow::Result<()> {
    eprintln!("Opening Chrome. Sign in to Wealthsimple there. The window closes by itself when you're done.");
    let session = Browser::new(Client::chrome()?, Endpoints::production()).login().await?;
    Keyring::default().save(&session)?;
    eprintln!("Logged in. Session saved to the system keychain.");
    Ok(())
}
