use toml::{Table, Value};

use crate::Error;

const DEFAULTS: &str = include_str!("../../../config.toml");

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Count,
    Flag,
    List,
}

const SETTINGS: [(&str, Option<&str>, Kind); 12] = [
    ("mode", None, Kind::Text),
    ("max_order_value", Some("limits"), Kind::Text),
    ("max_daily_spend", Some("limits"), Kind::Text),
    ("max_orders_per_day", Some("limits"), Kind::Count),
    ("limit_only", Some("limits"), Kind::Flag),
    ("max_limit_deviation_pct", Some("limits"), Kind::Text),
    ("market_hours_only", Some("limits"), Kind::Flag),
    ("allowed_accounts", Some("limits"), Kind::List),
    ("blocked_securities", Some("limits"), Kind::List),
    ("require_approval_above", Some("limits"), Kind::Text),
    ("approve_in_chat", Some("limits"), Kind::Flag),
    ("starting_cash", Some("paper"), Kind::Text),
];

/// The defaults, overridden by whatever `text` sets.
pub(crate) fn complete(text: &str) -> Result<Table, Error> {
    let mut table: Table = DEFAULTS.parse()?;
    for (key, value) in text.parse::<Table>()? {
        match (table.get_mut(&key), value) {
            (Some(Value::Table(base)), Value::Table(over)) => base.extend(over),
            (_, value) => {
                table.insert(key, value);
            }
        }
    }
    Ok(table)
}

pub(crate) fn assign(table: &mut Table, key: &str, value: &str) -> Result<(), Error> {
    let Some(&(name, section, kind)) = SETTINGS.iter().find(|(name, ..)| *name == key) else {
        let known = SETTINGS.iter().map(|(name, ..)| *name).collect::<Vec<_>>().join(", ");
        return Err(Error::UnknownSetting { key: key.to_owned(), known });
    };
    let value = match kind {
        Kind::Text => Value::String(value.to_owned()),
        Kind::Count => {
            Value::Integer(value.parse().map_err(|_| Error::Invalid { field: name, reason: "must be a whole number" })?)
        }
        Kind::Flag => Value::Boolean(value.parse().map_err(|_| Error::Invalid { field: name, reason: "must be true or false" })?),
        Kind::List => Value::Array(
            value.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| Value::String(s.to_owned())).collect(),
        ),
    };
    let target = match section {
        None => table,
        Some(section) => table
            .entry(section)
            .or_insert_with(|| Table::new().into())
            .as_table_mut()
            .ok_or(Error::Invalid { field: section, reason: "must be a table" })?,
    };
    target.insert(name.to_owned(), value);
    Ok(())
}
