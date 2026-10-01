//! Session storage backends: the system keyring, and a file store for tests and headless use.

use std::fs;
use std::io::Write;
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

/// Plain JSON on disk, readable only by the owner.
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
        match fs::read_to_string(&self.path) {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(store(e)),
        }
    }

    fn save(&self, session: &Session) -> Result<(), Error> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(store)?;
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&self.path).map_err(store)?;
        file.write_all(serde_json::to_string(session)?.as_bytes()).map_err(store)
    }

    fn clear(&self) -> Result<(), Error> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(store(e)),
        }
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
    fn file_store_round_trips_privately() {
        let path = std::env::temp_dir().join(format!("ws-auth-{}/session.json", uuid::Uuid::new_v4()));
        let file = File::new(&path);
        assert!(matches!(file.load(), Ok(None)));

        file.save(&session()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(file.load().ok().flatten(), Some(session()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).map(|m| m.permissions().mode() & 0o777).ok();
            assert_eq!(mode, Some(0o600));
        }

        file.clear().unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(file.load(), Ok(None)));
        let _ = fs::remove_dir(path.parent().unwrap_or(&path));
    }
}
