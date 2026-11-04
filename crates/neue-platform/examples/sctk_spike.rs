//! Masonry spike B (task 2.2): Masonry on neue-platform's sctk driver.
//!
//! - an xdg-toplevel with the neue-paint Aqua button, a `TextInput` (text-input-v3) and a
//!   `Selector`, whose menu is a Masonry layer shown as an xdg-popup;
//! - a layer-shell surface (top edge) with a label.
//!
//! Linux only, under a Wayland compositor (neue-comp in the VM, docs/checks/phase-2.md):
//!
//! ```sh
//! WGPU_BACKEND=gl RUST_LOG=info,neue_platform=debug cargo run -p neue-platform --example sctk_spike
//! ```
//!
//! Uses Masonry's stock widgets (`masonry` crate), which is allowed in spike examples only
//! (ADR 0002 §4).

#[cfg(target_os = "linux")]
#[path = "../../neue-paint/tests/common/aqua_widget.rs"]
#[allow(
    dead_code,
    reason = "shared with neue-paint's tests, which use `with_state`"
)]
mod aqua_widget;

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    use std::sync::Arc;

    use masonry::core::{NewWidget, Widget as _};
    use masonry::peniko::{Blob, Color};
    use masonry::widgets::{Flex, Label, Selector, TextInput};
    use neue_platform::wayland::{self, Options, SurfaceRole, SurfaceSpec};

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    static FONT: &[u8] = include_bytes!("../../neue-paint/fonts/FiraSans-Regular.ttf");

    let window = Flex::column()
        .with_fixed(Label::new("Masonry on sctk (task 2.2)").prepare())
        .with_fixed(aqua_widget::AquaButtonTile::new(neue_paint::Appearance::Light).prepare())
        .with_fixed(
            TextInput::new("")
                .with_placeholder("Type here (text-input-v3)")
                .prepare(),
        )
        .with_fixed(
            Selector::new(vec!["Linen".into(), "Aqua".into(), "Graphite".into()]).prepare(),
        );
    let bar = Flex::row().with_fixed(Label::new("neueOS layer-shell surface").prepare());

    let surfaces = vec![
        SurfaceSpec {
            role: SurfaceRole::Toplevel {
                title: "neue-platform: sctk spike".into(),
                app_id: "org.neueos.SctkSpike".into(),
                width: 420,
                height: 320,
            },
            root: NewWidget::new(window).erased(),
            background: Color::from_rgb8(0x4a, 0x50, 0x5a),
        },
        SurfaceSpec {
            role: SurfaceRole::Layer {
                namespace: "neue-sctk-spike".into(),
                width: 320,
                height: 32,
                margin_top: 40,
            },
            root: NewWidget::new(bar).erased(),
            background: Color::from_rgb8(0x2e, 0x4a, 0x6e),
        },
    ];
    let options = Options {
        default_properties: Arc::new(masonry::theme::default_property_set()),
        font: Some(Blob::new(Arc::new(FONT))),
    };
    wayland::run(
        surfaces,
        options,
        Box::new(|surface, widget, action| {
            tracing::info!(surface, %widget, ?action, "action");
        }),
    )?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("sctk_spike runs on Linux under a Wayland compositor only");
}
