#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("audit log: {0}")]
    Open(#[from] ws_common::Error),
    #[error("audit log: {0}")]
    Io(#[from] std::io::Error),
    #[error("audit log: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("audit log is unusable after a crash while writing")]
    Poisoned,
}
