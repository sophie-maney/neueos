//! neueOS's toolkit (PLAN.md §3.2, ADR 0035).
//!
//! An app implements [`Application`]: its state is the model, its
//! [`Message`](Application::Message) type lists what can happen, and
//! [`update`](Application::update) handles one message at a time. Widgets are built
//! from builders ([`Label`], [`Button`], [`Stack`]) that send messages; [`Handle`]s let
//! `update` change widgets afterwards. Blocking or async work runs in the background with
//! [`Cx::spawn`] and [`Cx::spawn_future`] and reports back with messages.
//!
//! ```no_run
//! use neue_kit::{Application, Button, Cx, Handle, Label, Stack, Window};
//!
//! #[derive(Default)]
//! struct Counter {
//!     count: i64,
//!     value: Handle<Label>,
//! }
//!
//! #[derive(Clone)]
//! enum Message {
//!     Increment,
//! }
//!
//! impl Application for Counter {
//!     type Message = Message;
//!
//!     fn start(&mut self, cx: &mut Cx<'_, Message>) {
//!         let root = Stack::column()
//!             .with(Label::new("0").bind(&mut self.value))
//!             .with(Button::new("Add one").on_press(Message::Increment));
//!         cx.open_window(Window::new("Counter", (200, 100), root));
//!     }
//!
//!     fn update(&mut self, cx: &mut Cx<'_, Message>, message: Message) {
//!         match message {
//!             Message::Increment => self.count += 1,
//!         }
//!         self.value.set_text(cx, self.count.to_string());
//!     }
//! }
//!
//! neue_kit::run(Counter::default(), neue_kit::Settings::default()).unwrap();
//! ```
//!
//! Masonry (`masonry_core`) is the widget tree underneath; none of its types are part of
//! this API, and app crates must not depend on it (ADR 0002 §4, checked by
//! `cargo xtask check`).

mod anim;
mod app;
mod date;
mod element;
mod menu;
mod menubar;
mod task;
#[cfg(feature = "testing")]
pub mod testing;
mod theme;
mod widgets;

#[cfg(test)]
mod tests;

pub use app::{Application, Cx, Error, Settings, Window, run};
pub use date::Date;
pub use element::{Element, Handle};
pub use menu::{Menu, MenuItem, MenuToggle, Shortcut, Signal};
pub use neue_paint::icon::{Glyph, Icon, IconError, IconPaint, VectorIcon};
pub use neue_paint::materials::{DividerStyle, Edge};
pub use neue_platform::WindowId;
pub use task::{Sender, Task};
pub use widgets::{
    Align, BevelButton, Button, CalendarPicker, Cell, CheckState, Checkbox, Collection, ColorWell,
    Column, ComboBox, ContextMenu, DatePicker, DisplayMode, Entry, IconImage, Item, Label,
    MaterialTile, Outline, Pane, PathControl, PopUpButton, Popover, ProgressBar, PullDownButton,
    Radio, ScrollAxes, ScrollView, Segmented, Sheet, Slider, SortOrder, SourceList, Specimen,
    Spinner, SplitView, Stack, Stepper, TabView, Table, TextField, TextStyle, TextView, Toolbar,
    ToolbarButton, VirtualGrid, VirtualList, Zoom,
};

/// What only the desktop shell uses (task 3.2a, ADR 0067): layer surfaces
/// ([`Window::layer`]), outputs ([`Application::output`]) and the compositor's window list
/// and window requests ([`Application::shell`], [`Cx::shell`]).
pub mod desktop {
    pub use crate::widgets::{Desktop, ExtraIcon, MenuBar, MenuTitle, NetworkIcon, Osd};
    pub use neue_paint::materials::desktop::{Fit, Picture};
    pub use neue_paint::materials::osd::Glyph as OsdGlyph;
    pub use neue_platform::desktop::*;
    pub use neue_platform::{Anchor, LayerKeyboard, LayerSpec, Margin, ShellLayer};
    pub use neue_theme::Backdrop;
}

/// Design tokens (colours, metrics, type scale, motion).
pub use neue_theme as tokens;
pub use neue_theme::{Accent, Appearance, Contrast, Theme, Variant};
