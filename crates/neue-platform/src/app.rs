//! The app-facing API both drivers share (ADR 0032 §3, ADR 0033).
//!
//! An [`App`] gets callbacks with a [`Ctx`], through which it opens and closes windows,
//! reaches their widget trees and ends the loop. The same app runs on the winit driver
//! (macOS) and the Wayland driver (Linux); [`crate::run`] picks the host's.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use masonry_core::app::RenderRoot;
use masonry_core::core::{ErasedAction, NewWidget, Widget, WidgetId};
use masonry_core::peniko::Color;

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
        }
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

    /// A handle other threads use to wake the loop (see [`App::on_wake`]).
    pub fn waker(&self) -> Waker {
        self.host.waker()
    }

    /// Tells the compositor where `window`'s menu bar is exported (`None`: it has none).
    /// On Wayland through `org_kde_kwin_appmenu`, for toplevels only; elsewhere ignored.
    pub fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>) {
        self.host.set_menu_address(window, address);
    }
}

/// The driver side of [`Ctx`].
pub(crate) trait Host {
    fn open_window(&mut self, spec: WindowSpec) -> WindowId;
    fn close_window(&mut self, window: WindowId);
    fn render_root(&mut self, window: WindowId) -> Option<&mut RenderRoot>;
    fn window_ids(&self) -> Vec<WindowId>;
    fn exit(&mut self);
    fn waker(&self) -> Waker;
    fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>);
}

/// A driver's open windows, the windows queued to open, and the exit flag.
pub(crate) struct Registry<W> {
    pub(crate) windows: HashMap<WindowId, W>,
    /// Queued by [`Ctx::open_window`]; the driver creates them after the callback.
    pub(crate) opening: Vec<(WindowId, WindowSpec)>,
    /// Queued by [`Ctx::set_menu_address`]; the driver applies them after opening windows.
    pub(crate) menu_addresses: Vec<(WindowId, Option<MenuAddress>)>,
    next_id: u64,
    pub(crate) exit: bool,
    pub(crate) waker: Waker,
}

impl<W> Registry<W> {
    pub(crate) fn new(waker: Waker) -> Self {
        Self {
            windows: HashMap::new(),
            opening: Vec::new(),
            menu_addresses: Vec::new(),
            next_id: 0,
            exit: false,
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

    fn waker(&self) -> Waker {
        self.waker.clone()
    }

    fn set_menu_address(&mut self, window: WindowId, address: Option<MenuAddress>) {
        self.menu_addresses.push((window, address));
    }
}

impl<W> Registry<W> {
    /// Whether the loop should end: exit was requested, or every window has closed.
    pub(crate) fn done(&self) -> bool {
        self.exit || (self.windows.is_empty() && self.opening.is_empty())
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
