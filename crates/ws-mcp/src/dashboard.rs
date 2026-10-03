//! Listens only on loopback, answers only its own `Host` and `Origin` (against DNS rebinding), and needs a
//! per-launch secret on every API call. The secret reaches the tab in the URL fragment and is never printed or saved.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::{Context as _, ensure};
use axum::Router;
use axum::extract::{Path, Query, Request, State};
use axum::http::header::{self, HeaderName};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use ws_broker::Reader;
use ws_config::Mode;
use ws_core::IdempotencyKey;
use ws_policy::{Approver, Denial, Guard, Kill, TicketId};

use crate::action::{Action, Failure, perform};
use crate::control;
use crate::home::Home;
use crate::read::{Accounts, PendingOrders, Positions};
use crate::trade::{CancelOrder, LimitsStatus};
use crate::wiring::Wiring;

const PAGE: &str = include_str!("../assets/dashboard.html");
const SCRIPT: &str = include_str!("../assets/dashboard.js");
const STYLE: &str = include_str!("../assets/dashboard.css");

const SECRET: &str = "x-ws-secret";

const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; \
    base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const HARDENING: [(HeaderName, &str); 5] = [
    (header::CONTENT_SECURITY_POLICY, POLICY),
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::X_FRAME_OPTIONS, "DENY"),
    (header::REFERRER_POLICY, "no-referrer"),
    (header::CACHE_CONTROL, "no-store"),
];

const AUDIT_DEFAULT: usize = 50;
const AUDIT_MAX: usize = 500;

pub async fn run() -> anyhow::Result<()> {
    let home = Home::locate()?;
    let Wiring { reader, guard } = Wiring::build(&home)?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.context("listening on 127.0.0.1")?;
    let address = listener.local_addr()?;
    let secret = Secret::fresh();
    open(&format!("http://{address}/#{}", secret.0))?;
    eprintln!("Dashboard opened in your browser at http://{address}. Press Ctrl-C to stop it.");
    let app = router(Arc::new(Dashboard { reader, guard, home, secret, local: Local(address) }));
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

struct Dashboard {
    reader: Arc<dyn Reader>,
    guard: Option<Arc<Guard>>,
    home: Home,
    secret: Secret,
    local: Local,
}

impl Dashboard {
    fn guard(&self) -> Result<&Guard, Problem> {
        self.guard.as_deref().ok_or_else(|| Failure::Policy(Denial::ReadOnly.into()).into())
    }
}

struct Secret(String);

impl Secret {
    fn fresh() -> Self {
        Self(format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()))
    }

    /// Constant time, so timing does not reveal the secret.
    fn matches(&self, presented: &str) -> bool {
        let (expected, presented) = (self.0.as_bytes(), presented.as_bytes());
        expected.len() == presented.len() && expected.iter().zip(presented).fold(0, |diff, (a, b)| diff | (a ^ b)) == 0
    }
}

struct Local(SocketAddr);

impl Local {
    fn is_host(&self, host: &str) -> bool {
        let port = self.0.port();
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    }

    fn is_origin(&self, origin: &str) -> bool {
        origin.strip_prefix("http://").is_some_and(|host| self.is_host(host))
    }
}

fn router(dashboard: Arc<Dashboard>) -> Router {
    let api = Router::new()
        .route("/overview", get(overview))
        .route("/accounts", get(accounts))
        .route("/accounts/{account}/positions", get(positions))
        .route("/accounts/{account}/orders", get(orders))
        .route("/orders/{key}/cancel", post(cancel))
        .route("/approvals", get(approvals))
        .route("/approvals/{ticket}", post(approve))
        .route("/kill", post(kill))
        .route("/resume", post(resume))
        .route("/audit", get(audit))
        .route_layer(middleware::from_fn_with_state(dashboard.clone(), authorize));
    Router::new()
        .route("/", get(|| async { Html(PAGE) }))
        .route("/dashboard.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], SCRIPT) }))
        .route("/dashboard.css", get(|| async { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLE) }))
        .nest("/api", api)
        .layer(middleware::from_fn_with_state(dashboard.clone(), same_host))
        .layer(middleware::map_response(harden))
        .with_state(dashboard)
}

async fn same_host(State(dashboard): State<Arc<Dashboard>>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).is_some_and(|h| dashboard.local.is_host(h));
    let origin = headers.get(header::ORIGIN).is_none_or(|v| v.to_str().is_ok_and(|o| dashboard.local.is_origin(o)));
    if host && origin {
        next.run(request).await
    } else {
        Problem(StatusCode::FORBIDDEN, "the dashboard only answers its own page".into()).into_response()
    }
}

async fn authorize(State(dashboard): State<Arc<Dashboard>>, request: Request, next: Next) -> Response {
    let secret = request.headers().get(SECRET).and_then(|v| v.to_str().ok());
    if secret.is_some_and(|s| dashboard.secret.matches(s)) {
        next.run(request).await
    } else {
        let message = "this page belongs to an earlier launch. Run `ws-mcp dashboard` again";
        Problem(StatusCode::UNAUTHORIZED, message.into()).into_response()
    }
}

async fn harden(mut response: Response) -> Response {
    let headers = response.headers_mut();
    for (name, value) in HARDENING {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

async fn overview(State(dashboard): State<Arc<Dashboard>>) -> Result<Response, Problem> {
    match &dashboard.guard {
        Some(guard) => reply(LimitsStatus {}, guard).await,
        None => encode(&json!({ "mode": Mode::Read, "killed": Kill::new(dashboard.home.kill()).engaged() })),
    }
}

async fn accounts(State(dashboard): State<Arc<Dashboard>>) -> Result<Response, Problem> {
    reply(Accounts {}, dashboard.reader.as_ref()).await
}

async fn positions(State(dashboard): State<Arc<Dashboard>>, Path(account): Path<String>) -> Result<Response, Problem> {
    reply(Positions { account }, dashboard.reader.as_ref()).await
}

async fn orders(State(dashboard): State<Arc<Dashboard>>, Path(account): Path<String>) -> Result<Response, Problem> {
    reply(PendingOrders { account }, dashboard.reader.as_ref()).await
}

async fn cancel(State(dashboard): State<Arc<Dashboard>>, Path(key): Path<IdempotencyKey>) -> Result<Response, Problem> {
    reply(CancelOrder { key }, dashboard.guard()?).await
}

async fn approvals(State(dashboard): State<Arc<Dashboard>>) -> Result<Response, Problem> {
    let tickets = match &dashboard.guard {
        Some(guard) => guard.awaiting_approval()?,
        None => Vec::new(),
    };
    encode(&tickets)
}

async fn approve(State(dashboard): State<Arc<Dashboard>>, Path(ticket): Path<TicketId>) -> Result<Response, Problem> {
    encode(&dashboard.guard()?.approve(&ticket, Approver::Dashboard)?)
}

async fn kill(State(dashboard): State<Arc<Dashboard>>) -> Result<Response, Problem> {
    control::halt(&dashboard.home)?;
    overview(State(dashboard)).await
}

async fn resume(State(dashboard): State<Arc<Dashboard>>) -> Result<Response, Problem> {
    control::lift(&dashboard.home)?;
    overview(State(dashboard)).await
}

#[derive(Deserialize)]
struct Recent {
    limit: Option<usize>,
}

async fn audit(State(dashboard): State<Arc<Dashboard>>, Query(recent): Query<Recent>) -> Result<Response, Problem> {
    let limit = recent.limit.unwrap_or(AUDIT_DEFAULT).clamp(1, AUDIT_MAX);
    let entries = ws_audit::File::recent(&dashboard.home.audit(), limit).map_err(anyhow::Error::from)?;
    encode(&entries)
}

async fn reply<A: Action>(action: A, context: &A::Context) -> Result<Response, Problem> {
    Ok(json(perform(action, context).await?))
}

fn encode<T: Serialize + ?Sized>(value: &T) -> Result<Response, Problem> {
    Ok(json(serde_json::to_string_pretty(value).map_err(Failure::from)?))
}

fn json(body: String) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

#[derive(Debug)]
struct Problem(StatusCode, String);

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        (self.0, axum::Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<Failure> for Problem {
    fn from(failure: Failure) -> Self {
        let status = match &failure {
            Failure::Broker(e) | Failure::Policy(ws_policy::Error::Broker(e)) => broker_status(e),
            Failure::Policy(ws_policy::Error::Denied(_)) => StatusCode::FORBIDDEN,
            Failure::Domain(_)
            | Failure::Invalid(_)
            | Failure::Unresolved(_)
            | Failure::Policy(ws_policy::Error::Domain(_)) => StatusCode::BAD_REQUEST,
            Failure::Policy(_) | Failure::Encode(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, failure.to_string())
    }
}

impl From<ws_policy::Error> for Problem {
    fn from(error: ws_policy::Error) -> Self {
        Failure::Policy(error).into()
    }
}

impl From<anyhow::Error> for Problem {
    fn from(error: anyhow::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}"))
    }
}

fn broker_status(error: &ws_broker::Error) -> StatusCode {
    match error {
        ws_broker::Error::NotFound(_) => StatusCode::NOT_FOUND,
        ws_broker::Error::Rejected(_) => StatusCode::UNPROCESSABLE_ENTITY,
        ws_broker::Error::Unauthorized | ws_broker::Error::Backend(_) => StatusCode::BAD_GATEWAY,
    }
}

fn open(url: &str) -> anyhow::Result<()> {
    let status = opener()
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("opening your browser")?;
    ensure!(status.success(), "could not open your browser ({status})");
    Ok(())
}

#[cfg(target_os = "macos")]
fn opener() -> std::process::Command {
    std::process::Command::new("open")
}

#[cfg(target_os = "windows")]
fn opener() -> std::process::Command {
    let mut command = std::process::Command::new("rundll32");
    command.arg("url.dll,FileProtocolHandler");
    command
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn opener() -> std::process::Command {
    std::process::Command::new("xdg-open")
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Method;
    use rust_decimal::dec;
    use tower::ServiceExt;
    use ws_common::fixtures::Scratch;
    use ws_core::fixtures;

    use super::*;
    use crate::fake;
    use crate::trade::{PreviewOrder, Side};

    const PORT: u16 = 4000;

    fn dashboard(home: &Home, paper: bool) -> (Router, Arc<Guard>) {
        let (reader, guard) = fake::paper(home);
        let secret = Secret("s".repeat(64));
        let local = Local(SocketAddr::from((Ipv4Addr::LOCALHOST, PORT)));
        let dashboard = Dashboard { reader, guard: paper.then(|| guard.clone()), home: home.clone(), secret, local };
        (router(Arc::new(dashboard)), guard)
    }

    fn from_page(method: Method, path: &str) -> Request {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, format!("127.0.0.1:{PORT}"))
            .header(header::ORIGIN, format!("http://127.0.0.1:{PORT}"))
            .header(SECRET, "s".repeat(64))
            .body(Body::empty());
        request.unwrap_or_else(|e| panic!("{e}"))
    }

    fn with(mut request: Request, name: HeaderName, value: &str) -> Request {
        let value = HeaderValue::from_str(value).unwrap_or_else(|e| panic!("{e}"));
        request.headers_mut().insert(name, value);
        request
    }

    fn without(mut request: Request, name: &str) -> Request {
        request.headers_mut().remove(name);
        request
    }

    async fn respond(app: &Router, request: Request) -> Response {
        app.clone().oneshot(request).await.unwrap_or_else(|e| match e {})
    }

    async fn send(app: &Router, request: Request) -> (StatusCode, serde_json::Value) {
        let response = respond(app, request).await;
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap_or_else(|e| panic!("{e}"));
        (status, serde_json::from_slice(&body).unwrap_or_default())
    }

    #[tokio::test]
    async fn serves_the_page_under_a_strict_policy() {
        let scratch = Scratch::new();
        let (app, _) = dashboard(&Home::at(scratch.dir()), true);
        let response = respond(&app, without(without(from_page(Method::GET, "/"), SECRET), "origin")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(header::CONTENT_SECURITY_POLICY).and_then(|v| v.to_str().ok()), Some(POLICY));
        assert_eq!(response.headers().get(header::CACHE_CONTROL).and_then(|v| v.to_str().ok()), Some("no-store"));
    }

    #[tokio::test]
    async fn api_calls_need_the_launch_secret() {
        let scratch = Scratch::new();
        let (app, _) = dashboard(&Home::at(scratch.dir()), true);
        let missing = without(from_page(Method::GET, "/api/accounts"), SECRET);
        assert_eq!(send(&app, missing).await.0, StatusCode::UNAUTHORIZED);
        let wrong = with(from_page(Method::GET, "/api/accounts"), HeaderName::from_static(SECRET), &"t".repeat(64));
        assert_eq!(send(&app, wrong).await.0, StatusCode::UNAUTHORIZED);

        let (status, accounts) = send(&app, from_page(Method::GET, "/api/accounts")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(accounts[0]["nickname"], "Main");
    }

    #[tokio::test]
    async fn answers_only_its_own_address() {
        let scratch = Scratch::new();
        let (app, _) = dashboard(&Home::at(scratch.dir()), true);
        let rebound = with(from_page(Method::GET, "/api/overview"), header::HOST, &format!("evil.example:{PORT}"));
        assert_eq!(send(&app, rebound).await.0, StatusCode::FORBIDDEN);
        let foreign = with(from_page(Method::POST, "/api/kill"), header::ORIGIN, "https://evil.example");
        assert_eq!(send(&app, foreign).await.0, StatusCode::FORBIDDEN);
        assert!(!Home::at(scratch.dir()).kill().exists());

        let named = with(from_page(Method::GET, "/api/overview"), header::HOST, &format!("localhost:{PORT}"));
        let named = with(named, header::ORIGIN, &format!("http://localhost:{PORT}"));
        assert_eq!(send(&app, named).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn approves_tickets_and_flips_the_kill_switch() {
        let scratch = Scratch::new();
        let (app, guard) = dashboard(&Home::at(scratch.dir()), true);
        let preview = PreviewOrder {
            account: fixtures::account().to_string(),
            security: fixtures::security().to_string(),
            side: Side::Buy,
            shares: Some(dec!(10)),
            amount: None,
            limit: Some(dec!(40)),
            stop: None,
            good_till_cancelled: false,
        };
        let ticket = preview.run(&guard).await.unwrap_or_else(|e| panic!("{e}"));

        let (_, waiting) = send(&app, from_page(Method::GET, "/api/approvals")).await;
        assert_eq!(waiting[0]["id"], ticket.id().as_str());
        let (status, approved) = send(&app, from_page(Method::POST, &format!("/api/approvals/{}", ticket.id()))).await;
        assert_eq!((status, approved["approval"].as_str()), (StatusCode::OK, Some("granted")));
        assert_eq!(send(&app, from_page(Method::GET, "/api/approvals")).await.1, json!([]));

        let (_, killed) = send(&app, from_page(Method::POST, "/api/kill")).await;
        assert_eq!(killed["killed"], true);
        let (status, _) = send(&app, from_page(Method::POST, &format!("/api/orders/{}/cancel", ticket.key()))).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (_, resumed) = send(&app, from_page(Method::POST, "/api/resume")).await;
        assert_eq!(resumed["killed"], false);

        let (_, entries) = send(&app, from_page(Method::GET, "/api/audit?limit=1")).await;
        assert_eq!(entries.as_array().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn read_mode_shows_but_cannot_act() {
        let scratch = Scratch::new();
        let (app, _) = dashboard(&Home::at(scratch.dir()), false);
        let (_, overview) = send(&app, from_page(Method::GET, "/api/overview")).await;
        assert_eq!(overview, json!({ "mode": "read", "killed": false }));
        assert_eq!(send(&app, from_page(Method::GET, "/api/approvals")).await.1, json!([]));
        let (status, _) = send(&app, from_page(Method::POST, "/api/approvals/ticket-1")).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
}
