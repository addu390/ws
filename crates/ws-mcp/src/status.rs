//! Reports the stored session.

use anyhow::bail;
use ws_auth::{Endpoints, Keyring, Store};
use ws_net::Client;

pub async fn show() -> anyhow::Result<()> {
    let store = Keyring::default();
    let Some(mut session) = store.load()? else { bail!("not logged in; run `ws-mcp login`") };
    let client = Client::chrome()?;
    let endpoints = Endpoints::production();

    if session.ensure_fresh(&client, &endpoints).await? {
        store.save(&session)?;
        eprintln!("Access token was expired and has been refreshed.");
    }
    let expires = session
        .introspect(&client, &endpoints)
        .await?
        .and_then(|i| i.expires_in())
        .map_or_else(|| "unknown".to_owned(), |s| format!("{} min", s / 60));
    eprintln!("Logged in as {} with {:?} scope. Access token expires in {expires}.", session.identity(), session.scope());
    Ok(())
}
