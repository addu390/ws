use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ws_core::IdentityId;
use ws_net::{Client, Headers, Response};

use crate::{Device, Endpoints, Error};

pub(crate) const CLIENT_HEADER: (&str, &str) = ("x-wealthsimple-client", "@wealthsimple/wealthsimple");

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    access: String,
    refresh: String,
}

impl fmt::Debug for Tokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tokens").finish_non_exhaustive()
    }
}

impl Tokens {
    pub(crate) fn new(access: String, refresh: String) -> Self {
        Self { access, refresh }
    }

    #[must_use]
    pub fn access(&self) -> &str {
        &self.access
    }

    pub async fn refresh(
        &self,
        client: &Client,
        endpoints: &Endpoints,
        device: &Device,
        session_id: &str,
    ) -> Result<Self, Error> {
        let headers = Headers::new()
            .with(CLIENT_HEADER.0, CLIENT_HEADER.1)
            .with("x-ws-profile", "invest")
            .session(session_id)
            .device(device.device_id());
        let body = json!({
            "grant_type": "refresh_token",
            "refresh_token": self.refresh,
            "client_id": device.client_id(),
        });
        let response = client.post(&endpoints.token(), &headers, &body).await?;
        if !response.is_success() {
            tracing::warn!(status = response.status(), "token refresh rejected");
            return Err(Error::Expired);
        }
        Self::from_grant(&response)
    }

    pub(crate) fn from_grant(response: &Response) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Grant {
            access_token: String,
            refresh_token: String,
        }
        let grant: Grant = response.json()?;
        Ok(Self { access: grant.access_token, refresh: grant.refresh_token })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Introspection {
    identity: IdentityId,
    expires_in: Option<u64>,
}

impl Introspection {
    pub async fn fetch(
        client: &Client,
        endpoints: &Endpoints,
        tokens: &Tokens,
        device: &Device,
        session_id: &str,
    ) -> Result<Option<Self>, Error> {
        let headers = Headers::new()
            .with(CLIENT_HEADER.0, CLIENT_HEADER.1)
            .session(session_id)
            .device(device.device_id())
            .bearer(tokens.access());
        let response = client.get(&endpoints.token_info(), &headers).await?;
        if response.status() == 401 {
            return Ok(None);
        }
        if !response.is_success() {
            return Err(Error::Rejected { status: response.status(), message: message(&response) });
        }
        let body: Value = response.json()?;
        let identity = body
            .get("identity_canonical_id")
            .and_then(Value::as_str)
            .ok_or(Error::Rejected { status: response.status(), message: "token info has no identity".to_owned() })?;
        Ok(Some(Self {
            identity: IdentityId::parse(identity)?,
            expires_in: body.get("expires_in").and_then(Value::as_u64),
        }))
    }

    #[must_use]
    pub fn identity(&self) -> &IdentityId {
        &self.identity
    }

    #[must_use]
    pub fn expires_in(&self) -> Option<u64> {
        self.expires_in
    }
}

pub(crate) fn message(response: &Response) -> String {
    response
        .json::<Value>()
        .ok()
        .and_then(|v| {
            let text = v.get("error_description").or_else(|| v.get("error")).or_else(|| v.get("message"))?;
            text.as_str().map(str::to_owned)
        })
        .unwrap_or_else(|| format!("HTTP {}", response.status()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn device() -> Device {
        serde_json::from_value(json!({"device_id": "dev", "client_id": "cid"})).unwrap_or_else(|e| panic!("{e}"))
    }

    fn tokens(access: &str, refresh: &str) -> Tokens {
        Tokens { access: access.to_owned(), refresh: refresh.to_owned() }
    }

    #[test]
    fn debug_never_prints_tokens() {
        assert!(!format!("{:?}", tokens("secret-a", "secret-r")).contains("secret"));
    }

    #[tokio::test]
    async fn refresh_rotates_both_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(header("x-ws-profile", "invest"))
            .and(body_partial_json(json!({"grant_type": "refresh_token", "refresh_token": "r1", "client_id": "cid"})))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"access_token": "a2", "refresh_token": "r2"})),
            )
            .mount(&server)
            .await;

        let client = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        let fresh = tokens("a1", "r1").refresh(&client, &Endpoints::at(&server.uri()), &device(), "s").await;
        assert_eq!(fresh.ok(), Some(tokens("a2", "r2")));
    }

    #[tokio::test]
    async fn rejected_refresh_means_expired() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(401)).mount(&server).await;

        let client = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        let result = tokens("a1", "r1").refresh(&client, &Endpoints::at(&server.uri()), &device(), "s").await;
        assert!(matches!(result, Err(Error::Expired)));
    }

    #[tokio::test]
    async fn introspection_reads_identity_or_reports_expiry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("authorization", "Bearer good"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"identity_canonical_id": "identity-abc", "expires_in": 1700})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(401)).mount(&server).await;

        let client = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        let endpoints = Endpoints::at(&server.uri());
        let live = Introspection::fetch(&client, &endpoints, &tokens("good", "r"), &device(), "s").await;
        let live = live.unwrap_or_else(|e| panic!("{e}")).unwrap_or_else(|| panic!("expected identity"));
        assert_eq!((live.identity().as_str(), live.expires_in()), ("identity-abc", Some(1700)));

        let dead = Introspection::fetch(&client, &endpoints, &tokens("stale", "r"), &device(), "s").await;
        assert_eq!(dead.ok(), Some(None));
    }
}
