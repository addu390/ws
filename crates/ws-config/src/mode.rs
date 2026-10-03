use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Read,
    Paper,
    Trade,
}

impl Mode {
    #[must_use]
    pub fn trades(self) -> bool {
        matches!(self, Self::Paper | Self::Trade)
    }

    #[must_use]
    pub fn is_live(self) -> bool {
        self == Self::Trade
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Read => "read",
            Self::Paper => "paper",
            Self::Trade => "trade",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_trade_is_live() {
        assert!(!Mode::Read.trades());
        assert!(Mode::Paper.trades() && !Mode::Paper.is_live());
        assert!(Mode::Trade.trades() && Mode::Trade.is_live());
    }
}
