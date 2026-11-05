//! The Elm-style app model (ADR 0035): the app is the model, widgets send messages, and
//! `update` changes the model and, through handles, the widgets.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::sync::Arc;

use masonry_core::app::RenderRoot;
use masonry_core::core::{ErasedAction, FromDynWidget, NewWidget, Widget, WidgetId, WidgetMut};
use masonry_core::peniko::{Blob, Color};
use neue_platform::{Decorations, MenuAddress, WindowId, WindowSpec};
use neue_theme::Theme;

use crate::element::{Element, Handle, Handler, Wiring};
use crate::menu::Menu;
use crate::menubar::MenuBars;
use crate::task::{Mailbox, Sender, Task};
use crate::theme;
use crate::widgets::{
    self, ButtonWidget, Chrome, ChromeKind, Popover, PopoverWidget, Pressed, Rewired, RootAction,
    RootWidget, RowsWidget, Sheet, SheetButtons, SheetKey,
};

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
    menu_bar: Option<Menu<M>>,
    /// Draws its own chrome (ADR 0055).
    chrome: bool,
    /// A HUD panel: its chrome is the dark HUD's.
    hud: bool,
    toolbar: Option<Element<M>>,
}

impl<M> std::fmt::Debug for Window<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("title", &self.title)
            .field("size", &self.size)
            .field("parent", &self.parent)
            .field("modal", &self.modal)
            .field("menu_bar", &self.menu_bar)
            .field("chrome", &self.chrome)
            .field("hud", &self.hud)
            .field("toolbar", &self.toolbar.is_some())
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
            menu_bar: None,
            chrome: false,
            hud: false,
            toolbar: None,
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

    /// Draws the window's own title bar, with the traffic lights and the title, instead of
    /// the compositor's (client-side chrome, ADR 0055). It looks the same: the bar comes
    /// from the same `neue-paint` code. Drag it to move the window, double-click it to
    /// zoom. [`Window::toolbar`] implies it.
    #[must_use]
    pub fn chrome(mut self) -> Self {
        self.chrome = true;
        self
    }

    /// A unified title bar and toolbar: `toolbar` sits under the title row, inside the
    /// chrome's gradient, across the window. Bare areas of the toolbar move the window like
    /// the title row does. Implies [`Window::chrome`].
    #[must_use]
    pub fn toolbar(mut self, toolbar: impl Into<Element<M>>) -> Self {
        self.chrome = true;
        self.toolbar = Some(toolbar.into());
        self
    }

    /// A HUD panel: a dark, small window for tools and inspectors, with a small title bar
    /// that has a close button and the title and nothing else. The HUD material fills the
    /// whole window (it is dark in both appearances), and there is no toolbar:
    /// [`Window::toolbar`] is ignored. Implies [`Window::chrome`].
    ///
    /// Give it a [`Window::parent`] to make it a utility window: the compositor keeps it
    /// above the parent, and the parent keeps its focused look while the HUD has the
    /// keyboard.
    ///
    /// The window's content is drawn as usual, so it is the app's to make it readable on
    /// a dark panel: [`Label::on_hud`] does it for text. Dark controls come with a theme
    /// override for a subtree, later.
    #[must_use]
    pub fn hud(mut self) -> Self {
        self.chrome = true;
        self.hud = true;
        self
    }

    /// The window's menu bar: its items are the menu titles, each a submenu. Shown in the
    /// global menu bar on neueOS (exported over D-Bus, ADR 0047); not shown on macOS
    /// hosts yet. Change it with [`Cx::set_menu_bar`].
    #[must_use]
    pub fn menu_bar(mut self, menu: Menu<M>) -> Self {
        self.menu_bar = Some(menu);
        self
    }
}

/// A window's root widget around `content`, with chrome if it has some, and the wiring of
/// the content and of the toolbar.
fn root_widget<M>(
    title: &str,
    content: Element<M>,
    chrome: Option<ChromeKind>,
    toolbar: Option<Element<M>>,
) -> (RootWidget, Wiring<M>, Option<Wiring<M>>) {
    let (content, wiring) = content.into_parts();
    let (toolbar, toolbar_wiring) = toolbar.map(Element::into_parts).unzip();
    let root = match chrome {
        Some(kind) => {
            RootWidget::with_chrome(content, Chrome::new(kind, title.to_owned(), toolbar))
        }
        None => RootWidget::new(content),
    };
    (root, wiring, toolbar_wiring)
}

impl<M> Window<M> {
    /// Which chrome the window draws, if any.
    fn chrome_kind(&self) -> Option<ChromeKind> {
        match (self.chrome, self.hud) {
            (_, true) => Some(ChromeKind::Hud),
            (true, false) => Some(ChromeKind::Standard),
            (false, false) => None,
        }
    }
}

#[cfg(feature = "testing")]
impl<M> Window<M> {
    /// The root widget the window opens with, and its size; message handlers are dropped.
    pub(crate) fn into_root(self) -> (RootWidget, (u32, u32)) {
        let kind = self.chrome_kind();
        let (root, ..) = root_widget(&self.title, self.root, kind, self.toolbar);
        (root, self.size)
    }
}

/// Sets each of `parents` to paint its focused look while one of its HUD windows has the
/// keyboard (a utility window doesn't take the parent's activation look).
fn refresh_hud_parent_look<M>(
    host: &mut dyn Host,
    kit: &Kit<M>,
    parents: impl IntoIterator<Item = WindowId>,
) {
    for parent in parents {
        let keep = kit
            .huds
            .iter()
            .any(|(hud, p)| *p == parent && kit.huds_focused.contains(hud));
        let (Some(&root_id), Some(root)) = (kit.roots.get(&parent), host.render_root(parent))
        else {
            continue;
        };
        if root.has_widget(root_id) {
            root.edit_widget(root_id, |mut r| {
                RootWidget::set_keep_active(&mut r.downcast(), keep);
            });
        }
    }
}

/// What [`Cx`] needs from the platform; `neue_platform::Ctx` in an app, a fake in tests.
pub(crate) trait Host {
    fn open_window(&mut self, spec: WindowSpec) -> WindowId;
    fn close_window(&mut self, window: WindowId);
    fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot>;
    fn windows(&self) -> Vec<WindowId>;
    fn exit(&mut self);
    fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>);
    /// Whether modal child windows exist ([`neue_platform::Ctx::has_modal_windows`]):
    /// sheets open as windows of their own, or else inside their parent.
    fn has_modal_windows(&self) -> bool;
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
    fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>) {
        neue_platform::Ctx::set_menu_address(self, window, address);
    }
    fn has_modal_windows(&self) -> bool {
        neue_platform::Ctx::has_modal_windows(self)
    }
}

/// A sheet open on a window.
#[derive(Debug)]
struct OpenSheet {
    /// The sheet's widget.
    widget: WidgetId,
    /// Its own window, where modal windows exist; `None` when the parent draws it.
    window: Option<WindowId>,
    /// The buttons Return and Escape press.
    keys: SheetButtons,
    /// What had keyboard focus in the parent when it opened (drawn in the parent only).
    return_focus: Option<WidgetId>,
}

/// neue-kit's side of the app: message handlers, the message queue and the mailbox.
pub(crate) struct Kit<M> {
    settings: Settings,
    /// Widget → (its window, what its actions mean).
    handlers: HashMap<WidgetId, (WindowId, Handler<M>)>,
    /// Each window's default button, which Return presses.
    defaults: HashMap<WindowId, WidgetId>,
    /// Each window's root widget.
    roots: HashMap<WindowId, WidgetId>,
    /// Open sheets, by the window they are on.
    sheets: HashMap<WindowId, OpenSheet>,
    /// Open HUD windows that have a parent, and the parent.
    huds: HashMap<WindowId, WindowId>,
    /// The HUD windows that have the keyboard.
    huds_focused: HashSet<WindowId>,
    /// Messages for `update`, in order.
    queue: VecDeque<M>,
    mailbox: Mailbox<M>,
    menu_bars: MenuBars<M>,
}

impl<M: Send + 'static> Kit<M> {
    /// The next message for `update`: queued ones first, then mailed ones, then chosen
    /// menu-bar items.
    fn next_message(&mut self) -> Option<M> {
        if let Some(message) = self.queue.pop_front().or_else(|| self.mailbox.try_recv()) {
            return Some(message);
        }
        self.queue.extend(self.menu_bars.take_messages());
        self.queue.pop_front()
    }
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
        let kind = window.chrome_kind();
        let (root, wiring, toolbar_wiring) =
            root_widget(&window.title, window.root, kind, window.toolbar);
        let root = NewWidget::new(root);
        let root_id = root.id();
        let mut spec = WindowSpec::new(window.title, window.size, root.erased());
        spec.focus_fallback = Some(root_id);
        spec.app_id = window
            .app_id
            .unwrap_or_else(|| self.kit.settings.app_id.clone());
        spec.parent = window.parent;
        spec.modal = window.modal;
        if kind.is_some() {
            // The chrome's top corners are rounded (a HUD's all of them); the root paints
            // everything else.
            spec.decorations = Decorations::Client;
            spec.background = Color::TRANSPARENT;
        } else {
            spec.background = self.kit.settings.theme.colors().window_background;
        }
        let id = self.host.open_window(spec);
        self.kit.roots.insert(id, root_id);
        if let (Some(ChromeKind::Hud), Some(parent)) = (kind, window.parent) {
            self.kit.huds.insert(id, parent);
        }
        self.register_wiring(id, wiring);
        if let Some(wiring) = toolbar_wiring {
            self.register_wiring(id, wiring);
        }
        if let Some(menu) = window.menu_bar {
            self.set_menu_bar(id, menu);
        }
        id
    }

    /// Replaces `window`'s menu bar (see [`Window::menu_bar`]); an empty menu removes it.
    pub fn set_menu_bar(&mut self, window: WindowId, menu: Menu<M>) {
        self.kit.menu_bars.set(self.host, window, menu);
    }

    /// Closes `window`, and a sheet open on it. Unknown or closed ids are ignored. Closing a
    /// sheet's own window closes the sheet ([`Cx::close_sheet`]).
    pub fn close_window(&mut self, window: WindowId) {
        if let Some(parent) = self.sheet_parent(window) {
            return self.close_sheet(parent);
        }
        self.close_sheet(window);
        self.forget_window(window);
    }

    /// Closes `window` and forgets everything about it.
    fn forget_window(&mut self, window: WindowId) {
        // HUD windows don't outlive their parent.
        let huds: Vec<WindowId> = self
            .kit
            .huds
            .iter()
            .filter_map(|(&hud, &parent)| (parent == window).then_some(hud))
            .collect();
        for hud in huds {
            self.forget_window(hud);
        }
        self.host.close_window(window);
        self.kit.menu_bars.remove(window);
        self.kit.handlers.retain(|_, (w, _)| *w != window);
        self.kit.defaults.remove(&window);
        self.kit.roots.remove(&window);
        // A HUD window going away gives its parent back its own look; a parent going away
        // leaves its HUD windows without one.
        self.kit.huds_focused.remove(&window);
        if let Some(parent) = self.kit.huds.remove(&window) {
            refresh_hud_parent_look(self.host, self.kit, [parent]);
        }
    }

    /// Shows `sheet` on `window`, replacing a sheet already open there (without sending its
    /// messages). `window` must be open: a window opened in the same callback isn't yet.
    ///
    /// The window takes no pointer input while the sheet is open, and keyboard focus stays
    /// in the sheet. Where the platform has modal windows (Wayland), the sheet is a modal
    /// window of its own that the compositor attaches to `window`; elsewhere (macOS)
    /// `window` draws it over its content, under its title bar and toolbar.
    pub fn show_sheet(&mut self, window: WindowId, sheet: Sheet<M>)
    where
        M: Clone + 'static,
    {
        let Some(&root_id) = self.kit.roots.get(&window) else {
            tracing::warn!(%window, "show_sheet on a window that isn't open");
            return;
        };
        if self.host.render_root(window).is_none() {
            tracing::warn!(%window, "show_sheet on a window that isn't open yet");
            return;
        }
        self.close_sheet(window);
        let parts = sheet.into_parts();
        let widget = parts.widget.id();
        if self.host.has_modal_windows() {
            let root = NewWidget::new(RootWidget::new(parts.widget.erased()));
            let sheet_root = root.id();
            let mut spec = WindowSpec::new(parts.title, parts.size, root.erased());
            spec.focus_fallback = Some(widget);
            spec.app_id = self.kit.settings.app_id.clone();
            spec.parent = Some(window);
            spec.modal = true;
            // No title bar: the sheet hangs from its parent's.
            spec.decorations = Decorations::Client;
            spec.background = self.kit.settings.theme.colors().window_background;
            let own = self.host.open_window(spec);
            self.kit.roots.insert(own, sheet_root);
            self.register_wiring(own, parts.wiring);
            if let Some(root) = self.host.render_root(window) {
                root.edit_widget(root_id, |mut r| {
                    RootWidget::attach_sheet(&mut r.downcast(), None);
                });
            }
            self.kit.sheets.insert(
                window,
                OpenSheet {
                    widget,
                    window: Some(own),
                    keys: parts.keys,
                    return_focus: None,
                },
            );
        } else {
            let default = self.kit.defaults.get(&window).copied();
            let Some(root) = self.host.render_root(window) else {
                return;
            };
            let return_focus = root.focused_widget();
            root.edit_widget(root_id, |mut r| {
                RootWidget::attach_sheet(&mut r.downcast(), Some(parts.widget));
            });
            root.set_focus_fallback(Some(widget));
            root.focus_on(Some(widget));
            // The window's default button rests while the sheet's is in charge.
            set_default_look(root, default, false);
            self.register_wiring(window, parts.wiring);
            self.kit.sheets.insert(
                window,
                OpenSheet {
                    widget,
                    window: None,
                    keys: parts.keys,
                    return_focus,
                },
            );
        }
    }

    /// Closes the sheet open on `window`, if there is one, without sending its messages.
    /// Keyboard focus goes back to where it was in `window`.
    pub fn close_sheet(&mut self, window: WindowId) {
        let Some(open) = self.kit.sheets.remove(&window) else {
            return;
        };
        if let Some(own) = open.window {
            self.forget_window(own);
        }
        let (Some(&root_id), default) = (
            self.kit.roots.get(&window),
            self.kit.defaults.get(&window).copied(),
        ) else {
            return;
        };
        let Some(root) = self.host.render_root(window) else {
            return;
        };
        if root.has_widget(root_id) {
            root.edit_widget(root_id, |mut r| RootWidget::detach_sheet(&mut r.downcast()));
        }
        if open.window.is_none() {
            root.set_focus_fallback(Some(root_id));
            root.focus_on(open.return_focus.filter(|&id| root.has_widget(id)));
            set_default_look(root, default, true);
            self.prune_handlers(window);
        }
    }

    /// The window a sheet with its own window `window` is on.
    fn sheet_parent(&self, window: WindowId) -> Option<WindowId> {
        self.kit
            .sheets
            .iter()
            .find_map(|(&parent, open)| (open.window == Some(window)).then_some(parent))
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

    /// Switches every window, and windows opened later, to `theme`. Widgets repaint (and
    /// re-lay out) with its colours and motion.
    pub fn set_theme(&mut self, theme: Theme) {
        if self.kit.settings.theme == theme {
            return;
        }
        self.kit.settings.theme = theme;
        let props = Arc::new(theme::default_properties(theme));
        for window in self.host.windows() {
            if let Some(root) = self.host.render_root(window) {
                root.set_default_properties(Arc::clone(&props));
            }
        }
    }

    /// Queues `message` for `update`, after the messages already queued.
    pub fn send(&mut self, message: M) {
        self.kit.queue.push_back(message);
    }

    /// Shows `popover` next to `anchor`'s widget, replacing one already shown there.
    ///
    /// A popover closes when the anchor is pressed again: if the last press outside it was
    /// on the anchor (a button that shows it, pressed again), this closes it, sending its
    /// `on_close`, instead of showing a new one.
    pub fn show_popover<W>(&mut self, anchor: &Handle<W>, popover: Popover<M>)
    where
        M: Clone + 'static,
    {
        let (Some(id), Some(window)) = (anchor.id(), self.window_of(anchor)) else {
            return;
        };
        // Forget the handlers of popovers that closed.
        self.prune_handlers(window);
        let Some(root) = self.host.render_root(window) else {
            return;
        };
        if let Some(open) = Self::shown_popover(root, id) {
            let toggled = root.edit_widget(open, |mut p| {
                let mut p = p.downcast::<PopoverWidget>();
                PopoverWidget::take_anchor_press(&mut p).then(|| PopoverWidget::close(&mut p, true))
            });
            if let Some(focus) = toggled {
                if focus.is_some() {
                    root.focus_on(focus);
                }
                return;
            }
        }
        let (widget, wiring) = popover.into_parts();
        root.edit_widget(id, |mut owner| widgets::popover::show(&mut owner, widget));
        self.register_wiring(window, wiring);
    }

    /// Closes the popover shown next to `anchor`'s widget, if there is one. Its `on_close`
    /// is not sent.
    pub fn close_popover<W>(&mut self, anchor: &Handle<W>) {
        let (Some(id), Some(window)) = (anchor.id(), self.window_of(anchor)) else {
            return;
        };
        let Some(root) = self.host.render_root(window) else {
            return;
        };
        if let Some(open) = Self::shown_popover(root, id) {
            let focus = root.edit_widget(open, |mut p| {
                PopoverWidget::close(&mut p.downcast::<PopoverWidget>(), false)
            });
            if focus.is_some() {
                root.focus_on(focus);
            }
        }
    }

    /// The popover open on the widget `owner`, if it is in the tree.
    fn shown_popover(root: &mut RenderRoot, owner: WidgetId) -> Option<WidgetId> {
        let open = root.edit_widget(owner, |w| widgets::popover::shown(&w))?;
        root.has_widget(open).then_some(open)
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

    pub(crate) fn register_wiring(&mut self, window: WindowId, wiring: Wiring<M>) {
        for (id, handler) in wiring.handlers {
            self.kit.handlers.insert(id, (window, handler));
        }
        if let Some(id) = wiring.default_button {
            match self.kit.defaults.entry(window) {
                Entry::Occupied(_) => {
                    tracing::warn!(%id, %window, "the window already has a default button");
                }
                Entry::Vacant(slot) => {
                    slot.insert(id);
                }
            }
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
        if self
            .kit
            .defaults
            .get(&window)
            .is_some_and(|id| !root.has_widget(*id))
        {
            self.kit.defaults.remove(&window);
        }
    }
}

/// Gives `button` (a window's default button) its default look, or takes it away while a
/// sheet's default button is in charge.
fn set_default_look(root: &mut RenderRoot, button: Option<WidgetId>, is_default: bool) {
    if let Some(id) = button.filter(|&id| root.has_widget(id)) {
        root.edit_widget(id, |mut b| {
            ButtonWidget::edit_default(&mut b.downcast(), is_default);
        });
    }
}

/// Moves keyboard focus to the next (`forward`) or previous control in the sheet `sheet`,
/// wrapping around: Tab never leaves a sheet.
fn cycle_focus(root: &mut RenderRoot, sheet: WidgetId, forward: bool) {
    let Some(widget) = root.get_widget(sheet) else {
        return;
    };
    let stops = widgets::focusable(widget);
    let (Some(&first), Some(&last)) = (stops.first(), stops.last()) else {
        return;
    };
    let at = root
        .focused_widget()
        .and_then(|f| stops.iter().position(|&s| s == f));
    let next = match (at, forward) {
        (None, true) => first,
        (None, false) => last,
        (Some(i), true) => stops.get(i + 1).copied().unwrap_or(first),
        (Some(i), false) => i.checked_sub(1).map_or(last, |i| stops[i]),
    };
    root.focus_on(Some(next));
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
    /// A peer-to-peer connection for menu bars instead of the session bus.
    #[cfg(all(test, target_os = "linux"))]
    pub(crate) dbus: Option<neue_dbus::zbus::blocking::Connection>,
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
            #[cfg(all(test, target_os = "linux"))]
            dbus: None,
        }
    }

    pub(crate) fn start(&mut self, host: &mut dyn Host, wake: crate::task::Wake) {
        let settings = self.settings.take().unwrap_or_default();
        let kit = self.kit.insert(Kit {
            settings,
            handlers: HashMap::new(),
            defaults: HashMap::new(),
            roots: HashMap::new(),
            sheets: HashMap::new(),
            huds: HashMap::new(),
            huds_focused: HashSet::new(),
            queue: VecDeque::new(),
            mailbox: Mailbox::new(wake.clone()),
            menu_bars: MenuBars::new(wake),
        });
        #[cfg(all(test, target_os = "linux"))]
        if let Some(conn) = self.dbus.take() {
            kit.menu_bars.use_connection(conn);
        }
        self.app.start(&mut Cx { host, kit });
        self.drain(host);
    }

    pub(crate) fn action(
        &mut self,
        host: &mut dyn Host,
        window: WindowId,
        widget: WidgetId,
        action: ErasedAction,
    ) {
        let action = match action.downcast::<RootAction>() {
            Ok(root) => return self.root_action(host, window, *root),
            Err(action) => action,
        };
        let action = match action.downcast::<SheetKey>() {
            Ok(key) => return self.sheet_key(host, window, widget, *key),
            Err(action) => action,
        };
        if action.is::<Rewired>() {
            return self.rewire(host, window, widget);
        }
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

    /// Handles a window root's action: Return presses the default button, window focus
    /// changes its look, and the chrome's close button asks the app to close the window.
    fn root_action(&mut self, host: &mut dyn Host, window: WindowId, action: RootAction) {
        let Some(kit) = &mut self.kit else { return };
        match action {
            RootAction::Close => self.close_requested(host, window),
            // While a sheet is open, Return is the sheet's.
            RootAction::DefaultKey if kit.sheets.contains_key(&window) => {}
            RootAction::DefaultKey => {
                if let Some(&id) = kit.defaults.get(&window) {
                    self.press(host, window, id);
                }
            }
            RootAction::WindowFocus(focused) => {
                if kit.huds.contains_key(&window) {
                    if focused {
                        kit.huds_focused.insert(window);
                    } else {
                        kit.huds_focused.remove(&window);
                    }
                    let parent = kit.huds[&window];
                    refresh_hud_parent_look(host, kit, [parent]);
                }
                // The window's default button, and that of a sheet drawn in it or that is
                // this window.
                let sheet = kit.sheets.iter().find_map(|(&parent, open)| {
                    (open.window.unwrap_or(parent) == window).then_some(open.keys.default)
                });
                let buttons = kit.defaults.get(&window).copied().into_iter();
                let Some(root) = host.render_root(window) else {
                    return;
                };
                for id in buttons.chain(sheet.flatten()) {
                    if root.has_widget(id) {
                        root.edit_widget(id, |mut button| {
                            ButtonWidget::edit_window_focus(&mut button.downcast());
                        });
                    }
                }
            }
        }
    }

    /// Presses the button `id` in `window` as Return presses a default button: if it is
    /// enabled, its message goes to `update`.
    fn press(&mut self, host: &mut dyn Host, window: WindowId, id: WidgetId) {
        let Some(kit) = &mut self.kit else { return };
        let enabled = host
            .render_root(window)
            .and_then(|root| root.get_widget(id))
            .is_some_and(|button| !button.ctx().is_disabled());
        if enabled
            && let Some((_, handler)) = kit.handlers.get_mut(&id)
            && let Some(message) = handler(Box::new(Pressed))
        {
            kit.queue.push_back(message);
        }
        self.drain(host);
    }

    /// Handles a sheet's key: Return and Escape press its default and cancel buttons, Tab
    /// cycles focus inside it.
    fn sheet_key(&mut self, host: &mut dyn Host, window: WindowId, sheet: WidgetId, key: SheetKey) {
        let Some(kit) = &mut self.kit else { return };
        let Some(open) = kit.sheets.values().find(|open| open.widget == sheet) else {
            return;
        };
        let keys = open.keys;
        match key {
            SheetKey::Default => {
                if let Some(id) = keys.default {
                    self.press(host, window, id);
                }
            }
            SheetKey::Cancel => {
                if let Some(id) = keys.cancel {
                    self.press(host, window, id);
                }
            }
            SheetKey::Tab { forward } => {
                if let Some(root) = host.render_root(window) {
                    cycle_focus(root, sheet, forward);
                }
            }
        }
    }

    /// A virtual list built or dropped rows: registers the new rows' message handlers and
    /// forgets the dropped rows' ones.
    fn rewire(&mut self, host: &mut dyn Host, window: WindowId, rows: WidgetId) {
        let Some(kit) = &mut self.kit else { return };
        let Some(root) = host.render_root(window) else {
            return;
        };
        if !root.has_widget(rows) {
            return;
        }
        let (wirings, dropped) =
            root.edit_widget(rows, |mut w| RowsWidget::take_wiring(&mut w.downcast()));
        let mut cx = Cx { host, kit };
        for wiring in wirings {
            match wiring.downcast::<Wiring<A::Message>>() {
                Ok(wiring) => cx.register_wiring(window, *wiring),
                Err(_) => tracing::warn!("a list row's wiring has the wrong message type"),
            }
        }
        if dropped {
            cx.prune_handlers(window);
        }
    }

    pub(crate) fn close_requested(&mut self, host: &mut dyn Host, window: WindowId) {
        let Some(kit) = &mut self.kit else { return };
        // A sheet's own window (the compositor asked): that's Escape, not the app's call.
        if let Some(open) = kit.sheets.values().find(|open| open.window == Some(window)) {
            if let Some(cancel) = open.keys.cancel {
                self.press(host, window, cancel);
            }
            return;
        }
        self.app.close_requested(&mut Cx { host, kit }, window);
        self.drain(host);
    }

    /// Feeds every queued and mailed message to `update`, including ones `update` sends.
    pub(crate) fn drain(&mut self, host: &mut dyn Host) {
        let Some(kit) = &mut self.kit else { return };
        while let Some(message) = kit.next_message() {
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
        window: WindowId,
        widget: WidgetId,
        action: ErasedAction,
    ) {
        self.action(cx, window, widget, action);
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

    pub(crate) fn app_mut(&mut self) -> &mut A {
        &mut self.app
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
