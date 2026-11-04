//! `neue-session [start|stop]`: see the library docs. `start` is the default and what
//! greetd runs.

use anyhow::bail;
use neue_session::{Systemctl, run, stop};
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .init();

    match std::env::args().nth(1).as_deref() {
        None | Some("start") => run(&mut Systemctl, std::env::vars())?,
        Some("stop") => stop(&mut Systemctl)?,
        Some("-h" | "--help") => {
            eprintln!("usage: neue-session [start|stop]");
        }
        Some(other) => bail!("unknown argument `{other}` (try --help)"),
    }
    Ok(())
}
