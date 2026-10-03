use std::path::Path;
use std::sync::Arc;

use anyhow::bail;
use ws_api::Api;
use ws_auth::{Keyring, Store};
use ws_broker::{Broker, Reader};
use ws_config::{Config, Mode};
use ws_core::{Clock, SystemClock};
use ws_net::Client;
use ws_paper::Paper;
use ws_policy::{Guard, Kill};

use crate::config;
use crate::home::Home;
use crate::read::Read;
use crate::server::Server;
use crate::trade::Trade;

pub struct Wiring {
    pub reader: Arc<dyn Reader>,
    pub guard: Option<Arc<Guard>>,
}

impl Wiring {
    pub fn build(home: &Home) -> anyhow::Result<Self> {
        let config = config::load(home)?;
        let api = api()?;
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        Ok(match config.mode() {
            Mode::Read => Self { reader: api, guard: None },
            Mode::Paper => {
                let paper = Arc::new(Paper::new(api, config.paper_cash(), home.ledger(), clock.clone()));
                let guard = guard(paper.clone(), &config, home, &home.paper(), clock)?;
                Self { reader: paper, guard: Some(guard) }
            }
            Mode::Trade => {
                let guard = guard(api.clone(), &config, home, &home.trade(), clock)?;
                Self { reader: api, guard: Some(guard) }
            }
        })
    }

    pub fn guard(self) -> anyhow::Result<Arc<Guard>> {
        match self.guard {
            Some(guard) => Ok(guard),
            None => bail!("ws-mcp is in read mode. Set mode = \"paper\" or \"trade\" in ~/.ws-mcp/config.toml"),
        }
    }
}

pub fn api() -> anyhow::Result<Arc<Api>> {
    let store = Arc::new(Keyring::default());
    let Some(session) = store.load()? else { bail!("not logged in, run `ws-mcp login` in a terminal first") };
    Ok(Arc::new(Api::new(Client::chrome()?, session, store)))
}

fn guard(broker: Arc<dyn Broker>, config: &Config, home: &Home, dir: &Path, clock: Arc<dyn Clock>) -> anyhow::Result<Arc<Guard>> {
    let journal = Arc::new(ws_audit::File::open(home.audit())?);
    Ok(Arc::new(Guard::new(broker, config, journal, Kill::new(home.kill()), dir, clock)))
}

pub async fn serve() -> anyhow::Result<()> {
    let wiring = Wiring::build(&Home::locate()?)?;
    Server::new(wiring.reader, wiring.guard).run().await
}

pub async fn read(action: Read) -> anyhow::Result<()> {
    let wiring = Wiring::build(&Home::locate()?)?;
    println!("{}", action.perform(wiring.reader.as_ref()).await?);
    Ok(())
}

pub async fn trade(action: Trade) -> anyhow::Result<()> {
    let guard = Wiring::build(&Home::locate()?)?.guard()?;
    println!("{}", action.perform(&guard).await?);
    Ok(())
}
