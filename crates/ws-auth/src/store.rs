use std::path::PathBuf;

use crate::{Error, Session};

pub trait Store: Send + Sync {
    fn load(&self) -> Result<Option<Session>, Error>;
    fn save(&self, session: &Session) -> Result<(), Error>;
    fn clear(&self) -> Result<(), Error>;
}

#[derive(Debug, Clone)]
pub struct Keyring {
    service: String,
    account: String,
}

impl Keyring {
    #[must_use]
    pub fn new(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self { service: service.into(), account: account.into() }
    }

    fn entry(&self) -> Result<keyring::Entry, Error> {
        keyring::Entry::new(&self.service, &self.account).map_err(store)
    }
}

impl Default for Keyring {
    fn default() -> Self {
        Self::new("ws-mcp", "session")
    }
}

impl Store for Keyring {
    fn load(&self) -> Result<Option<Session>, Error> {
        match self.entry()?.get_password() {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(store(e)),
        }
    }

    fn save(&self, session: &Session) -> Result<(), Error> {
        self.entry()?.set_password(&serde_json::to_string(session)?).map_err(store)
    }

    fn clear(&self) -> Result<(), Error> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(store(e)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct File {
    path: PathBuf,
}

impl File {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Store for File {
    fn load(&self) -> Result<Option<Session>, Error> {
        ws_common::read_json(&self.path).map_err(store)
    }

    fn save(&self, session: &Session) -> Result<(), Error> {
        ws_common::write_json(&self.path, session).map_err(store)
    }

    fn clear(&self) -> Result<(), Error> {
        ws_common::remove(&self.path).map(drop).map_err(store)
    }
}

#[allow(clippy::needless_pass_by_value)]
fn store(error: impl std::fmt::Display) -> Error {
    Error::Store(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session() -> Session {
        serde_json::from_value(json!({
            "device": {"device_id": "dev", "client_id": "cid"},
            "id": "sid",
            "tokens": {"access": "a", "refresh": "r"},
            "identity": "identity-abc",
            "scope": "write",
        }))
        .unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn file_store_round_trips() {
        let scratch = ws_common::fixtures::Scratch::new();
        let file = File::new(scratch.path("session.json"));
        assert!(matches!(file.load(), Ok(None)));

        file.save(&session()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(file.load().ok().flatten(), Some(session()));

        file.clear().unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(file.load(), Ok(None)));
    }
}
