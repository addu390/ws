use std::path::PathBuf;

use crate::{Error, Ticket, TicketId};

#[derive(Debug, Clone)]
pub(crate) struct Desk {
    dir: PathBuf,
}

impl Desk {
    pub(crate) fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub(crate) fn file(&self, ticket: &Ticket) -> Result<(), Error> {
        Ok(ws_common::write_json(&self.path(ticket.id(), "json"), ticket)?)
    }

    pub(crate) fn get(&self, id: &TicketId) -> Result<Option<Ticket>, Error> {
        Ok(ws_common::read_json(&self.path(id, "json"))?)
    }

    pub(crate) fn list(&self) -> Result<Vec<Ticket>, Error> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(ws_common::Error::Io { path: self.dir.clone(), source }.into()),
        };
        let mut tickets = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")).and_then(|n| TicketId::parse(n).ok()) else {
                continue;
            };
            if let Some(ticket) = self.get(&id)? {
                tickets.push(ticket);
            }
        }
        Ok(tickets)
    }

/// The rename is what makes a ticket single use across processes.
    pub(crate) fn take(&self, id: &TicketId) -> Result<Option<Ticket>, Error> {
        let (filed, claimed) = (self.path(id, "json"), self.path(id, "taken"));
        match std::fs::rename(&filed, &claimed) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ws_common::Error::Io { path: filed, source }.into()),
        }
        let ticket = ws_common::read_json(&claimed);
        ws_common::remove(&claimed)?;
        Ok(ticket?)
    }

    fn path(&self, id: &TicketId, extension: &str) -> PathBuf {
        self.dir.join(format!("{id}.{extension}"))
    }
}
