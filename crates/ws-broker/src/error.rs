#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not signed in to Wealthsimple. Run `ws-mcp login` in a terminal")]
    Unauthorized,

    #[error("{0} not found")]
    NotFound(String),

    #[error("order rejected: {0}")]
    Rejected(String),

    #[error(transparent)]
    Backend(Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    pub fn backend(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Backend(Box::new(error))
    }
}
