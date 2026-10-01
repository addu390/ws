//! Password login with a 2FA step.

use std::fmt;

use serde_json::json;
use uuid::Uuid;
use ws_net::{Client, Headers};

use crate::token::{CLIENT_HEADER, message};
use crate::{Device, Endpoints, Error, Introspection, Scope, Session, Tokens};

#[derive(Clone)]
pub struct Credentials {
    username: String,
    password: String,
}

impl Credentials {
    #[must_use]
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self { username: username.into(), password: password.into() }
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials").field("username", &self.username).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct Login {
    client: Client,
    endpoints: Endpoints,
}

#[derive(Debug)]
pub enum Outcome {
    Done(Session),
    NeedsCode(Pending),
}

/// A login waiting for its 2FA code. Submitting a wrong code leaves it usable for another try.
#[derive(Debug)]
pub struct Pending {
    login: Login,
    device: Device,
    session_id: String,
    credentials: Credentials,
    scope: Scope,
}

enum Attempt {
    Granted(Tokens),
    NeedsCode,
}

impl Login {
    #[must_use]
    pub fn new(client: Client, endpoints: Endpoints) -> Self {
        Self { client, endpoints }
    }

    pub async fn start(self, credentials: Credentials, scope: Scope) -> Result<Outcome, Error> {
        let device = Device::bootstrap(&self.client, &self.endpoints).await?;
        let session_id = Uuid::new_v4().to_string();
        match self.attempt(&device, &session_id, &credentials, scope, None).await? {
            Attempt::Granted(tokens) => Ok(Outcome::Done(self.finish(device, session_id, tokens, scope).await?)),
            Attempt::NeedsCode => Ok(Outcome::NeedsCode(Pending { login: self, device, session_id, credentials, scope })),
        }
    }

    async fn attempt(
        &self,
        device: &Device,
        session_id: &str,
        credentials: &Credentials,
        scope: Scope,
        code: Option<&str>,
    ) -> Result<Attempt, Error> {
        let mut headers = Headers::new()
            .with(CLIENT_HEADER.0, CLIENT_HEADER.1)
            .with("x-ws-profile", "undefined")
            .session(session_id)
            .device(device.device_id());
        if let Some(code) = code {
            headers = headers.with("x-wealthsimple-otp", format!("{code};remember=true"));
        }
        let body = json!({
            "grant_type": "password",
            "username": credentials.username,
            "password": credentials.password,
            "skip_provision": "true",
            "scope": scope.oauth(),
            "client_id": device.client_id(),
            "otp_claim": null,
        });
        let response = self.client.post(&self.endpoints.token(), &headers, &body).await?;
        if response.is_success() {
            return Ok(Attempt::Granted(Tokens::from_grant(&response)?));
        }
        let invalid_grant = response.json::<serde_json::Value>().ok().and_then(|v| v.get("error").cloned())
            == Some(json!("invalid_grant"));
        if invalid_grant && code.is_none() {
            return Ok(Attempt::NeedsCode);
        }
        Err(Error::Rejected { status: response.status(), message: message(&response) })
    }

    async fn finish(&self, device: Device, session_id: String, tokens: Tokens, scope: Scope) -> Result<Session, Error> {
        let introspection = Introspection::fetch(&self.client, &self.endpoints, &tokens, &device, &session_id)
            .await?
            .ok_or(Error::Rejected { status: 401, message: "new token was not accepted".to_owned() })?;
        Ok(Session::new(device, session_id, tokens, introspection.identity().clone(), scope))
    }
}

impl Pending {
    pub async fn submit(&self, code: &str) -> Result<Session, Error> {
        let code = code.trim();
        let login = &self.login;
        match login.attempt(&self.device, &self.session_id, &self.credentials, self.scope, Some(code)).await? {
            Attempt::Granted(tokens) => {
                login.finish(self.device.clone(), self.session_id.clone(), tokens, self.scope).await
            }
            Attempt::NeedsCode => Err(Error::Rejected { status: 401, message: "2FA code not accepted".to_owned() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn mock_wealthsimple() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/app/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "wssdi=dev; Path=/")
                    .set_body_string(r#"<script src="/app-abc.js"></script>"#),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/app-abc.js"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#""production":{clientId:"c0ffee"}"#))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/oauth/token/info"))
            .and(header("authorization", "Bearer a1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"identity_canonical_id": "identity-me"})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(header("x-wealthsimple-otp", "123456;remember=true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token": "a1", "refresh_token": "r1"})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(header_exists("x-wealthsimple-otp"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "invalid_grant"})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(header("x-ws-device-id", "dev"))
            .and(body_partial_json(json!({"grant_type": "password", "client_id": "c0ffee", "scope": Scope::Read.oauth()})))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "invalid_grant"})))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn asks_for_a_code_and_allows_retrying_it() {
        let server = mock_wealthsimple().await;
        let client = Client::chrome().unwrap_or_else(|e| panic!("{e}"));
        let login = Login::new(client, Endpoints::at(&server.uri()));

        let outcome = login.start(Credentials::new("me@example.com", "pw"), Scope::Read).await;
        let Ok(Outcome::NeedsCode(pending)) = outcome else { panic!("expected a 2FA prompt, got {outcome:?}") };

        assert!(matches!(pending.submit("000000").await, Err(Error::Rejected { .. })));
        let session = pending.submit(" 123456 ").await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((session.identity().as_str(), session.scope()), ("identity-me", Scope::Read));
    }

    #[test]
    fn debug_hides_the_password() {
        assert!(!format!("{:?}", Credentials::new("me", "hunter2")).contains("hunter2"));
    }
}
