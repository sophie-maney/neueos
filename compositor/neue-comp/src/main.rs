//! `neue-comp`: the neueOS Wayland compositor.
//!
//! ```text
//! neue-comp [--winit|--headless] [-c|--command <cmd>]...
//! ```
//!
//! The default backend is winit when compiled in (the `winit` feature), else headless.
//! The tty backend (task 1.4) will become the default outside a graphical session.

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, bail};
    use neue_comp::{BackendKind, Options};
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let default_backend = if cfg!(feature = "winit") {
        BackendKind::Winit
    } else {
        BackendKind::Headless
    };
    let mut options = Options::new(default_backend);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--winit" => options.backend = BackendKind::Winit,
            "--headless" => options.backend = BackendKind::Headless,
            "-c" | "--command" => options
                .commands
                .push(args.next().context("--command needs an argument")?),
            "-h" | "--help" => {
                println!("usage: neue-comp [--winit|--headless] [-c|--command <cmd>]...");
                return Ok(());
            }
            other => bail!("unknown argument `{other}` (try --help)"),
        }
    }

    neue_comp::run(options).context("neue-comp failed")
}

#[cfg(not(target_os = "linux"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("neue-comp only runs on Linux")
}
