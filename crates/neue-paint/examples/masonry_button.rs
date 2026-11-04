//! Masonry spike (task 2.1): the neue-paint Aqua button as a `masonry_core` widget in a
//! `masonry_winit` window, rendered by `vello_gpu` (`imaging_vello_hybrid`). Press it to see
//! the pressed state. macOS only: `masonry_winit` is a spike shortcut (ADR 0002 §4) and is
//! replaced by neue-platform's drivers in tasks 2.4 and 2.5.
//!
//! ```sh
//! cargo run -p neue-paint --example masonry_button --features imaging
//! ```

#[cfg(target_os = "macos")]
#[path = "../tests/common/aqua_widget.rs"]
#[allow(
    dead_code,
    reason = "shared with tests/masonry.rs, which uses `with_state`"
)]
mod aqua_widget;

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use masonry_core::core::{DefaultProperties, ErasedAction, NewWidget, WidgetId};
    use masonry_core::dpi::LogicalSize;
    use masonry_winit::app::{AppDriver, DriverCtx, NewWindow, WindowId};
    use masonry_winit::winit::window::Window;
    use neue_paint::Appearance;
    use neue_paint::demo::{BUTTON_TILE_HEIGHT, BUTTON_TILE_WIDTH};

    /// The tile emits no actions.
    struct Driver;

    impl AppDriver for Driver {
        fn on_action(
            &mut self,
            _window_id: WindowId,
            _ctx: &mut DriverCtx<'_>,
            _widget_id: WidgetId,
            _action: ErasedAction,
        ) {
        }
    }

    let appearance = match std::env::args().nth(1).as_deref() {
        Some("dark") => Appearance::Dark,
        _ => Appearance::Light,
    };
    let size = LogicalSize::new(BUTTON_TILE_WIDTH, BUTTON_TILE_HEIGHT);
    let attributes = Window::default_attributes()
        .with_title("neue-paint: Masonry button")
        .with_inner_size(size)
        .with_resizable(false);
    let widget = NewWidget::new(aqua_widget::AquaButtonTile::new(appearance)).erased();
    masonry_winit::app::run(
        vec![NewWindow::new(attributes, widget)],
        Driver,
        DefaultProperties::new(),
    )?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("masonry_button runs on macOS only (masonry_winit is a spike shortcut)");
}
