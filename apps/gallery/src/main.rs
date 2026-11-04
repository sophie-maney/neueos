//! `cargo xtask gallery` (or `cargo run -p neue-gallery`).

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let settings = neue_kit::Settings {
        app_id: "org.neueos.Gallery".into(),
        ..neue_kit::Settings::default()
    };
    neue_kit::run(neue_gallery::Gallery::default(), settings)?;
    Ok(())
}
