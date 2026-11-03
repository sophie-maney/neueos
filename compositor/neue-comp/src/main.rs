//! `neue-comp`: the neueOS Wayland compositor.
//!
//! ```text
//! neue-comp [--winit] [-c|--command <cmd>]...
//! ```

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, bail};
    use neue_comp::{BackendKind, Options};
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let mut options = Options {
        backend: BackendKind::Winit,
        commands: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--winit" => options.backend = BackendKind::Winit,
            "-c" | "--command" => options
                .commands
                .push(args.next().context("--command needs an argument")?),
            "-h" | "--help" => {
                println!("usage: neue-comp [--winit] [-c|--command <cmd>]...");
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
