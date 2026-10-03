use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use wreq::{Method, StatusCode};
use wreq_util::Emulation;

use crate::{Error, Headers};

const TIMEOUT: Duration = Duration::from_secs(20);
const RETRY_DELAY: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct Client {
    inner: wreq::Client,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

impl Client {
    pub fn chrome() -> Result<Self, Error> {
        let inner = wreq::Client::builder()
            .emulation(Emulation::Chrome149)
            .cookie_store(true)
            .timeout(TIMEOUT)
            .build()
            .map_err(Error::Build)?;
        Ok(Self { inner })
    }

    pub async fn get(&self, url: &str, headers: &Headers) -> Result<Response, Error> {
        self.send(Method::GET, url, headers, None::<&()>).await
    }

    pub async fn post(&self, url: &str, headers: &Headers, body: &impl Serialize) -> Result<Response, Error> {
        self.send(Method::POST, url, headers, Some(body)).await
    }

    async fn send<B: Serialize + ?Sized>(
        &self,
        method: Method,
        url: &str,
        headers: &Headers,
        body: Option<&B>,
    ) -> Result<Response, Error> {
        let map = headers.to_map()?;
        let mut attempt = 0;
        loop {
            let mut request = self.inner.request(method.clone(), url).headers(map.clone());
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request
                .send()
                .await
                .map_err(|source| Error::Transport { url: url.to_owned(), source })?;
            let status = response.status();
            if attempt == 0 && matches!(status, StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE) {
                attempt += 1;
                tracing::warn!(%url, %status, "retrying once");
                tokio::time::sleep(RETRY_DELAY).await;
                continue;
            }
            return Response::read(url, response).await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Response {
    url: String,
    status: u16,
    cookies: Vec<(String, String)>,
    body: String,
}

impl Response {
    async fn read(url: &str, response: wreq::Response) -> Result<Self, Error> {
        let status = response.status().as_u16();
        let cookies = response.cookies().map(|c| (c.name().to_owned(), c.value().to_owned())).collect();
        let body = response.text().await.map_err(|source| Error::Transport { url: url.to_owned(), source })?;
        Ok(Self { url: url.to_owned(), status, cookies, body })
    }

    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.iter().rev().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.body
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, Error> {
        serde_json::from_str(&self.body).map_err(|source| Error::Decode {
            url: self.url.clone(),
            status: self.status,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client() -> Client {
        Client::chrome().unwrap_or_else(|e| panic!("{e}"))
    }

    #[tokio::test]
    async fn posts_json_with_headers_and_reads_cookies() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(header("x-ws-session-id", "s-1"))
            .and(body_json(json!({"grant_type": "password"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("set-cookie", "wssdi=abc123; Path=/")
                    .set_body_json(json!({"ok": true})),
            )
            .mount(&server)
            .await;

        let response = client()
            .post(&format!("{}/token", server.uri()), &Headers::new().session("s-1"), &json!({"grant_type": "password"}))
            .await
            .unwrap_or_else(|e| panic!("{e}"));

        assert!(response.is_success());
        assert_eq!(response.cookie("wssdi"), Some("abc123"));
        assert_eq!(response.json::<Value>().ok(), Some(json!({"ok": true})));
    }

    #[tokio::test]
    async fn retries_once_on_429() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200)).mount(&server).await;

        let response = client().get(&server.uri(), &Headers::new()).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn returns_non_success_responses_to_the_caller() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(401).set_body_string("nope")).mount(&server).await;

        let response = client().get(&server.uri(), &Headers::new()).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((response.status(), response.text()), (401, "nope"));
        assert!(matches!(response.json::<Value>(), Err(Error::Decode { status: 401, .. })));
    }
}
