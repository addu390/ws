//! Authentication errors.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Net(#[from] ws_net::Error),
    #[error("login page bootstrap failed: {0}")]
    Bootstrap(&'static str),
    #[error("login rejected (HTTP {status}): {message}")]
    Rejected { status: u16, message: String },
    #[error("browser login: {0}")]
    Browser(String),
    #[error("session expired and could not be refreshed; run `ws-mcp login` again")]
    Expired,
    #[error("unexpected identity from Wealthsimple: {0}")]
    Identity(#[from] ws_core::Error),
    #[error("credential store: {0}")]
    Store(String),
    #[error("stored session is corrupt: {0}")]
    Corrupt(#[from] serde_json::Error),
}
