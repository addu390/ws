use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Scratch(PathBuf);

impl Scratch {
    #[must_use]
    pub fn new() -> Self {
        Self(std::env::temp_dir().join(format!("ws-test-{}", uuid::Uuid::new_v4())))
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.0
    }

    #[must_use]
    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
