//! The Elm-style app model (ADR 0035): the app is the model, widgets send messages, and
//! `update` changes the model and, through handles, the widgets.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::Arc;

use masonry_core::app::RenderRoot;
use masonry_core::core::{ErasedAction, FromDynWidget, Widget, WidgetId, WidgetMut};
use masonry_core::peniko::Blob;
use neue_platform::{WindowId, WindowSpec};
use neue_theme::Theme;

use crate::element::{Element, Handle, Handler, Handlers};
use crate::task::{Mailbox, Sender, Task};
use crate::theme;

/// An app: its state is the model, [`Application::Message`] is everything that can happen
/// to it, and [`Application::update`] is the only place the state changes.
///
/// Widgets send messages (a button's `on_press`), and so do background tasks
/// ([`Cx::spawn`]) and other threads ([`Cx::sender`]). `update` runs on the UI thread, one
/// message at a time; it changes the model and updates widgets through [`Handle`]s.
pub trait Application {
    /// What can happen to the app.
    type Message: Send + 'static;

    /// Called once, when the platform is ready. Open the first window here.
    fn start(&mut self, cx: &mut Cx<'_, Self::Message>);

    /// Handles one message.
    fn update(&mut self, cx: &mut Cx<'_, Self::Message>, message: Self::Message);

    /// The user asked to close `window`. Closes it by default; the app ends when its last
    /// window closes.
    fn close_requested(&mut self, cx: &mut Cx<'_, Self::Message>, window: WindowId) {
        cx.close_window(window);
    }
}

/// App-wide settings for [`run`].
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// Colours and motion for every widget.
    pub theme: Theme,
    /// Reverse-DNS app id (`org.neueos.Calculator`) for windows that don't set their own.
    pub app_id: String,
}

/// Errors that end [`run`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Platform(#[from] neue_platform::Error),
}

/// Runs `app` until it calls [`Cx::exit`] or its last window closes.
pub fn run<A: Application>(app: A, settings: Settings) -> Result<(), Error> {
    let options = neue_platform::Options {
        default_properties: Arc::new(theme::default_properties(settings.theme)),
        font: Some(Blob::new(Arc::new(neue_paint::text::ui_font_data()))),
    };
    let mut runner = Runner::new(app, settings);
    neue_platform::run(&mut runner, options)?;
    Ok(())
}

/// A window to open.
pub struct Window<M> {
    title: String,
    size: (u32, u32),
    root: Element<M>,
    app_id: Option<String>,
    parent: Option<WindowId>,
    modal: bool,
}

impl<M> std::fmt::Debug for Window<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("title", &self.title)
            .field("size", &self.size)
            .field("parent", &self.parent)
            .field("modal", &self.modal)
            .finish_non_exhaustive()
    }
}

impl<M> Window<M> {
    /// A window of `size` logical pixels showing `root`.
    pub fn new(title: impl Into<String>, size: (u32, u32), root: impl Into<Element<M>>) -> Self {
        Self {
            title: title.into(),
            size,
            root: root.into(),
            app_id: None,
            parent: None,
            modal: false,
        }
    }

    /// Overrides [`Settings::app_id`] for this window.
    #[must_use]
    pub fn app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = Some(app_id.into());
        self
    }

    /// Keeps the window above `parent` (a dialog).
    #[must_use]
    pub fn parent(mut self, parent: WindowId) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Blocks input to the parent while open. Needs [`Window::parent`].
    #[must_use]
    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }
}

/// What [`Cx`] needs from the platform; `neue_platform::Ctx` in an app, a fake in tests.
pub(crate) trait Host {
    fn open_window(&mut self, spec: WindowSpec) -> WindowId;
    fn close_window(&mut self, window: WindowId);
    fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot>;
    fn windows(&self) -> Vec<WindowId>;
    fn exit(&mut self);
}

impl Host for neue_platform::Ctx<'_> {
    fn open_window(&mut self, spec: WindowSpec) -> WindowId {
        neue_platform::Ctx::open_window(self, spec)
    }
    fn close_window(&mut self, window: WindowId) {
        neue_platform::Ctx::close_window(self, window);
    }
    fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot> {
        neue_platform::Ctx::render_root(self, window)
    }
    fn windows(&self) -> Vec<WindowId> {
        neue_platform::Ctx::windows(self).collect()
    }
    fn exit(&mut self) {
        neue_platform::Ctx::exit(self);
    }
}

/// neue-kit's side of the app: message handlers, the message queue and the mailbox.
pub(crate) struct Kit<M> {
    settings: Settings,
    /// Widget → (its window, what its actions mean).
    handlers: HashMap<WidgetId, (WindowId, Handler<M>)>,
    /// Messages for `update`, in order.
    queue: VecDeque<M>,
    mailbox: Mailbox<M>,
}

impl<M> std::fmt::Debug for Kit<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kit")
            .field("handlers", &self.handlers.len())
            .field("queue", &self.queue.len())
            .finish_non_exhaustive()
    }
}

/// What an [`Application`] can do from `start` and `update`.
pub struct Cx<'a, M> {
    host: &'a mut dyn Host,
    kit: &'a mut Kit<M>,
}

impl<M> std::fmt::Debug for Cx<'_, M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cx")
            .field("windows", &self.host.windows())
            .field("kit", &self.kit)
            .finish()
    }
}

impl<M> Cx<'_, M> {
    /// Opens a window. It appears after the current callback returns; handles to its
    /// widgets work from then on.
    pub fn open_window(&mut self, window: Window<M>) -> WindowId {
        let (root, handlers) = window.root.into_parts();
        let mut spec = WindowSpec::new(window.title, window.size, root);
        spec.app_id = window
            .app_id
            .unwrap_or_else(|| self.kit.settings.app_id.clone());
        spec.background = self.kit.settings.theme.colors().window_background;
        spec.parent = window.parent;
        spec.modal = window.modal;
        let id = self.host.open_window(spec);
        self.register_handlers(id, handlers);
        id
    }

    /// Closes `window`. Unknown or closed ids are ignored.
    pub fn close_window(&mut self, window: WindowId) {
        self.host.close_window(window);
        self.kit.handlers.retain(|_, (w, _)| *w != window);
    }

    /// Open windows, in no particular order.
    pub fn windows(&self) -> Vec<WindowId> {
        self.host.windows()
    }

    /// Ends the app after the current callback.
    pub fn exit(&mut self) {
        self.host.exit();
    }

    /// The app's theme.
    pub fn theme(&self) -> Theme {
        self.kit.settings.theme
    }

    /// Queues `message` for `update`, after the messages already queued.
    pub fn send(&mut self, message: M) {
        self.kit.queue.push_back(message);
    }

    /// The open window `handle`'s widget is in.
    pub(crate) fn window_of<W>(&mut self, handle: &Handle<W>) -> Option<WindowId> {
        let id = handle.id()?;
        self.host
            .windows()
            .into_iter()
            .find(|&w| self.host.render_root(w).is_some_and(|r| r.has_widget(id)))
    }

    /// Runs `f` on `handle`'s widget, if it's in an open window.
    pub(crate) fn edit<W, R>(
        &mut self,
        handle: &Handle<impl Sized>,
        f: impl FnOnce(WidgetMut<'_, W>) -> R,
    ) -> Option<R>
    where
        W: Widget + FromDynWidget,
    {
        let id = handle.id()?;
        let window = self.window_of(handle)?;
        let root = self.host.render_root(window)?;
        Some(root.edit_widget(id, |mut widget| f(widget.downcast::<W>())))
    }

    pub(crate) fn register_handlers(&mut self, window: WindowId, handlers: Handlers<M>) {
        for (id, handler) in handlers {
            self.kit.handlers.insert(id, (window, handler));
        }
    }

    /// Drops the handlers of `window`'s widgets that left its tree.
    pub(crate) fn prune_handlers(&mut self, window: WindowId) {
        let Some(root) = self.host.render_root(window) else {
            return;
        };
        self.kit
            .handlers
            .retain(|id, (w, _)| *w != window || root.has_widget(*id));
    }
}

impl<M: Send + 'static> Cx<'_, M> {
    /// A sender for other threads (or async runtimes) to send messages with.
    pub fn sender(&self) -> Sender<M> {
        self.kit.mailbox.sender()
    }

    /// Runs blocking `work` on a background thread; its result is sent as a message.
    /// `work` can send progress messages through the [`Sender`] it's given, and should
    /// check [`Sender::is_cancelled`] if it runs long.
    pub fn spawn(&mut self, work: impl FnOnce(&Sender<M>) -> M + Send + 'static) -> Task {
        self.kit.mailbox.spawn(work)
    }

    /// Runs `future` to completion on a background thread; its output is sent as a
    /// message. The thread polls it with a minimal executor: futures that need a specific
    /// runtime (tokio) belong on that runtime, sending through [`Cx::sender`].
    pub fn spawn_future(&mut self, future: impl Future<Output = M> + Send + 'static) -> Task {
        self.kit.mailbox.spawn_future(future)
    }
}

/// Drives an [`Application`] from the platform's callbacks.
pub(crate) struct Runner<A: Application> {
    app: A,
    /// Created at start, when the platform hands out a waker.
    kit: Option<Kit<A::Message>>,
    settings: Option<Settings>,
}

impl<A: Application> std::fmt::Debug for Runner<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("kit", &self.kit)
            .finish_non_exhaustive()
    }
}

impl<A: Application> Runner<A> {
    pub(crate) fn new(app: A, settings: Settings) -> Self {
        Self {
            app,
            kit: None,
            settings: Some(settings),
        }
    }

    pub(crate) fn start(&mut self, host: &mut dyn Host, wake: crate::task::Wake) {
        let settings = self.settings.take().unwrap_or_default();
        let kit = self.kit.insert(Kit {
            settings,
            handlers: HashMap::new(),
            queue: VecDeque::new(),
            mailbox: Mailbox::new(wake),
        });
        self.app.start(&mut Cx { host, kit });
        self.drain(host);
    }

    pub(crate) fn action(&mut self, host: &mut dyn Host, widget: WidgetId, action: ErasedAction) {
        let Some(kit) = &mut self.kit else { return };
        let Some((_, handler)) = kit.handlers.get_mut(&widget) else {
            tracing::trace!(?action, "action without a handler");
            return;
        };
        if let Some(message) = handler(action) {
            kit.queue.push_back(message);
        }
        self.drain(host);
    }

    pub(crate) fn close_requested(&mut self, host: &mut dyn Host, window: WindowId) {
        let Some(kit) = &mut self.kit else { return };
        self.app.close_requested(&mut Cx { host, kit }, window);
        self.drain(host);
    }

    /// Feeds every queued and mailed message to `update`, including ones `update` sends.
    pub(crate) fn drain(&mut self, host: &mut dyn Host) {
        let Some(kit) = &mut self.kit else { return };
        while let Some(message) = kit.queue.pop_front().or_else(|| kit.mailbox.try_recv()) {
            self.app.update(&mut Cx { host, kit }, message);
        }
    }
}

impl<A: Application> neue_platform::App for Runner<A> {
    fn on_start(&mut self, cx: &mut neue_platform::Ctx<'_>) {
        let waker = cx.waker();
        self.start(cx, Arc::new(move || waker.wake()));
    }

    fn on_action(
        &mut self,
        cx: &mut neue_platform::Ctx<'_>,
        _window: WindowId,
        widget: WidgetId,
        action: ErasedAction,
    ) {
        self.action(cx, widget, action);
    }

    fn on_close_requested(&mut self, cx: &mut neue_platform::Ctx<'_>, window: WindowId) {
        self.close_requested(cx, window);
    }

    fn on_wake(&mut self, cx: &mut neue_platform::Ctx<'_>) {
        self.drain(cx);
    }
}

#[cfg(test)]
impl<A: Application> Runner<A> {
    pub(crate) fn app(&self) -> &A {
        &self.app
    }

    pub(crate) fn handler_count(&self) -> usize {
        self.kit.as_ref().map_or(0, |kit| kit.handlers.len())
    }

    /// Delivers `message` as if a widget had sent it.
    pub(crate) fn send(&mut self, host: &mut dyn Host, message: A::Message) {
        if let Some(kit) = &mut self.kit {
            kit.queue.push_back(message);
        }
        self.drain(host);
    }
}
