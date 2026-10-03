#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not build HTTP client: {0}")]
    Build(#[source] wreq::Error),
    #[error("invalid header {name}")]
    Header { name: String },
    #[error("request to {url} failed: {source}")]
    Transport {
        url: String,
        #[source]
        source: wreq::Error,
    },
    #[error("response from {url} (HTTP {status}) is not the expected JSON: {source}")]
    Decode {
        url: String,
        status: u16,
        #[source]
        source: serde_json::Error,
    },
}
