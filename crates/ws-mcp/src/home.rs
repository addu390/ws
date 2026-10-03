use std::path::PathBuf;

use anyhow::Context;

const DIR: &str = ".ws-mcp";

#[derive(Debug, Clone)]
pub struct Home(PathBuf);

impl Home {
    pub fn locate() -> anyhow::Result<Self> {
        let home = std::env::home_dir().filter(|dir| !dir.as_os_str().is_empty()).context("no home directory")?;
        Ok(Self(home.join(DIR)))
    }

    #[cfg(test)]
    pub fn at(root: &std::path::Path) -> Self {
        Self(root.to_path_buf())
    }

    pub fn config(&self) -> PathBuf {
        self.0.join("config.toml")
    }

    pub fn audit(&self) -> PathBuf {
        self.0.join("audit.jsonl")
    }

    pub fn kill(&self) -> PathBuf {
        self.0.join("KILL")
    }

    pub fn paper(&self) -> PathBuf {
        self.0.join("paper")
    }

    pub fn trade(&self) -> PathBuf {
        self.0.join("trade")
    }

    pub fn ledger(&self) -> PathBuf {
        self.paper().join("ledger.json")
    }
}
