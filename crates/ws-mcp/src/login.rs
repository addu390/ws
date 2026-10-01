//! Interactive login, in the terminal or through a Chrome window.

use std::io::{self, BufRead, Write};

use anyhow::{Context, bail};
use ws_auth::{Browser, Credentials, Endpoints, Keyring, Login, Outcome, Scope, Session, Store};
use ws_net::Client;

const CODE_ATTEMPTS: usize = 3;

pub async fn terminal(scope: Scope) -> anyhow::Result<()> {
    let username = prompt("Wealthsimple email: ")?;
    let password = rpassword::prompt_password("Password: ").context("reading password")?;
    let login = Login::new(Client::chrome()?, Endpoints::production());

    let session = match login.start(Credentials::new(username, password), scope).await? {
        Outcome::Done(session) => session,
        Outcome::NeedsCode(pending) => {
            let mut attempt = 1;
            loop {
                let code = prompt("2FA code: ")?;
                match pending.submit(&code).await {
                    Ok(session) => break session,
                    Err(e) if attempt < CODE_ATTEMPTS => {
                        eprintln!("{e}. Try again.");
                        attempt += 1;
                    }
                    Err(e) => bail!(e),
                }
            }
        }
    };
    save(&session, scope)
}

pub async fn browser(scope: Scope) -> anyhow::Result<()> {
    eprintln!("Opening Chrome. Sign in to Wealthsimple there; the window closes by itself when you're done.");
    let session = Browser::new(Client::chrome()?, Endpoints::production()).login().await?;
    save(&session, scope)
}

fn save(session: &Session, requested: Scope) -> anyhow::Result<()> {
    Keyring::default().save(session)?;
    eprintln!("Logged in as {} with {:?} scope. Session saved to the system keychain.", session.identity(), session.scope());
    if requested == Scope::Read && session.scope() == Scope::Write {
        eprintln!(
            "Note: browser logins always have full access, so this session can place orders. Read-only mode \
             relies on ws-mcp's own policy checks. For a session that is read-only at the Wealthsimple level, \
             use the terminal login (without --browser)."
        );
    }
    Ok(())
}

fn prompt(label: &str) -> anyhow::Result<String> {
    eprint!("{label}");
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).context("reading input")?;
    let value = line.trim().to_owned();
    if value.is_empty() {
        bail!("no input given");
    }
    Ok(value)
}
