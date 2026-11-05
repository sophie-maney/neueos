//! Widget goldens (feature `testing`, ADR 0037): render a widget alone in a window, in a
//! state and theme, the way goldens are rendered (`vello_cpu`), and compare against PNGs.
//!
//! ```no_run
//! # use neue_kit::{Button, Theme};
//! # use neue_kit::testing::{self, Goldens, NORMAL, PRESSED};
//! // In an integration test, pass `env!("CARGO_TARGET_TMPDIR")` for the failures.
//! let goldens = Goldens::new(
//!     concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"),
//!     std::env::temp_dir(),
//! );
//! // push-button-{normal,pressed}-{light,dark}@{1,2}x.png
//! testing::check_states(&goldens, "push-button", (110, 40), Theme::default(),
//!     &[NORMAL, PRESSED], || Button::<()>::new("Button").into())
//!     .unwrap();
//! ```
//!
//! Every widget task ships goldens this way: its states × light/dark × scale 1 and 2.

use std::sync::Arc;
use std::time::Duration;

use masonry_core::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use masonry_core::core::{Ime, NewWidget, PointerButton, TextEvent, Widget, WidgetId, WidgetRef};
use masonry_core::dpi::PhysicalSize;
use masonry_core::kurbo::{Axis, Point, Vec2};
use masonry_core::peniko::{Blob, Color};
use masonry_testing::{TestHarness, TestHarnessParams};
use neue_paint::Pixmap;
pub use neue_paint::golden::{Goldens, Mismatch};
use neue_theme::metrics::spacing;
use neue_theme::{Appearance, Theme};

use crate::app::Window;
use crate::element::Element;
use crate::theme::default_properties;
use crate::widgets::{Popover, RootWidget, StackWidget, popover};

/// Errors from [`Fixture::render`] and [`check_states`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("render of {0}×{1} pixels is too large for a pixmap")]
    TooLarge(u32, u32),
    #[error("{} of the goldens didn't match:\n{}", .0.len(), list(.0))]
    Mismatches(Vec<Mismatch>),
}

fn list(mismatches: &[Mismatch]) -> String {
    mismatches
        .iter()
        .map(|m| format!("  {m}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One widget in a window of its own, in the window's background colour, with a small
/// margin around it. Drive it into a state, then [`render`](Self::render).
pub struct Fixture {
    harness: TestHarness<RootWidget>,
    target: WidgetId,
}

impl std::fmt::Debug for Fixture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fixture")
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}

impl Fixture {
    /// `element` at the top left of a `size` (logical pixels) window drawn at `scale`.
    /// Message handlers are dropped: a fixture renders, it doesn't run an app.
    pub fn new<M>(
        element: impl Into<Element<M>>,
        theme: Theme,
        scale: f64,
        size: (u32, u32),
    ) -> Self {
        // Shortcuts in goldens read the same on every host.
        crate::menu::command_is_ctrl_here();
        let (widget, _wiring) = element.into().into_parts();
        let target = widget.id();
        let mut stack = StackWidget::new(Axis::Vertical, 0.0);
        stack.padding = spacing::S;
        stack.push(widget);
        let root = RootWidget::new(NewWidget::new(stack).erased());
        Self::with_root(root, Some(target), theme, scale, size)
    }

    /// `window` as it opens, filling a window of its size drawn at `scale`: its content
    /// under its chrome, if it has some. The target is the window's root.
    pub fn window<M>(window: Window<M>, theme: Theme, scale: f64) -> Self {
        crate::menu::command_is_ctrl_here();
        let (root, size) = window.into_root();
        Self::with_root(root, None, theme, scale, size)
    }

    fn with_root(
        root: RootWidget,
        target: Option<WidgetId>,
        theme: Theme,
        scale: f64,
        size: (u32, u32),
    ) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "small positive window sizes"
        )]
        let physical = |v: u32| (f64::from(v) * scale).ceil() as u32;
        let params = TestHarnessParams::default()
            .with_size(PhysicalSize::new(physical(size.0), physical(size.1)))
            .with_scale(scale)
            .with_padding(0)
            .with_background(Color::TRANSPARENT);
        let root = NewWidget::new(root);
        let root_id = root.id();
        let mut harness = TestHarness::create_with(default_properties(theme), root, params);
        harness
            .render_root()
            .register_fonts(Blob::new(Arc::new(neue_paint::text::ui_font_data())));
        harness.set_focus_fallback(Some(root_id));
        Self {
            harness,
            target: target.unwrap_or(root_id),
        }
    }

    /// Puts the pointer over the widget.
    pub fn hover(&mut self) {
        self.harness.mouse_move_to(self.target);
    }

    /// Holds the primary button down over the widget.
    pub fn press(&mut self) {
        self.hover();
        self.harness.mouse_button_press(None);
    }

    /// Holds the primary button down `(fx, fy)` of the way across the widget (as
    /// [`hover_at`](Self::hover_at)).
    pub fn press_at(&mut self, fx: f64, fy: f64) {
        self.hover_at(fx, fy);
        self.harness.mouse_button_press(None);
    }

    /// Clicks the secondary button over the widget's middle (opens a context menu), and
    /// leaves the pointer there.
    pub fn context_click(&mut self) {
        // Not `hover`: the widget under the pointer is the area's child.
        self.hover_at(0.5, 0.5);
        self.harness
            .mouse_button_press(Some(PointerButton::Secondary));
        self.harness
            .mouse_button_release(Some(PointerButton::Secondary));
    }

    /// Presses and releases `key` on the focused widget.
    pub fn press_key(&mut self, key: NavKey) {
        let named = match key {
            NavKey::Up => NamedKey::ArrowUp,
            NavKey::Down => NamedKey::ArrowDown,
            NavKey::Left => NamedKey::ArrowLeft,
            NavKey::Right => NamedKey::ArrowRight,
            NavKey::Home => NamedKey::Home,
            NavKey::End => NamedKey::End,
            NavKey::Return => NamedKey::Enter,
            NavKey::Escape => NamedKey::Escape,
            NavKey::Menu => NamedKey::ContextMenu,
        };
        self.key(Key::Named(named), Modifiers::empty());
    }

    /// Puts the pointer `(fx, fy)` of the way across the widget (0 is its left or top
    /// edge, 1 its right or bottom edge).
    pub fn hover_at(&mut self, fx: f64, fy: f64) {
        let widget = self.harness.get_widget_with_id(self.target);
        let ctx = widget.ctx();
        let size = ctx.border_box().size();
        let at = ctx.to_window(Point::new(size.width * fx, size.height * fy));
        self.harness.mouse_move(at);
    }

    /// Turns the mouse wheel (or swipes) over the widget: `dx` and `dy` logical pixels,
    /// negative to see further right or down.
    pub fn scroll(&mut self, dx: f64, dy: f64) {
        self.hover_at(0.5, 0.5);
        self.harness.mouse_wheel(Vec2::new(dx, dy));
    }

    /// Gives the widget keyboard focus, as Tab would: to the widget itself, or if that
    /// takes no focus (a text view's scroll view), to the first widget inside that does.
    pub fn focus(&mut self) {
        let target =
            first_focusable(self.harness.get_widget_with_id(self.target)).unwrap_or(self.target);
        self.harness.focus_on(Some(target));
    }

    /// Shows `popover` next to the widget, or to the first widget in it that takes focus
    /// (a button in a container), as [`Cx::show_popover`](crate::Cx::show_popover) would.
    /// Its messages go nowhere.
    pub fn show_popover<M: Clone + 'static>(&mut self, popover: Popover<M>) {
        let anchor =
            first_focusable(self.harness.get_widget_with_id(self.target)).unwrap_or(self.target);
        let (widget, _wiring) = popover.into_parts();
        self.harness
            .edit_widget_with_id(anchor, |mut owner| popover::show(&mut owner, widget));
    }

    /// Disables (or re-enables) the widget.
    pub fn set_disabled(&mut self, disabled: bool) {
        self.harness
            .edit_widget_with_id(self.target, |mut w| w.ctx.set_disabled(disabled));
    }

    /// Makes the window active or inactive.
    pub fn set_window_focused(&mut self, focused: bool) {
        self.harness
            .process_text_event(TextEvent::WindowFocusChange(focused));
    }

    /// Types `text` into the focused widget, one key per character.
    pub fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.key(Key::Character(c.to_string()), Modifiers::empty());
        }
    }

    /// Selects all of a focused text field's or text view's text (the command key and A).
    pub fn select_text(&mut self) {
        let command = if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
        self.key(Key::Character("a".to_owned()), command);
    }

    /// An input method's uncommitted text at the caret of the focused widget, with the
    /// IME's caret at its end.
    pub fn compose(&mut self, preedit: &str) {
        self.harness
            .process_text_event(TextEvent::Ime(Ime::Enabled));
        let end = preedit.len();
        self.harness.process_text_event(TextEvent::Ime(Ime::Preedit(
            preedit.to_owned(),
            Some((end, end)),
        )));
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        for state in [KeyState::Down, KeyState::Up] {
            self.harness
                .process_text_event(TextEvent::Keyboard(KeyboardEvent {
                    state,
                    key: key.clone(),
                    modifiers,
                    ..KeyboardEvent::default()
                }));
        }
    }

    /// Lets animations run for `by`, in one frame.
    pub fn advance(&mut self, by: Duration) {
        let ms = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
        self.harness.animate_ms(ms);
    }

    /// The window as premultiplied pixels, as goldens store them.
    pub fn render(&mut self) -> Result<Pixmap, Error> {
        let image = self.harness.render();
        let (width, height) = image.dimensions();
        let too_large = || Error::TooLarge(width, height);
        let w = u16::try_from(width).map_err(|_| too_large())?;
        let h = u16::try_from(height).map_err(|_| too_large())?;
        neue_paint::golden::pixmap_from_unpremultiplied(w, h, image.into_raw())
            .ok_or_else(too_large)
    }
}

/// `widget`, or the first widget in it, depth first, that takes focus.
fn first_focusable(widget: WidgetRef<'_, dyn Widget>) -> Option<WidgetId> {
    if widget.accepts_focus() {
        return Some(widget.id());
    }
    widget.children().into_iter().find_map(first_focusable)
}

/// Keys [`Fixture::press_key`] presses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavKey {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Return,
    Escape,
    /// The Menu (context menu) key.
    Menu,
}

/// A state to render a widget in: its name in golden file names, and what puts the
/// [`Fixture`] there.
pub type State = (&'static str, fn(&mut Fixture));

/// At rest.
pub const NORMAL: State = ("normal", |_| {});
/// Pointer over it.
pub const HOVERED: State = ("hovered", Fixture::hover);
/// Pointer held down over it.
pub const PRESSED: State = ("pressed", Fixture::press);
/// Keyboard focus.
pub const FOCUSED: State = ("focused", Fixture::focus);
/// Disabled.
pub const DISABLED: State = ("disabled", |f| f.set_disabled(true));
/// In an inactive window.
pub const INACTIVE: State = ("inactive", |f| f.set_window_focused(false));

/// Renders `make()` in every state × light/dark × scale 1 and 2, in a `size` window, and
/// checks each against the golden `{name}-{state}-{light|dark}@{1|2}x`. `theme` sets
/// everything but the appearance (accent, contrast, motion). Reports every mismatch, not
/// only the first; `cargo xtask golden --bless` rewrites them.
pub fn check_states<M>(
    goldens: &Goldens,
    name: &str,
    size: (u32, u32),
    theme: Theme,
    states: &[State],
    make: impl Fn() -> Element<M>,
) -> Result<(), Error> {
    check(goldens, name, theme, states, |theme, scale| {
        Fixture::new(make(), theme, scale, size)
    })
}

/// [`check_states`] for a whole window, chrome and all ([`Fixture::window`]), at its own
/// size.
pub fn check_window_states<M>(
    goldens: &Goldens,
    name: &str,
    theme: Theme,
    states: &[State],
    make: impl Fn() -> Window<M>,
) -> Result<(), Error> {
    check(goldens, name, theme, states, |theme, scale| {
        Fixture::window(make(), theme, scale)
    })
}

fn check(
    goldens: &Goldens,
    name: &str,
    theme: Theme,
    states: &[State],
    fixture: impl Fn(Theme, f64) -> Fixture,
) -> Result<(), Error> {
    let mut mismatches = Vec::new();
    for &(state, enter) in states {
        for (appearance, a) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
            for scale in [1_u8, 2] {
                let mut theme = theme;
                theme.variant.appearance = appearance;
                let mut fixture = fixture(theme, f64::from(scale));
                enter(&mut fixture);
                let pixmap = fixture.render()?;
                if let Err(m) = goldens.check(&format!("{name}-{state}-{a}@{scale}x"), pixmap) {
                    mismatches.push(m);
                }
            }
        }
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(Error::Mismatches(mismatches))
    }
}
