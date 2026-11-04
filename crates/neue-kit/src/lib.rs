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

mod app;
mod element;
mod task;
mod theme;
mod widgets;

#[cfg(test)]
mod tests;

pub use app::{Application, Cx, Error, Settings, Window, run};
pub use element::{Element, Handle};
pub use neue_platform::WindowId;
pub use task::{Sender, Task};
pub use widgets::{Align, Button, Label, Stack, TextStyle};

/// Design tokens (colours, metrics, type scale, motion).
pub use neue_theme as tokens;
pub use neue_theme::{Accent, Appearance, Contrast, Theme, Variant};
