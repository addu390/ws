use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chromiumoxide::{Browser as Chrome, BrowserConfig};
use futures::StreamExt;
use percent_encoding::percent_decode_str;
use serde::Deserialize;
use tokio::time::Instant;
use uuid::Uuid;
use ws_net::Client;

use crate::{Device, Endpoints, Error, Introspection, Scope, Session, Tokens};

const ACCESS_COOKIE: &str = "_oauth2_access_v2";
const DEVICE_COOKIE: &str = "wssdi";
const COOKIE_DOMAIN: &str = "wealthsimple.com";
const PROFILE_PREFIX: &str = "ws-mcp-login-";
const POLL: Duration = Duration::from_secs(1);
const DEADLINE: Duration = Duration::from_mins(10);

#[derive(Debug, Clone)]
pub struct Browser {
    client: Client,
    endpoints: Endpoints,
}

impl Browser {
    #[must_use]
    pub fn new(client: Client, endpoints: Endpoints) -> Self {
        Self { client, endpoints }
    }

    /// Web sessions carry the web app's full read+write scope, and Wealthsimple refuses to narrow it on refresh.
    pub async fn login(&self) -> Result<Session, Error> {
        let captured = capture(self.endpoints.login()).await?;
        let device = Device::adopt(captured.device_id, &self.client, &self.endpoints).await?;
        let session_id = Uuid::new_v4().to_string();
        let identity = Introspection::fetch(&self.client, &self.endpoints, &captured.tokens, &device, &session_id)
            .await?
            .ok_or(Error::Rejected { status: 401, message: "captured token was not accepted".to_owned() })?
            .identity()
            .clone();
        Ok(Session::new(device, session_id, captured.tokens, identity, captured.scope))
    }

    pub fn purge_leftovers() -> Result<usize, Error> {
        purge_in(&std::env::temp_dir())
    }
}

fn purge_in(dir: &Path) -> Result<usize, Error> {
    let failed = |e: std::io::Error| Error::Browser(format!("cleaning up browser profiles: {e}"));
    let mut removed = 0;
    for entry in fs::read_dir(dir).map_err(failed)? {
        let entry = entry.map_err(failed)?;
        let is_profile = entry.file_name().to_str().is_some_and(|name| name.starts_with(PROFILE_PREFIX));
        if is_profile && entry.file_type().map_err(failed)?.is_dir() {
            fs::remove_dir_all(entry.path()).map_err(failed)?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[derive(Debug, PartialEq, Eq)]
struct Captured {
    device_id: String,
    tokens: Tokens,
    scope: Scope,
}

impl Captured {
    fn from_cookies(access: &str, device_id: &str) -> Option<Self> {
        #[derive(Deserialize)]
        struct Cookie {
            access_token: String,
            refresh_token: String,
            scope: String,
            identity_canonical_id: String,
        }
        let json = percent_decode_str(access).decode_utf8().ok()?;
        let cookie: Cookie = serde_json::from_str(&json).ok()?;
        if cookie.identity_canonical_id.is_empty() || device_id.is_empty() {
            return None;
        }
        Some(Self {
            device_id: device_id.to_owned(),
            tokens: Tokens::new(cookie.access_token, cookie.refresh_token),
            scope: Scope::granted(&cookie.scope),
        })
    }
}

async fn capture(url: &str) -> Result<Captured, Error> {
    let profile = Profile::create()?;
    let config = BrowserConfig::builder()
        .with_head()
        .disable_default_args()
        .hide()
        .user_data_dir(profile.path())
        .arg("no-first-run")
        .arg("no-default-browser-check")
        .arg(("password-store", "basic"))
        .window_size(1100, 900)
        .build()
        .map_err(Error::Browser)?;
    let (mut chrome, mut handler) = Chrome::launch(config).await.map_err(chrome_error)?;
    let driver = tokio::spawn(async move { while handler.next().await.is_some() {} });

    let result = wait_for_login(&chrome, url).await;

    let _ = chrome.close().await;
    let _ = chrome.wait().await;
    driver.abort();
    drop(profile);
    result
}

async fn wait_for_login(chrome: &Chrome, url: &str) -> Result<Captured, Error> {
    chrome.new_page(url).await.map_err(chrome_error)?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        let cookies = chrome
            .get_cookies()
            .await
            .map_err(|_| Error::Browser("the browser was closed before login finished".to_owned()))?;
        let find = |name: &str| {
            cookies.iter().find(|c| c.name == name && c.domain.ends_with(COOKIE_DOMAIN)).map(|c| c.value.as_str())
        };
        if let (Some(access), Some(device)) = (find(ACCESS_COOKIE), find(DEVICE_COOKIE))
            && let Some(captured) = Captured::from_cookies(access, device)
        {
            return Ok(captured);
        }
        if Instant::now() >= deadline {
            return Err(Error::Browser("timed out waiting for login".to_owned()));
        }
        tokio::time::sleep(POLL).await;
    }
}

#[allow(clippy::needless_pass_by_value)]
fn chrome_error(error: chromiumoxide::error::CdpError) -> Error {
    Error::Browser(error.to_string())
}

/// A private Chrome profile directory, deleted when dropped so no cookies remain on disk.
struct Profile {
    path: PathBuf,
}

impl Profile {
    fn create() -> Result<Self, Error> {
        let path = std::env::temp_dir().join(format!("{PROFILE_PREFIX}{}", Uuid::new_v4()));
        ws_common::private_dir(&path).map_err(|e| Error::Browser(format!("creating browser profile: {e}")))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.path) {
            tracing::warn!(path = %self.path.display(), "could not delete browser profile: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_IN: &str = "{%22access_token%22:%22a1%22%2C%22token_type%22:%22Bearer%22%2C%22expires_in%22:1800\
        %2C%22refresh_token%22:%22r1%22%2C%22scope%22:%22read%20write%22\
        %2C%22identity_canonical_id%22:%22identity-me%22%2C%22profiles%22:{}}";

    #[test]
    fn captures_a_signed_in_web_session() {
        let captured = Captured::from_cookies(SIGNED_IN, "dev");
        let expected = Captured {
            device_id: "dev".to_owned(),
            tokens: Tokens::new("a1".to_owned(), "r1".to_owned()),
            scope: Scope::Write,
        };
        assert_eq!(captured, Some(expected));
    }

    #[test]
    fn waits_while_not_fully_signed_in() {
        let anonymous = "{%22access_token%22:%22a1%22%2C%22scope%22:%22read%22}";
        assert_eq!(Captured::from_cookies(anonymous, "dev"), None);
        assert_eq!(Captured::from_cookies(SIGNED_IN, ""), None);
        assert_eq!(Captured::from_cookies("not json", "dev"), None);
    }

    #[test]
    fn profile_is_private_and_deleted_on_drop() {
        let profile = Profile::create().unwrap_or_else(|e| panic!("{e}"));
        let path = profile.path().to_owned();
        fs::write(path.join("Cookies"), "secret").unwrap_or_else(|e| panic!("{e}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).map(|m| m.permissions().mode() & 0o777).ok(), Some(0o700));
        }
        drop(profile);
        assert!(!path.exists());
    }

    #[test]
    fn purges_leftover_profiles_only() {
        let scratch = ws_common::fixtures::Scratch::new();
        let dir = scratch.dir();
        let leftover = dir.join(format!("{PROFILE_PREFIX}abc"));
        let unrelated = dir.join(format!("not-{PROFILE_PREFIX}abc"));
        for path in [&leftover, &unrelated] {
            fs::create_dir_all(path.join("Default")).unwrap_or_else(|e| panic!("{e}"));
        }

        assert_eq!(purge_in(dir).ok(), Some(1));
        assert!(!leftover.exists());
        assert!(unrelated.exists());
    }
}
