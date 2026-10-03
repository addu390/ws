#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Net(#[from] ws_net::Error),

    #[error(transparent)]
    Auth(#[from] ws_auth::Error),

    #[error("Wealthsimple rejected the session, run `ws-mcp login` again")]
    Unauthorized,

    #[error("{operation} failed with HTTP {status}")]
    Http { operation: &'static str, status: u16 },

    #[error("{operation} failed: {messages}")]
    Graphql { operation: &'static str, messages: String },

    #[error("{operation} returned an unexpected shape: {detail}")]
    Shape { operation: &'static str, detail: String },

    #[error("{0} not found")]
    NotFound(String),

    #[error("Wealthsimple rejected the order: {0}")]
    Rejected(String),

    #[error("not supported: {0}")]
    Unsupported(String),
}
