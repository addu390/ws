use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    File(#[from] ws_common::Error),
    #[error("{0} is not valid UTF-8")]
    Encoding(PathBuf),
    #[error("invalid config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("could not write config: {0}")]
    Encode(#[from] toml::ser::Error),
    #[error("unknown setting {key}. Settings are {known}")]
    UnknownSetting { key: String, known: String },
    #[error("invalid config: {field} {reason}")]
    Invalid { field: &'static str, reason: &'static str },
    #[error("invalid config: {0}")]
    Domain(#[from] ws_core::Error),
}
