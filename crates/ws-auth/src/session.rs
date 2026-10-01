//! An authenticated session: device, session id, tokens, identity, and granted scope.

use serde::{Deserialize, Serialize};
use ws_core::IdentityId;
use ws_net::{Client, Headers};

use crate::{Device, Endpoints, Error, Introspection, Tokens};

/// OAuth scope requested at login. A `Read` session cannot place orders even if asked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Read,
    Write,
}

impl Scope {
    #[must_use]
    pub fn oauth(self) -> &'static str {
        match self {
            Self::Read => "invest.read trade.read tax.read",
            Self::Write => "invest.read trade.read tax.read invest.write trade.write tax.write",
        }
    }

    /// Classifies a scope string granted by Wealthsimple, such as `read write` from the web app.
    #[must_use]
    pub fn granted(scope: &str) -> Self {
        let writes = |s: &str| s == "write" || s.rsplit_once('.').is_some_and(|(_, action)| action == "write");
        if scope.split_whitespace().any(writes) { Self::Write } else { Self::Read }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    device: Device,
    id: String,
    tokens: Tokens,
    identity: IdentityId,
    scope: Scope,
}

impl Session {
    pub(crate) fn new(device: Device, id: String, tokens: Tokens, identity: IdentityId, scope: Scope) -> Self {
        Self { device, id, tokens, identity, scope }
    }

    #[must_use]
    pub fn identity(&self) -> &IdentityId {
        &self.identity
    }

    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Session, device, and bearer headers for authenticated requests.
    #[must_use]
    pub fn headers(&self) -> Headers {
        Headers::new().session(&self.id).device(self.device.device_id()).bearer(self.tokens.access())
    }

    /// Swaps in fresh tokens. Callers must persist the session afterwards.
    pub async fn refresh(&mut self, client: &Client, endpoints: &Endpoints) -> Result<(), Error> {
        self.tokens = self.tokens.refresh(client, endpoints, &self.device, &self.id).await?;
        Ok(())
    }

    /// Verifies the access token, refreshing it if expired. Returns whether it was refreshed.
    pub async fn ensure_fresh(&mut self, client: &Client, endpoints: &Endpoints) -> Result<bool, Error> {
        if self.introspect(client, endpoints).await?.is_some() {
            return Ok(false);
        }
        self.refresh(client, endpoints).await?;
        self.introspect(client, endpoints).await?.ok_or(Error::Expired)?;
        Ok(true)
    }

    pub async fn introspect(&self, client: &Client, endpoints: &Endpoints) -> Result<Option<Introspection>, Error> {
        Introspection::fetch(client, endpoints, &self.tokens, &self.device, &self.id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn session(access: &str) -> Session {
        serde_json::from_value(json!({
            "device": {"device_id": "dev", "client_id": "cid"},
            "id": "sid",
            "tokens": {"access": access, "refresh": "r1"},
            "identity": "identity-abc",
            "scope": "read",
        }))
        .unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn write_scope_is_a_superset_of_read() {
        assert!(Scope::Write.oauth().starts_with(Scope::Read.oauth()));
        assert!(!Scope::Read.oauth().contains("write"));
    }

    #[test]
    fn classifies_granted_scopes() {
        assert_eq!(Scope::granted("read write"), Scope::Write);
        assert_eq!(Scope::granted(Scope::Write.oauth()), Scope::Write);
        assert_eq!(Scope::granted(Scope::Read.oauth()), Scope::Read);
        assert_eq!(Scope::granted("read"), Scope::Read);
    }

    #[tokio::test]
    async fn refreshes_only_when_the_token_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/oauth/token/info"))
            .and(header("authorization", "Bearer fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"identity_canonical_id": "identity-abc"})))
            .mount(&server)
            .await;
        Mock::given(method("GET")).and(path("/oauth/token/info")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "fresh", "refresh_token": "r2"})))
            .expect(1)
            .mount(&server)
            .await;

        let client = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        let endpoints = Endpoints::at(&server.uri());

        let mut current = session("fresh");
        assert_eq!(current.ensure_fresh(&client, &endpoints).await.ok(), Some(false));

        let mut stale = session("stale");
        assert_eq!(stale.ensure_fresh(&client, &endpoints).await.ok(), Some(true));
        assert_eq!(stale, session("fresh").with_refresh("r2"));
    }

    impl Session {
        fn with_refresh(mut self, refresh: &str) -> Self {
            self.tokens = serde_json::from_value(json!({"access": self.tokens.access(), "refresh": refresh}))
                .unwrap_or_else(|e| panic!("{e}"));
            self
        }
    }
}
