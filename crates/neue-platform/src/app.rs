//! The app-facing API both drivers share (ADR 0032 §3, ADR 0033).
//!
//! An [`App`] gets callbacks with a [`Ctx`], through which it opens and closes windows,
//! reaches their widget trees and ends the loop. The same app runs on the winit driver
//! (macOS) and the Wayland driver (Linux); [`crate::run`] picks the host's.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use masonry_core::app::RenderRoot;
use masonry_core::core::{DefaultProperties, ErasedAction, NewWidget, Widget, WidgetId};
use masonry_core::peniko::Color;

use crate::desktop::{OutputEvent, OutputId, OutputInfo, ShellEvent, ShellRequest};

/// Identifies a window for the lifetime of the driver. Never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(u64);

impl WindowId {
    /// An id made outside a driver, for fakes of the app API in tests (neue-kit's). Never
    /// mix such ids with a running driver's.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The number inside, unique for the driver's lifetime (for names such as D-Bus paths).
    pub const fn to_raw(self) -> u64 {
        self.0
    }
}

/// Where a window's menu bar is exported over D-Bus (`com.canonical.dbusmenu`), for the
/// global menu (ADR 0005, ADR 0047).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MenuAddress {
    /// The exporting connection's bus name (`:1.42`).
    pub service: String,
    /// The menu's object path.
    pub path: String,
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "window#{}", self.0)
    }
}

/// A window to open.
#[derive(Debug)]
pub struct WindowSpec {
    pub title: String,
    /// Reverse-DNS app id (`xdg_toplevel.set_app_id`). Unused on macOS.
    pub app_id: String,
    /// Initial logical size. For a layer surface, 0 on an axis anchored to both edges
    /// stretches it across the output.
    pub width: u32,
    pub height: u32,
    pub root: NewWidget<dyn Widget>,
    /// Cleared to before the widgets paint. Opaque, except for windows with
    /// [`Decorations::Client`], whose rounded top corners need alpha (ADR 0055).
    pub background: Color,
    pub kind: WindowKind,
    /// Who draws a toplevel's title bar. Ignored for layer surfaces.
    pub decorations: Decorations,
    /// The window this one belongs to (`xdg_toplevel.set_parent`): the compositor keeps it
    /// above its parent. Toplevels only; Wayland only for now.
    pub parent: Option<WindowId>,
    /// A modal dialog (xdg-dialog `set_modal`): its parent takes no input while it's open.
    /// Wayland only for now.
    pub modal: bool,
    /// The widget that gets keyboard events while no widget has focus
    /// (`RenderRoot::set_focus_fallback`). neue-kit points it at its window root, which
    /// turns Return into a press of the default button.
    pub focus_fallback: Option<WidgetId>,
    /// The widgets' default properties (the theme) for this window. `None` takes
    /// [`crate::Options::default_properties`], which is fixed when the driver starts: a
    /// window opened after the app switched themes would otherwise open in the old one.
    pub default_properties: Option<Arc<DefaultProperties>>,
    /// Where the chrome the window draws at its top ends, kept up to date by the widget
    /// that lays it out. The Wayland driver tells the compositor (`neue_window_v1`), which
    /// hangs sheets under it (ADR 0064). `None` for a window without chrome of its own.
    pub chrome_height: Option<ChromeHeight>,
    /// Ask the compositor to blur what is behind the window (task 3.5, ADR 0072). Ignored
    /// without [`Ctx::has_blur`]. The window should then paint a translucent material.
    pub blur: Option<Blur>,
}

/// A request to blur the background behind a whole window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blur {
    /// Radius of the window's rounded corners in logical pixels; the blur stays inside
    /// them. 0: a plain rectangle.
    pub corner_radius: f64,
}

impl WindowSpec {
    /// A toplevel window with a neutral grey background and an empty app id.
    pub fn new(title: impl Into<String>, size: (u32, u32), root: NewWidget<dyn Widget>) -> Self {
        Self {
            title: title.into(),
            app_id: String::new(),
            width: size.0,
            height: size.1,
            root,
            background: Color::from_rgb8(0xec, 0xec, 0xec),
            kind: WindowKind::Toplevel,
            decorations: Decorations::Server,
            parent: None,
            modal: false,
            focus_fallback: None,
            default_properties: None,
            chrome_height: None,
            blur: None,
        }
    }
}

/// The height of the chrome (title bar and toolbar) a window draws at its top, in logical
/// pixels from the top of the window, shared between the widget that lays the chrome out
/// and the driver (ADR 0064). Clones share the value.
#[derive(Debug, Clone, Default)]
pub struct ChromeHeight(Arc<AtomicU32>);

impl ChromeHeight {
    /// A height of 0 (no chrome yet).
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the chrome's laid-out height, rounded up to whole pixels so nothing hung
    /// under it covers its last row.
    pub fn set(&self, height: f64) {
        // Saturating: NaN and negative heights become 0, huge ones u32::MAX.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason above
        let px = height.ceil() as u32;
        self.0.store(px, Ordering::Relaxed);
    }

    /// The last height recorded.
    pub fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Who draws a toplevel's title bar (ADR 0055).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Decorations {
    /// The compositor (xdg-decoration, server-side by default); the native title bar on
    /// macOS.
    #[default]
    Server,
    /// The window draws its own chrome. On Wayland it asks for client-side decorations
    /// through xdg-decoration; on macOS the title bar is transparent and full-size, with
    /// the native title and buttons hidden, so the native shadow, corners and resizing stay.
    Client,
}

/// What kind of surface a window is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum WindowKind {
    /// An ordinary application window (xdg-toplevel, server-side title bar).
    #[default]
    Toplevel,
    /// A shell surface on a desktop layer (wlr-layer-shell): menu bar, dock, panels.
    /// Wayland only; the winit driver opens a toplevel instead.
    Layer(LayerSpec),
}

/// Placement of a layer-shell surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSpec {
    /// Tells the compositor what the surface is for (e.g. `neue-menubar`).
    pub namespace: String,
    pub layer: ShellLayer,
    pub anchor: Anchor,
    pub margin: Margin,
    /// Space reserved at the anchored edge; 0 for none, -1 to ignore other surfaces' zones.
    pub exclusive_zone: i32,
    pub keyboard: LayerKeyboard,
    /// The output to put it on; `None` lets the compositor choose (the active one). An
    /// output that has gone away by the time the surface opens counts as `None`.
    pub output: Option<OutputId>,
}

impl LayerSpec {
    /// A top-layer surface with the given anchor, no margin, no exclusive zone and
    /// on-demand keyboard focus.
    pub fn new(namespace: impl Into<String>, anchor: Anchor) -> Self {
        Self {
            namespace: namespace.into(),
            layer: ShellLayer::Top,
            anchor,
            margin: Margin::default(),
            exclusive_zone: 0,
            keyboard: LayerKeyboard::OnDemand,
            output: None,
        }
    }
}

/// The desktop layer a layer surface sits on, bottom to top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Output edges a layer surface is anchored to. None: centred.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent edges, as in the protocol's bitfield"
)]
pub struct Anchor {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl Anchor {
    pub const TOP: Self = Self {
        top: true,
        bottom: false,
        left: false,
        right: false,
    };
    pub const BOTTOM: Self = Self {
        top: false,
        bottom: true,
        left: false,
        right: false,
    };
    /// The whole top edge (a menu bar).
    pub const TOP_EDGE: Self = Self {
        top: true,
        bottom: false,
        left: true,
        right: true,
    };
    /// Every edge: the whole output (a desktop background).
    pub const ALL: Self = Self {
        top: true,
        bottom: true,
        left: true,
        right: true,
    };
}

/// Logical distance from the anchored edges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Margin {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

/// When a layer surface takes keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKeyboard {
    None,
    /// Takes focus whenever it's mapped (lock screens, launchers).
    Exclusive,
    /// Takes focus when clicked, like a window.
    OnDemand,
}

/// The app side of a driver: opens windows and reacts to widget actions.
pub trait App {
    /// Called once, when the platform is ready for windows. Open the first window here.
    fn on_start(&mut self, cx: &mut Ctx<'_>);

    /// A widget in `window` emitted `action`.
    fn on_action(
        &mut self,
        cx: &mut Ctx<'_>,
        window: WindowId,
        widget: WidgetId,
        action: ErasedAction,
    );

    /// The user asked to close `window` (the close button). Closes it by default.
    fn on_close_requested(&mut self, cx: &mut Ctx<'_>, window: WindowId) {
        cx.close_window(window);
    }

    /// Another thread called [`Waker::wake`]. Wakes coalesce: one call can stand for
    /// several, so drain whatever the other threads queued.
    fn on_wake(&mut self, _cx: &mut Ctx<'_>) {}

    /// An output appeared, changed or went away (Wayland only; see [`crate::desktop`]).
    fn on_output(&mut self, _cx: &mut Ctx<'_>, _event: OutputEvent) {}

    /// The compositor's window list changed (`neue_shell_v1`; only the shell gets these).
    fn on_shell(&mut self, _cx: &mut Ctx<'_>, _event: ShellEvent) {}
}

/// Wakes a driver's event loop from any thread; the loop then calls [`App::on_wake`].
/// Get one from [`Ctx::waker`].
#[derive(Clone)]
pub struct Waker {
    /// A wake is queued and [`App::on_wake`] hasn't run for it yet.
    pending: Arc<AtomicBool>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waker")
            .field("pending", &self.pending.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Waker {
    /// A waker that runs `notify` to wake the loop, at most once per [`Waker::take`].
    pub(crate) fn new(notify: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            pending: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(notify),
        }
    }

    /// Makes the loop call [`App::on_wake`] soon. Cheap when a wake is already pending.
    pub fn wake(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            (self.notify)();
        }
    }

    /// Clears the pending wake; true if there was one. The driver calls this before
    /// [`App::on_wake`], so a wake from inside the callback queues another.
    pub(crate) fn take(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }
}

/// What an [`App`] can do from a callback.
pub struct Ctx<'a> {
    host: &'a mut dyn Host,
}

impl std::fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx")
            .field("windows", &self.host.window_ids())
            .finish_non_exhaustive()
    }
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(host: &'a mut dyn Host) -> Self {
        Self { host }
    }

    /// Queues a window; it opens right after the callback returns (on macOS hidden until
    /// its first frame is drawn, on Wayland once the compositor has configured it).
    pub fn open_window(&mut self, spec: WindowSpec) -> WindowId {
        self.host.open_window(spec)
    }

    /// Closes `window`. Unknown or already-closed ids are ignored.
    pub fn close_window(&mut self, window: WindowId) {
        self.host.close_window(window);
    }

    /// The widget tree of an open window. `None` while the window is still queued, and
    /// after it closed.
    pub fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot> {
        self.host.render_root(window)
    }

    /// Open windows, in no particular order.
    pub fn windows(&self) -> impl Iterator<Item = WindowId> + '_ {
        self.host.window_ids().into_iter()
    }

    /// Whether a toplevel with [`WindowSpec::parent`] and [`WindowSpec::modal`] opens as a
    /// modal child window that the compositor keeps above its parent: on Wayland
    /// (xdg-dialog). The winit driver ignores both, so neue-kit draws sheets inside the
    /// parent window there instead (ADR 0057).
    pub fn has_modal_windows(&self) -> bool {
        cfg!(target_os = "linux")
    }

    /// Ends the event loop after the current callback.
    pub fn exit(&mut self) {
        self.host.exit();
    }

    /// Keeps the loop running while no window is open, until [`Ctx::exit`]: for the
    /// shell, whose surfaces come and go with the outputs.
    pub fn keep_alive(&mut self) {
        self.host.keep_alive();
    }

    /// A handle other threads use to wake the loop (see [`App::on_wake`]).
    pub fn waker(&self) -> Waker {
        self.host.waker()
    }

    /// Makes `text` the primary selection (what a middle click pastes), as when the user
    /// has just selected it. Call it from the callback that handles the selecting input:
    /// Wayland needs that input's serial. A no-op on winit and for an empty `text`.
    pub fn set_primary_selection(&mut self, text: String) {
        self.host.set_primary_selection(text);
    }

    /// Tells the compositor where `window`'s menu bar is exported (`None`: it has none).
    /// On Wayland through `org_kde_kwin_appmenu`, for toplevels only; elsewhere ignored.
    pub fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>) {
        self.host.set_menu_address(window, address);
    }

    /// The outputs connected now, in the order they appeared. Empty on the winit driver.
    pub fn outputs(&self) -> Vec<OutputInfo> {
        self.host.outputs()
    }

    /// Whether the compositor offers this client `neue_shell_v1`: true only for the shell
    /// the compositor started (ADR 0066).
    pub fn has_shell(&self) -> bool {
        self.host.has_shell()
    }

    /// Whether the compositor blurs what is behind surfaces that ask (`ext-background-effect-v1`
    /// with the blur capability): translucent materials can then draw their translucent
    /// tint. Always false on the winit driver. Known when [`App::on_start`] runs.
    pub fn has_blur(&self) -> bool {
        self.host.has_blur()
    }

    /// Sends a request to the compositor over `neue_shell_v1`, after the callback returns.
    /// Dropped (with a debug log) without the protocol.
    pub fn shell(&mut self, request: ShellRequest) {
        self.host.shell(request);
    }
}

/// The driver side of [`Ctx`].
pub(crate) trait Host {
    fn open_window(&mut self, spec: WindowSpec) -> WindowId;
    fn close_window(&mut self, window: WindowId);
    fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot>;
    fn window_ids(&self) -> Vec<WindowId>;
    fn exit(&mut self);
    fn keep_alive(&mut self);
    fn waker(&self) -> Waker;
    fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>);
    fn set_primary_selection(&mut self, text: String);
    fn outputs(&self) -> Vec<OutputInfo>;
    fn has_shell(&self) -> bool;
    fn has_blur(&self) -> bool;
    fn shell(&mut self, request: ShellRequest);
}

/// A driver's open windows, the windows queued to open, and the exit flag.
pub(crate) struct Registry<W> {
    pub(crate) windows: HashMap<WindowId, W>,
    /// Queued by [`Ctx::open_window`]; the driver creates them after the callback.
    pub(crate) opening: Vec<(WindowId, WindowSpec)>,
    /// Queued by [`Ctx::set_menu_address`]; the driver applies them after opening windows.
    pub(crate) menu_addresses: Vec<(WindowId, Option<MenuAddress>)>,
    /// Queued by [`Ctx::set_primary_selection`]; the latest wins. The driver takes the
    /// selection after the callback.
    pub(crate) primary_selection: Option<String>,
    /// Connected outputs, in the order they appeared; kept by the driver.
    pub(crate) outputs: Vec<OutputInfo>,
    /// Whether `neue_shell_manager_v1` is bound; set by the driver.
    pub(crate) has_shell: bool,
    /// Whether the compositor offers blur; set by the driver, which may change it while
    /// running.
    pub(crate) blur: Arc<AtomicBool>,
    /// Queued by [`Ctx::shell`]; the driver sends them after the callback.
    pub(crate) shell_requests: Vec<ShellRequest>,
    next_id: u64,
    pub(crate) exit: bool,
    /// Set by [`Ctx::keep_alive`]: no open window doesn't end the loop.
    keep_alive: bool,
    pub(crate) waker: Waker,
}

impl<W> Registry<W> {
    pub(crate) fn new(waker: Waker) -> Self {
        Self {
            windows: HashMap::new(),
            opening: Vec::new(),
            menu_addresses: Vec::new(),
            primary_selection: None,
            outputs: Vec::new(),
            has_shell: false,
            blur: Arc::new(AtomicBool::new(false)),
            shell_requests: Vec::new(),
            next_id: 0,
            exit: false,
            keep_alive: false,
            waker,
        }
    }
}

impl<W> std::fmt::Debug for Registry<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("windows", &self.windows.len())
            .field("opening", &self.opening.len())
            .field("exit", &self.exit)
            .finish()
    }
}

/// A driver window that owns a widget tree.
pub(crate) trait DriverWindow {
    fn render_root(&mut self) -> &mut RenderRoot;
}

impl<W: DriverWindow> Host for Registry<W> {
    fn open_window(&mut self, spec: WindowSpec) -> WindowId {
        let id = WindowId(self.next_id);
        self.next_id += 1;
        self.opening.push((id, spec));
        id
    }

    fn close_window(&mut self, window: WindowId) {
        self.opening.retain(|(id, _)| *id != window);
        if self.windows.remove(&window).is_some() {
            tracing::debug!(%window, "closed window");
        }
    }

    fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot> {
        self.windows.get_mut(&window).map(DriverWindow::render_root)
    }

    fn window_ids(&self) -> Vec<WindowId> {
        self.windows.keys().copied().collect()
    }

    fn exit(&mut self) {
        self.exit = true;
    }

    fn keep_alive(&mut self) {
        self.keep_alive = true;
    }

    fn waker(&self) -> Waker {
        self.waker.clone()
    }

    fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>) {
        self.menu_addresses.push((window, address));
    }

    fn set_primary_selection(&mut self, text: String) {
        if !text.is_empty() {
            self.primary_selection = Some(text);
        }
    }

    fn outputs(&self) -> Vec<OutputInfo> {
        self.outputs.clone()
    }

    fn has_shell(&self) -> bool {
        self.has_shell
    }

    fn has_blur(&self) -> bool {
        self.blur.load(Ordering::Relaxed)
    }

    fn shell(&mut self, request: ShellRequest) {
        if self.has_shell {
            self.shell_requests.push(request);
        } else {
            tracing::debug!(?request, "no neue_shell_v1: request dropped");
        }
    }
}

impl<W> Registry<W> {
    /// Whether the loop should end: exit was requested, or every window has closed (unless
    /// kept alive).
    pub(crate) fn done(&self) -> bool {
        self.exit || (!self.keep_alive && self.windows.is_empty() && self.opening.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy;

    impl DriverWindow for Dummy {
        fn render_root(&mut self) -> &mut RenderRoot {
            unreachable!("not used by these tests")
        }
    }

    fn spec() -> WindowSpec {
        // Any widget will do; the registry never builds it.
        WindowSpec::new("t", (10, 10), NewWidget::new(Spacer).erased())
    }

    #[test]
    fn the_latest_primary_selection_wins_and_empty_is_ignored() {
        let mut reg = Registry::<Dummy>::new(Waker::new(|| {}));
        Ctx::new(&mut reg).set_primary_selection("a".into());
        Ctx::new(&mut reg).set_primary_selection("b".into());
        Ctx::new(&mut reg).set_primary_selection(String::new());
        assert_eq!(reg.primary_selection.as_deref(), Some("b"));
    }

    #[test]
    fn chrome_height_rounds_up_and_is_shared() {
        let height = ChromeHeight::new();
        let driver = height.clone();
        assert_eq!(driver.get(), 0);
        height.set(51.2);
        assert_eq!(driver.get(), 52);
        height.set(-3.0);
        assert_eq!(driver.get(), 0);
        height.set(f64::NAN);
        assert_eq!(driver.get(), 0);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut reg = Registry::<Dummy>::new(Waker::new(|| {}));
        let a = Ctx::new(&mut reg).open_window(spec());
        Ctx::new(&mut reg).close_window(a);
        let b = Ctx::new(&mut reg).open_window(spec());
        assert_ne!(a, b);
        assert_eq!(reg.opening.len(), 1, "closing a queued window unqueues it");
    }

    #[test]
    fn wakes_coalesce_until_taken() {
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = count.clone();
        let waker = Waker::new(move || {
            c.fetch_add(1, Ordering::Relaxed);
        });
        waker.wake();
        waker.clone().wake();
        assert_eq!(count.load(Ordering::Relaxed), 1);
        assert!(waker.take());
        assert!(!waker.take());
        waker.wake();
        assert_eq!(count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn done_when_no_window_is_open_or_queued() {
        let mut reg = Registry::<Dummy>::new(Waker::new(|| {}));
        assert!(reg.done());
        let id = Ctx::new(&mut reg).open_window(spec());
        assert!(!reg.done());
        reg.opening.clear();
        reg.windows.insert(id, Dummy);
        assert!(!reg.done());
        Ctx::new(&mut reg).close_window(id);
        assert!(reg.done());
        Ctx::new(&mut reg).open_window(spec());
        Ctx::new(&mut reg).exit();
        assert!(reg.done());
    }

    #[test]
    fn kept_alive_without_windows_until_exit() {
        let mut reg = Registry::<Dummy>::new(Waker::new(|| {}));
        Ctx::new(&mut reg).keep_alive();
        assert!(!reg.done());
        Ctx::new(&mut reg).exit();
        assert!(reg.done());
    }

    use masonry_core::accesskit::{Node, Role};
    use masonry_core::core::{
        AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
        RegisterCtx,
    };
    use masonry_core::imaging::Painter;
    use masonry_core::kurbo::{Axis, Size};
    use masonry_core::layout::{LenReq, Length};

    /// The smallest widget there is.
    struct Spacer;

    impl Widget for Spacer {
        type Action = NoAction;
        fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
        fn measure(
            &mut self,
            _: &mut MeasureCtx<'_>,
            _: &PropertiesRef<'_>,
            _: Axis,
            _: LenReq,
            _: Option<Length>,
        ) -> Length {
            Length::ZERO
        }
        fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}
        fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}
        fn accessibility_role(&self) -> Role {
            Role::GenericContainer
        }
        fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
        fn children_ids(&self) -> ChildrenIds {
            ChildrenIds::new()
        }
    }
}
