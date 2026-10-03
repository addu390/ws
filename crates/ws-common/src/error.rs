use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path} is corrupt: {source}")]
    Corrupt { path: PathBuf, source: serde_json::Error },
}
