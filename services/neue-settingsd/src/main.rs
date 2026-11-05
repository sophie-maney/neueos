//! `neue-settingsd`: serves `org.neueos.Settings1` on the session bus (ADR 0074). Started
//! by the session (task 3.27); logs go to stderr, filtered by `RUST_LOG`.

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    #[cfg(target_os = "linux")]
    return run();
    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("neue-settingsd runs only on Linux; apps on macOS use `Store::in_memory`");
}

#[cfg(target_os = "linux")]
fn run() -> anyhow::Result<()> {
    use neue_settingsd::daemon::{self, Outputs};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(daemon::run(
        neue_config::Layers::standard(),
        Outputs::standard(),
    ))?;
    Ok(())
}
