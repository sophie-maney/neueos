//! `neue-comp`: the neueOS Wayland compositor.
//!
//! ```text
//! neue-comp [--tty|--winit|--headless] [-c|--command <cmd>]...
//!           [--no-xwayland] [--xwayland-satellite <path>] [--notify] [--shell <cmd>]
//! ```
//!
//! Default backend: winit inside another Wayland session (`WAYLAND_DISPLAY` set), else tty;
//! each only when its cargo feature is compiled in, falling back to headless.

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, bail};
    use neue_comp::{BackendKind, Options};
    use tracing_subscriber::EnvFilter;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let nested = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let default_backend = if cfg!(feature = "winit") && nested {
        BackendKind::Winit
    } else if cfg!(feature = "tty") {
        BackendKind::Tty
    } else if cfg!(feature = "winit") {
        BackendKind::Winit
    } else {
        BackendKind::Headless
    };
    let mut options = Options::new(default_backend);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tty" => options.backend = BackendKind::Tty,
            "--winit" => options.backend = BackendKind::Winit,
            "--headless" => options.backend = BackendKind::Headless,
            "-c" | "--command" => options
                .commands
                .push(args.next().context("--command needs an argument")?),
            "--no-xwayland" => options.xwayland = None,
            "--notify" => options.notify = true,
            "--shell" => options.shell = Some(args.next().context("--shell needs a command")?),
            "--xwayland-satellite" => {
                let path = args.next().context("--xwayland-satellite needs a path")?;
                options
                    .xwayland
                    .get_or_insert_with(Default::default)
                    .satellite = path.into();
            }
            "-h" | "--help" => {
                println!(
                    "usage: neue-comp [--tty|--winit|--headless] [-c|--command <cmd>]...\n\
                     \x20                [--no-xwayland] [--xwayland-satellite <path>] [--notify]\n\
                     \x20                [--shell <cmd>]"
                );
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
