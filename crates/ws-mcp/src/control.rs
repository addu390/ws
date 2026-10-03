use anyhow::bail;
use ws_policy::{Approver, TicketId};

use crate::consent::describe;
use crate::home::Home;
use crate::prompt::confirm;
use crate::wiring::Wiring;

pub fn approve(id: &TicketId) -> anyhow::Result<()> {
    let guard = Wiring::build(&Home::locate()?)?.guard()?;
    let ticket = guard.ticket(id)?;
    eprintln!("{}", describe(&ticket));
    eprintln!("Account {}, about {}, ticket expires {}.", ticket.order().account(), ticket.value(), ticket.expires());
    if !confirm("approve")? {
        bail!("not approved");
    }
    guard.approve(id, Approver::Terminal)?;
    eprintln!("Approved. The agent can now place {id} once, before it expires.");
    Ok(())
}

pub fn kill() -> anyhow::Result<()> {
    halt(&Home::locate()?)?;
    eprintln!("Kill switch engaged: no orders can be previewed, placed, or cancelled. Run `ws-mcp resume` to lift it.");
    Ok(())
}

pub fn resume() -> anyhow::Result<()> {
    let home = Home::locate()?;
    if !home.kill().exists() {
        eprintln!("The kill switch is not engaged.");
        return Ok(());
    }
    if !confirm("resume")? {
        bail!("kill switch left engaged");
    }
    lift(&home)?;
    eprintln!("Kill switch lifted. WS_KILL=1 in the environment still halts trading if set.");
    Ok(())
}

pub fn halt(home: &Home) -> anyhow::Result<()> {
    Ok(ws_common::write(&home.kill(), b"")?)
}

pub fn lift(home: &Home) -> anyhow::Result<()> {
    ws_common::remove(&home.kill())?;
    Ok(())
}
