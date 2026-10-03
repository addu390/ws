use std::path::Path;

use anyhow::Context;
use ws_config::Config;

use crate::home::Home;

pub fn run(key: Option<&str>, value: Option<&str>) -> anyhow::Result<()> {
    let home = Home::locate()?;
    match key.zip(value) {
        Some((key, value)) => {
            set(&home, &[(key, value)])?;
            eprintln!("Set {key} = {value}. A running ws-mcp picks it up when restarted.");
        }
        None => print!("{}", Config::effective(&read(&home.config())?)?),
    }
    Ok(())
}

pub fn set(home: &Home, changes: &[(&str, &str)]) -> anyhow::Result<()> {
    let path = home.config();
    let text = Config::set(&read(&path)?, changes)?;
    ws_common::write(&path, text.as_bytes())?;
    Ok(())
}

pub fn load(home: &Home) -> anyhow::Result<Config> {
    let path = home.config();
    Config::load(&path).with_context(|| format!("reading {}", path.display()))
}

fn read(path: &Path) -> anyhow::Result<String> {
    let bytes = ws_common::read(path)?.unwrap_or_default();
    String::from_utf8(bytes).with_context(|| format!("{} is not UTF-8", path.display()))
}
