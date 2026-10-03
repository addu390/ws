use std::path::Path;

use rust_decimal::{Decimal, dec};
use serde::Deserialize;
use ws_core::Money;

use crate::limits::{self, Limits};
use crate::{Error, Mode, setting};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    mode: Mode,
    limits: Limits,
    paper_cash: Money,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let bytes = ws_common::read(path)?.unwrap_or_default();
        Self::parse(std::str::from_utf8(&bytes).map_err(|_| Error::Encoding(path.to_owned()))?)
    }

    pub fn parse(text: &str) -> Result<Self, Error> {
        let raw: Raw = toml::from_str(text)?;
        Ok(Self {
            mode: raw.mode,
            limits: raw.limits.try_into()?,
            paper_cash: limits::positive("paper.starting_cash", raw.paper.starting_cash)?,
        })
    }

    /// Every setting, including defaults, with each `(key, value)` applied.
    pub fn set(text: &str, changes: &[(&str, &str)]) -> Result<String, Error> {
        let mut table = setting::complete(text)?;
        for (key, value) in changes {
            setting::assign(&mut table, key, value)?;
        }
        Self::render(&table)
    }

    /// Every setting as it applies, including defaults.
    pub fn effective(text: &str) -> Result<String, Error> {
        Self::render(&setting::complete(text)?)
    }

    fn render(table: &toml::Table) -> Result<String, Error> {
        let text = toml::to_string(table)?;
        Self::parse(&text)?;
        Ok(text)
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    #[must_use]
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    #[must_use]
    pub fn paper_cash(&self) -> Money {
        self.paper_cash
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Raw {
    mode: Mode,
    limits: limits::Raw,
    paper: RawPaper,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawPaper {
    starting_cash: Decimal,
}

impl Default for RawPaper {
    fn default() -> Self {
        Self { starting_cash: dec!(10000) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ws_core::{AccountId, SecurityId};

    fn parse(text: &str) -> Config {
        Config::parse(text).unwrap_or_else(|e| panic!("{e}"))
    }

    fn rejected(text: &str) -> String {
        Config::parse(text).err().map(|e| e.to_string()).unwrap_or_default()
    }

    #[test]
    fn defaults_are_read_only_with_no_tradable_accounts() {
        let config = parse("");
        assert_eq!(config.mode(), Mode::Read);
        assert!(!config.limits().allows(&AccountId::parse("tfsa-abc").unwrap_or_else(|e| panic!("{e}"))));
        assert!(config.limits().limit_only());
        assert_eq!(config.limits().max_order_value().amount(), dec!(500));
    }

    #[test]
    fn shipped_example_matches_defaults() {
        assert_eq!(parse(include_str!("../../../config.toml")), parse(""));
    }

    #[test]
    fn missing_file_uses_defaults() {
        let scratch = ws_common::fixtures::Scratch::new();
        assert_eq!(Config::load(&scratch.path("config.toml")).ok(), Some(parse("")));
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let config = parse(
            "mode = \"paper\"\n[limits]\nallowed_accounts = [\"tfsa-abc\"]\nblocked_securities = [\"sec-s-x\"]\n",
        );
        assert_eq!(config.mode(), Mode::Paper);
        assert!(config.limits().allows(&AccountId::parse("tfsa-abc").unwrap_or_else(|e| panic!("{e}"))));
        assert!(config.limits().blocks(&SecurityId::parse("sec-s-x").unwrap_or_else(|e| panic!("{e}"))));
        assert_eq!(config.limits().max_orders_per_day(), 5);
    }

    #[test]
    fn setting_several_values_at_once() {
        let tfsa = AccountId::parse("tfsa-abc").unwrap_or_else(|e| panic!("{e}"));
        let text = "mode = \"read\"\n[limits]\nmax_orders_per_day = 2\nallowed_accounts = [\"rrsp-old\"]\n";
        let changes = [("mode", "trade"), ("allowed_accounts", "tfsa-abc")];
        let chosen = parse(&Config::set(text, &changes).unwrap_or_else(|e| panic!("{e}")));
        assert_eq!(chosen.mode(), Mode::Trade);
        assert!(chosen.limits().allows(&tfsa));
        assert!(!chosen.limits().allows(&AccountId::parse("rrsp-old").unwrap_or_else(|e| panic!("{e}"))));
        assert_eq!(chosen.limits().max_orders_per_day(), 2);
        assert!(Config::set(text, &[("mode", "trade"), ("max_orders_per_day", "0")]).is_err());
    }

    #[test]
    fn setting_one_value_keeps_the_rest_and_refuses_bad_ones() {
        let text = "mode = \"trade\"\n[limits]\nallowed_accounts = [\"tfsa-abc\"]\n";
        let set = |key: &str, value: &str| Config::set(text, &[(key, value)]).map(|t| parse(&t));
        let changed = set("market_hours_only", "false").unwrap_or_else(|e| panic!("{e}"));
        assert!(!changed.limits().market_hours_only());
        assert_eq!(changed.mode(), Mode::Trade);
        assert!(changed.limits().allows(&AccountId::parse("tfsa-abc").unwrap_or_else(|e| panic!("{e}"))));
        assert_eq!(set("max_order_value", "300").map(|c| c.limits().max_order_value().amount()).ok(), Some(dec!(300)));
        assert_eq!(set("max_orders_per_day", "2").map(|c| c.limits().max_orders_per_day()).ok(), Some(2));
        assert_eq!(set("blocked_securities", "sec-s-a, sec-s-b").map(|c| c.limits().blocks(&SecurityId::parse("sec-s-b").unwrap_or_else(|e| panic!("{e}")))).ok(), Some(true));

        let refused = |key: &str, value: &str| set(key, value).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(refused("market_hours", "false").contains("unknown setting market_hours"));
        assert!(refused("limit_only", "no").contains("true or false"));
        assert!(refused("mode", "yolo").contains("unknown variant"));
        assert!(refused("max_order_value", "5000").contains("must not exceed max_daily_spend"));
        assert_eq!(parse(&Config::effective("").unwrap_or_else(|e| panic!("{e}"))), parse(""));
    }

    #[test]
    fn rejects_typos_and_unknown_modes() {
        assert!(rejected("[limits]\nmax_order_valu = \"1\"\n").contains("unknown field"));
        assert!(rejected("mode = \"yolo\"\n").contains("unknown variant"));
    }

    #[test]
    fn rejects_unsafe_values() {
        assert!(rejected("[limits]\nmax_order_value = \"0\"\n").contains("max_order_value must be positive"));
        assert!(rejected("[limits]\nmax_order_value = \"5000\"\n").contains("must not exceed max_daily_spend"));
        assert!(rejected("[limits]\nmax_orders_per_day = 0\n").contains("at least 1"));
        assert!(rejected("[limits]\nmax_limit_deviation_pct = 0\n").contains("above 0"));
        assert!(rejected("[limits]\nallowed_accounts = [\"tfsa abc\"]\n").contains("invalid account id"));
        assert!(rejected("[limits]\nblocked_securities = [\"XEQT\"]\n").contains("invalid security id"));
        assert!(rejected("[paper]\nstarting_cash = \"-1\"\n").contains("paper.starting_cash"));
    }
}
