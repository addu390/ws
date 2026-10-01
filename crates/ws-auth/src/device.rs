//! Unauthenticated bootstrap of the device id (`wssdi` cookie) and the OAuth client id.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ws_net::{Client, Headers, Response};

use crate::{Endpoints, Error};

static APP_SCRIPT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)<script[^>]*\ssrc="([^"]*/app-[a-f0-9]+\.js)""#).unwrap_or_else(|e| panic!("{e}")));
static CLIENT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)"production"[^}]*clientId:"([a-f0-9]+)""#).unwrap_or_else(|e| panic!("{e}")));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    device_id: String,
    client_id: String,
}

impl Device {
    /// Loads the web app's login page like a browser would, to obtain the identifiers
    /// Wealthsimple expects on every subsequent request.
    pub async fn bootstrap(client: &Client, endpoints: &Endpoints) -> Result<Self, Error> {
        let page = login_page(client, endpoints).await?;
        let device_id = page.cookie("wssdi").ok_or(Error::Bootstrap("no wssdi cookie on the login page"))?.to_owned();
        let client_id = scrape_client_id(client, endpoints, page.text()).await?;
        tracing::debug!("bootstrapped device");
        Ok(Self { device_id, client_id })
    }

    /// Keeps a device id issued to a real browser, since its tokens may be bound to it.
    pub(crate) async fn adopt(device_id: String, client: &Client, endpoints: &Endpoints) -> Result<Self, Error> {
        let page = login_page(client, endpoints).await?;
        let client_id = scrape_client_id(client, endpoints, page.text()).await?;
        Ok(Self { device_id, client_id })
    }

    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

async fn login_page(client: &Client, endpoints: &Endpoints) -> Result<Response, Error> {
    let page = client.get(endpoints.login(), &Headers::new()).await?;
    if page.is_success() {
        Ok(page)
    } else {
        Err(Error::Rejected { status: page.status(), message: "login page unavailable".to_owned() })
    }
}

async fn scrape_client_id(client: &Client, endpoints: &Endpoints, html: &str) -> Result<String, Error> {
    let script = app_script(html).ok_or(Error::Bootstrap("no app script on the login page"))?;
    let bundle = client.get(&endpoints.resolve(script), &Headers::new()).await?;
    let id = client_id(bundle.text()).ok_or(Error::Bootstrap("no production clientId in the app script"))?;
    Ok(id.to_owned())
}

fn app_script(html: &str) -> Option<&str> {
    APP_SCRIPT.captures(html).and_then(|c| c.get(1)).map(|m| m.as_str())
}

fn client_id(js: &str) -> Option<&str> {
    CLIENT_ID.captures(js).and_then(|c| c.get(1)).map(|m| m.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn finds_app_script() {
        let html = r#"<head><script defer src="https://cdn.ws.com/app/app-3fa9c2.js"></script></head>"#;
        assert_eq!(app_script(html), Some("https://cdn.ws.com/app/app-3fa9c2.js"));
        assert_eq!(app_script(r#"<script src="/vendor-1.js">"#), None);
    }

    #[test]
    fn finds_production_client_id_only() {
        let js = r#"{"development":{clientId:"aaa1"},"production":{env:"p",clientId:"4da53ac2b0"}}"#;
        assert_eq!(client_id(js), Some("4da53ac2b0"));
    }

    #[tokio::test]
    async fn bootstraps_from_login_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/app/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "wssdi=d3v1ce; Path=/")
                    .set_body_string(r#"<script src="/static/app-abc123.js"></script>"#),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/static/app-abc123.js"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#""production":{clientId:"c11e47"}"#))
            .mount(&server)
            .await;

        let client = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        let device = Device::bootstrap(&client, &Endpoints::at(&server.uri())).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((device.device_id(), device.client_id()), ("d3v1ce", "c11e47"));
    }

    #[tokio::test]
    #[ignore = "hits the real Wealthsimple login page; run with --ignored"]
    async fn bootstraps_against_production() {
        let client = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        let device = Device::bootstrap(&client, &Endpoints::production()).await.unwrap_or_else(|e| panic!("{e}"));
        assert!(!device.device_id().is_empty() && !device.client_id().is_empty());
    }
}
