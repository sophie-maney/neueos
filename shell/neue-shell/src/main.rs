//! `neue-shell`: the desktop shell process. The compositor starts it with `--shell` and
//! hands it its private socket in `WAYLAND_SOCKET` (ADR 0066); it isn't meant to be run by
//! hand.

#[cfg(target_os = "linux")]
mod appearance;
#[cfg(target_os = "linux")]
mod global_menu;
#[cfg(target_os = "linux")]
mod shell;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    #[cfg(target_os = "linux")]
    return shell::run();
    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("neue-shell runs only in a neueOS session (Linux)");
}
