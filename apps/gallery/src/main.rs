//! `cargo xtask gallery` (or `cargo run -p neue-gallery`).

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let settings = neue_kit::Settings {
        app_id: "org.neueos.Gallery".into(),
        ..neue_kit::Settings::default()
    };
    // `NEUE_GALLERY_PAGE=<n>` opens page n (0-based), for screenshots.
    let page = std::env::var("NEUE_GALLERY_PAGE")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    neue_kit::run(neue_gallery::Gallery::on_page(page), settings)?;
    Ok(())
}
