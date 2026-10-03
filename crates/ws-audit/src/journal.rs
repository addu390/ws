use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{Entry, Error};

/// A failed `record` must be treated as a refusal. Nothing happens unless it was written down.
pub trait Journal: Send + Sync {
    fn record(&self, entry: &Entry) -> Result<(), Error>;
}

#[derive(Debug)]
pub struct File {
    path: PathBuf,
    file: Mutex<fs::File>,
}

impl File {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let file = ws_common::append(&path)?;
        Ok(Self { path, file: Mutex::new(file) })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn recent(path: &Path, limit: usize) -> Result<Vec<Entry>, Error> {
        let bytes = ws_common::read(path)?.unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        text.lines().rev().filter(|l| !l.trim().is_empty()).take(limit).map(|l| Ok(serde_json::from_str(l)?)).collect()
    }
}

impl Journal for File {
    fn record(&self, entry: &Entry) -> Result<(), Error> {
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        let mut file = self.file.lock().map_err(|_| Error::Poisoned)?;
        file.write_all(&line)?;
        file.sync_data()?;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct Memory {
    entries: Mutex<Vec<Entry>>,
}

impl Memory {
    #[must_use]
    pub fn entries(&self) -> Vec<Entry> {
        self.entries.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl Journal for Memory {
    fn record(&self, entry: &Entry) -> Result<(), Error> {
        self.entries.lock().map_err(|_| Error::Poisoned)?.push(entry.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, Verdict};
    use rust_decimal::dec;
    use ws_core::fixtures::{account, noon, quantity, security};
    use ws_core::{Order, OrderId};

    fn order() -> Order {
        Order::market_buy(account(), security(), quantity(dec!(2)))
    }

    fn entries() -> [Entry; 2] {
        let at = noon();
        [
            Entry::new(at, Action::Preview, Verdict::Denied).because("market orders are disabled").for_order(order()),
            Entry::new(at, Action::Cancel, Verdict::Succeeded)
                .with_order_id(OrderId::parse("order-1").unwrap_or_else(|e| panic!("{e}"))),
        ]
    }

    #[test]
    fn file_appends_one_line_per_entry() {
        let scratch = ws_common::fixtures::Scratch::new();
        let path = scratch.path("audit.jsonl");
        let [first, second] = entries();

        File::open(&path).and_then(|j| j.record(&first)).unwrap_or_else(|e| panic!("{e}"));
        File::open(&path).and_then(|j| j.record(&second)).unwrap_or_else(|e| panic!("{e}"));

        let text = fs::read_to_string(&path).unwrap_or_default();
        let read: Vec<Entry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        assert_eq!(read, vec![first.clone(), second.clone()]);
        assert_eq!(File::recent(&path, 1).ok(), Some(vec![second.clone()]));
        assert_eq!(File::recent(&path, 10).ok(), Some(vec![second, first]));
        assert_eq!(File::recent(&scratch.path("missing.jsonl"), 10).ok(), Some(Vec::new()));
    }

    #[test]
    fn omits_empty_fields() {
        let [_, cancel] = entries();
        let json = serde_json::to_string(&cancel).unwrap_or_default();
        assert_eq!(
            json,
            r#"{"at":"2026-10-02T15:00:00Z","action":"cancel","verdict":"succeeded","order_id":"order-1"}"#
        );
    }

    #[test]
    fn memory_keeps_entries_in_order() {
        let journal = Memory::default();
        let [first, second] = entries();
        journal.record(&first).unwrap_or_else(|e| panic!("{e}"));
        journal.record(&second).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(journal.entries(), vec![first, second]);
    }
}
