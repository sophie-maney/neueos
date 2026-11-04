//! neue-platform's winit driver (task 2.4): a text field, IME, the clipboard, a Masonry
//! layer (the selector's menu), AccessKit and several windows.
//!
//! macOS only, like the driver (ADR 0032):
//!
//! ```sh
//! RUST_LOG=info,neue_platform=debug cargo run -p neue-platform --example winit_demo
//! ```
//!
//! Manual checks (IME, VoiceOver) are in docs/checks/phase-2.md §2. Uses Masonry's stock
//! widgets (`masonry` crate), which is allowed in examples only (ADR 0002 §4).

#[cfg(target_os = "macos")]
#[path = "../../neue-paint/tests/common/aqua_widget.rs"]
#[allow(
    dead_code,
    reason = "shared with neue-paint's tests, which use `with_state`"
)]
mod aqua_widget;

#[cfg(target_os = "macos")]
mod demo {
    use masonry::core::{ErasedAction, NewWidget, Widget as _, WidgetId, WidgetTag};
    use masonry::peniko::Color;
    use masonry::widgets::{Button, ButtonPress, Flex, Label, Selector, TextAction, TextInput};
    use neue_platform::winit::{App, Ctx, WindowId, WindowSpec};

    use super::aqua_widget::AquaButtonTile;

    const ECHO: WidgetTag<Label> = WidgetTag::named("echo");
    const NEW_WINDOW: WidgetTag<Button> = WidgetTag::named("new-window");
    const CLOSE: WidgetTag<Button> = WidgetTag::named("close");

    #[derive(Debug, Default)]
    pub(super) struct Demo {
        opened: u32,
    }

    impl Demo {
        fn window(&mut self) -> WindowSpec {
            self.opened += 1;
            let n = self.opened;
            let buttons = Flex::row()
                .with_fixed(NewWidget::new(Button::with_text("New window")).with_tag(NEW_WINDOW))
                .with_fixed(NewWidget::new(Button::with_text("Close")).with_tag(CLOSE));
            let root = Flex::column()
                .with_fixed(Label::new(format!("neue-platform winit driver, window {n}")).prepare())
                .with_fixed(AquaButtonTile::new(neue_paint::Appearance::Light).prepare())
                .with_fixed(
                    TextInput::new("")
                        .with_placeholder("Type here; try an input method")
                        .prepare(),
                )
                .with_fixed(
                    NewWidget::new(Label::new("Press Return to echo the text")).with_tag(ECHO),
                )
                .with_fixed(
                    Selector::new(vec!["Linen".into(), "Aqua".into(), "Graphite".into()]).prepare(),
                )
                .with_fixed(buttons.prepare());
            WindowSpec {
                title: format!("neue-platform: winit demo {n}"),
                width: 440,
                height: 360,
                root: NewWidget::new(root).erased(),
                background: Color::from_rgb8(0x4a, 0x50, 0x5a),
            }
        }
    }

    impl App for Demo {
        fn on_start(&mut self, cx: &mut Ctx<'_>) {
            let spec = self.window();
            cx.open_window(spec);
        }

        fn on_action(
            &mut self,
            cx: &mut Ctx<'_>,
            window: WindowId,
            widget: WidgetId,
            action: ErasedAction,
        ) {
            tracing::info!(%window, %widget, ?action, "action");
            let Some(root) = cx.render_root(window) else {
                return;
            };
            let id_of = |tag: WidgetTag<Button>| root.get_widget_with_tag(tag).map(|w| w.id());
            let (new_window, close) = (id_of(NEW_WINDOW), id_of(CLOSE));
            if let Some(TextAction::Entered(text)) = action.downcast_ref::<TextAction>() {
                root.edit_widget_with_tag(ECHO, |mut label| {
                    Label::set_text(&mut label, format!("You typed: {text}"));
                });
            } else if action.is::<ButtonPress>() {
                if new_window == Some(widget) {
                    let spec = self.window();
                    cx.open_window(spec);
                } else if close == Some(widget) {
                    cx.close_window(window);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use std::sync::Arc;

    use masonry::peniko::Blob;
    use neue_platform::Options;

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    static FONT: &[u8] = include_bytes!("../../neue-paint/fonts/FiraSans-Regular.ttf");
    let options = Options {
        default_properties: Arc::new(masonry::theme::default_property_set()),
        font: Some(Blob::new(Arc::new(FONT))),
    };
    neue_platform::winit::run(&mut demo::Demo::default(), options)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("winit_demo runs on macOS only; on Linux, see the sctk_spike example");
}
