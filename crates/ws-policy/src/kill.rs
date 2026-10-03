use std::ffi::OsStr;
use std::path::PathBuf;

const ENV: &str = "WS_KILL";

/// Engaged while the file exists or `WS_KILL=1` is set. Checked on every write, never cached.
#[derive(Debug, Clone)]
pub struct Kill {
    file: PathBuf,
}

impl Kill {
    #[must_use]
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into() }
    }

    #[must_use]
    pub fn engaged(&self) -> bool {
        engaged(std::env::var_os(ENV).as_deref(), self.file.exists())
    }
}

fn engaged(env: Option<&OsStr>, file_exists: bool) -> bool {
    file_exists || env.is_some_and(|value| value == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_or_env_engages() {
        assert!(!engaged(None, false));
        assert!(!engaged(Some(OsStr::new("0")), false));
        assert!(engaged(Some(OsStr::new("1")), false));
        assert!(engaged(None, true));
    }

    #[test]
    fn notices_the_file_appearing() {
        let scratch = ws_common::fixtures::Scratch::new();
        let kill = Kill::new(scratch.path("KILL"));
        assert!(!engaged(None, kill.file.exists()));
        ws_common::write(&scratch.path("KILL"), b"").unwrap_or_else(|e| panic!("{e}"));
        assert!(kill.engaged());
    }
}
