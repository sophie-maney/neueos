//! Compositor state shared by all backends.
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/state.rs`.

use std::collections::HashMap;
use std::ffi::OsString;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Instant;

use neue_ipc::{OutputInfo, WindowId, WindowInfo};
use smithay::desktop::{PopupManager, Space, Window, WindowSurfaceType};
use smithay::input::keyboard::{KeyboardHandle, XkbConfig};
use smithay::input::pointer::{CursorIcon, CursorImageStatus, PointerHandle};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::{EventLoop, LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DisplayHandle};
use smithay::utils::{Clock, Logical, Monotonic, Point};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::DmabufState;
use smithay::wayland::foreign_toplevel_list::ForeignToplevelListState;
use smithay::wayland::fractional_scale::FractionalScaleManagerState;
use smithay::wayland::idle_inhibit::IdleInhibitManagerState;
use smithay::wayland::idle_notify::IdleNotifierState;
use smithay::wayland::input_method::InputMethodManagerState;
use smithay::wayland::keyboard_shortcuts_inhibit::{
    KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::pointer_gestures::PointerGesturesState;
use smithay::wayland::presentation::PresentationState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::security_context::{SecurityContext, SecurityContextState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::ext_data_control::DataControlState as ExtDataControlState;
use smithay::wayland::selection::primary_selection::PrimarySelectionState;
use smithay::wayland::selection::wlr_data_control::DataControlState as WlrDataControlState;
use smithay::wayland::session_lock::SessionLockManagerState;
use smithay::wayland::shell::kde::decoration::KdeDecorationState;
use smithay::wayland::shell::wlr_layer::{Layer, WlrLayerShellState};
use smithay::wayland::shell::xdg::dialog::XdgDialogState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDefaultMode;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::XdgActivationState;
use smithay::wayland::xdg_foreign::XdgForeignState;
use smithay::wayland::xdg_system_bell::XdgSystemBellState;
use smithay::wayland::xdg_toplevel_icon::XdgToplevelIconManager;
use tracing::warn;

use crate::Error;
use crate::appmenu::AppmenuState;
use crate::config::ConfigRuntime;
use crate::decorations::Chrome;
use crate::ipc::IpcServer;
use crate::lock::SessionLockStatus;
use crate::outputs::LaidOutOutput;
use crate::presentation::TearingControlState;
use crate::screencopy::Screencopy;
use crate::security::is_unrestricted;

/// Key repeat delay and rate of the seat's keyboard until the config is loaded. They
/// match the config defaults, which `State::init_config` applies right after start-up.
const REPEAT_DELAY_MS: i32 = 300;
const REPEAT_RATE_HZ: i32 = 30;

/// The backend that owns the renderer and outputs.
#[derive(Debug)]
pub enum Backend {
    #[cfg(feature = "winit")]
    // Boxed: it holds the GL context and is far larger than the other variants.
    Winit(Box<crate::winit::WinitData>),
    Headless(crate::headless::HeadlessData),
    #[cfg(feature = "tty")]
    // Boxed for the same reason as winit.
    Tty(Box<crate::tty::TtyData>),
}

impl Backend {
    /// The outputs the backend has right now. [`State::new`] adds them to the layout;
    /// later changes go through [`State::add_output`] and [`State::remove_output`].
    pub fn outputs(&self) -> Vec<Output> {
        match self {
            #[cfg(feature = "winit")]
            Self::Winit(data) => vec![data.output().clone()],
            Self::Headless(data) => data.outputs().to_vec(),
            #[cfg(feature = "tty")]
            Self::Tty(data) => data.outputs(),
        }
    }

    /// What `output`'s connector can do for variable refresh, and whether it is on now.
    pub fn adaptive_sync(&self, output: &Output) -> (crate::vrr::Support, bool) {
        match self {
            #[cfg(feature = "tty")]
            Self::Tty(data) => data.adaptive_sync(output),
            _ => {
                let _ = output; // only the tty backend has VRR
                (crate::vrr::Support::None, false)
            }
        }
    }
}

/// Everything the compositor knows. One instance, owned by the event loop.
#[derive(Debug)]
pub struct State {
    pub start_time: Instant,
    /// `CLOCK_MONOTONIC`, the clock of presentation feedback.
    pub clock: Clock<Monotonic>,
    /// Name of our Wayland socket (e.g. `wayland-1`), relative to `XDG_RUNTIME_DIR`.
    /// `None` when the compositor does not listen (tests connect clients directly).
    pub socket_name: Option<OsString>,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
    pub loop_handle: LoopHandle<'static, State>,
    pub backend: Backend,
    /// The outputs in the layout, in connection order (see `outputs.rs`).
    pub(crate) outputs: Vec<LaidOutOutput>,
    /// What `neuectl` subscribers were last told about the outputs.
    pub(crate) reported_outputs: Vec<OutputInfo>,

    /// Mapped windows, their positions and their stacking order (last is topmost).
    pub space: Space<Window>,
    /// Toplevels that exist but have not committed a buffer yet. They are placed and mapped
    /// on their first buffer commit, once their size is known.
    pub unmapped: Vec<Window>,
    /// Minimised windows and where they were, in the order they were minimised.
    pub minimized: Vec<(Window, Point<i32, Logical>)>,
    /// Where the last window of each app (by app id) was when it closed, this session.
    pub remembered: HashMap<String, (i32, i32)>,
    pub popups: PopupManager,
    /// Id for the next toplevel (see [`State::window_id`]). Ids start at 1 and are never
    /// reused.
    next_window_id: u64,
    /// The `neuectl` socket and its connections.
    pub ipc: IpcServer,
    /// `compositor.toml`: the active config, key bindings and reload machinery.
    pub config: ConfigRuntime,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    /// `xdg-decoration`: clients ask for server-side title bars (ADR 0013).
    pub xdg_decoration_state: XdgDecorationState,
    /// KDE's older server-decoration protocol, used by GTK3 and older Qt.
    pub kde_decoration_state: KdeDecorationState,
    /// Title-bar rendering and its caches.
    pub chrome: Chrome,
    pub shm_state: ShmState,
    pub dmabuf_state: DmabufState,
    pub output_manager_state: OutputManagerState,
    pub data_device_state: DataDeviceState,
    pub primary_selection_state: PrimarySelectionState,
    /// `ext-data-control-v1`, for clipboard managers and `wl-clipboard`.
    pub ext_data_control_state: ExtDataControlState,
    /// `wlr-data-control-unstable-v1`, the older form most clipboard tools still use.
    pub wlr_data_control_state: WlrDataControlState,
    /// The surface a client attached to the running drag as its icon, if any.
    pub dnd_icon: Option<DndIcon>,
    /// A pointer drag-and-drop is running (with or without an icon). Escape cancels it.
    pub dnd_active: bool,
    pub xdg_activation_state: XdgActivationState,
    pub fractional_scale_state: FractionalScaleManagerState,
    /// `wp_cursor_shape_manager_v1`: clients name a cursor, the compositor draws it.
    pub cursor_shape_state: CursorShapeManagerState,
    pub viewporter_state: ViewporterState,
    /// `wlr-layer-shell`: panels, the dock, wallpapers (see `layer.rs`).
    pub layer_shell_state: WlrLayerShellState,
    /// `ext-session-lock-v1` global (see `lock.rs`).
    pub lock_manager_state: SessionLockManagerState,
    /// Whether the session is locked, and by whom.
    pub session_lock: SessionLockStatus,
    pub idle_notifier_state: IdleNotifierState<State>,
    /// `idle-inhibit`: the surfaces that hold an inhibitor, one entry each.
    pub idle_inhibitors: Vec<WlSurface>,
    pub idle_inhibit_state: IdleInhibitManagerState,
    pub foreign_toplevel_list_state: ForeignToplevelListState,
    pub xdg_foreign_state: XdgForeignState,
    /// `xdg-dialog` (modal flag only).
    pub xdg_dialog_state: XdgDialogState,
    pub toplevel_icon_manager: XdgToplevelIconManager,
    pub system_bell_state: XdgSystemBellState,
    /// `org_kde_kwin_appmenu` and `gtk_shell1`: where apps export their menus (ADR 0025).
    pub appmenu_state: AppmenuState,
    pub shortcuts_inhibit_state: KeyboardShortcutsInhibitState,
    /// The shortcut inhibitor of the focused surface, while it is active: key bindings are
    /// off.
    pub active_inhibitor: Option<KeyboardShortcutsInhibitor>,
    /// `pointer-constraints`: lock or confine the pointer (see `input_protocols.rs`).
    pub pointer_constraints_state: PointerConstraintsState,
    /// The surface whose pointer constraint is active, if any.
    pub constrained_surface: Option<WlSurface>,
    /// Pointer constraints that exist (an upper bound: one-shot constraints that ended
    /// are not subtracted). While zero, nothing needs checking.
    pub constraint_count: usize,
    /// Where the pointer is, kept by `move_pointer`. Constraint checks read this instead
    /// of the seat's pointer, whose lock is held while a grab runs.
    pub pointer_pos: Point<f64, Logical>,
    /// `relative-pointer`: unaccelerated motion for games and 3D views.
    pub relative_pointer_state: RelativePointerManagerState,
    /// `pointer-gestures`: touchpad swipe, pinch and hold.
    pub pointer_gestures_state: PointerGesturesState,
    /// `text-input-v3`: apps ask for an input method.
    pub text_input_state: TextInputManagerState,
    /// `input-method-v2`: the input method (fcitx5, ibus) that commits text.
    pub input_method_state: InputMethodManagerState,
    /// `wp_presentation`: when frames reached the screen (see `presentation.rs`).
    pub presentation_state: PresentationState,
    /// `wp_tearing_control_v1`: per-surface tearing hints (recorded, not acted on yet).
    pub tearing_control_state: TearingControlState,
    /// `wp_security_context_v1`: sandboxes create sockets whose clients see fewer globals
    /// (see `security.rs`).
    pub security_context_state: SecurityContextState,
    /// `ext-image-copy-capture` with output sources, and its sessions (see
    /// `screencopy.rs`).
    pub screencopy: Screencopy,
    pub seat_state: SeatState<State>,
    pub seat: Seat<State>,
    pub keyboard: KeyboardHandle<State>,
    pub pointer: PointerHandle<State>,
    /// What the pointer looks like: what the focused client asked for (a named shape or a
    /// surface), or a shape the compositor chose (a resize grab). Drawn by the tty backend;
    /// the winit backend sets the host window's cursor instead.
    pub cursor_status: CursorImageStatus,
    /// A shape the compositor insists on (an interactive resize): clients cannot change
    /// the cursor while it is set, even though the grab gives them the right to.
    pub cursor_override: Option<CursorIcon>,
}

/// The icon of a drag in progress, drawn at the pointer.
#[derive(Debug)]
pub struct DndIcon {
    pub surface: WlSurface,
    /// Where the surface's origin sits relative to the pointer. Starts at zero and moves
    /// with the buffer offsets the client commits.
    pub offset: Point<i32, Logical>,
}

impl State {
    /// Force the cursor to `icon` until called again with `None`, which restores the
    /// default (the client under the pointer then sets its own).
    pub fn set_cursor_override(&mut self, icon: Option<CursorIcon>) {
        self.cursor_override = icon;
        self.set_cursor(
            icon.map_or_else(CursorImageStatus::default_named, CursorImageStatus::Named),
        );
    }

    /// Change what the pointer looks like. The tty backend draws `image` itself; the winit
    /// backend passes the shape on to the host window.
    pub fn set_cursor(&mut self, image: CursorImageStatus) {
        #[cfg(feature = "winit")]
        if let Backend::Winit(data) = &self.backend {
            data.set_host_cursor(&image);
        }
        self.cursor_status = image;
    }

    /// Draw every output again: something they may show changed. Only the tty backend
    /// draws on demand (ADR 0019); winit redraws continuously and headless does not draw.
    pub fn queue_redraw_all(&mut self) {
        #[cfg(feature = "tty")]
        if let Backend::Tty(data) = &mut self.backend {
            data.queue_redraw_all();
        }
    }

    /// Create the protocol globals and, if `listen` is set, the listening socket.
    pub fn new(
        event_loop: &EventLoop<'static, State>,
        dh: DisplayHandle,
        backend: Backend,
        dmabuf_state: DmabufState,
        listen: bool,
    ) -> Result<Self, Error> {
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let kde_decoration_state = KdeDecorationState::new::<Self>(&dh, KdeDefaultMode::Server);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        // Clipboard. Some clients (foot) refuse to start without it.
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        // Data-control lets any unsandboxed client manage the clipboard without focus
        // (ADR 0011). Privileged globals are hidden from sandboxed clients (ADR 0017).
        let ext_data_control_state = ExtDataControlState::new::<Self, _>(
            &dh,
            Some(&primary_selection_state),
            is_unrestricted,
        );
        let wlr_data_control_state = WlrDataControlState::new::<Self, _>(
            &dh,
            Some(&primary_selection_state),
            is_unrestricted,
        );
        let xdg_activation_state = XdgActivationState::new::<Self>(&dh);
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(&dh);
        let cursor_shape_state = CursorShapeManagerState::new::<Self>(&dh);
        let viewporter_state = ViewporterState::new::<Self>(&dh);
        let layer_shell_state =
            WlrLayerShellState::new_with_filter::<Self, _>(&dh, is_unrestricted);
        // Every unsandboxed client may lock: `swaylock`-style lockers are ordinary clients.
        let lock_manager_state = SessionLockManagerState::new::<Self, _>(&dh, is_unrestricted);
        let idle_notifier_state = IdleNotifierState::new(&dh, event_loop.handle());
        let idle_inhibit_state = IdleInhibitManagerState::new::<Self>(&dh);
        let foreign_toplevel_list_state =
            ForeignToplevelListState::new_with_filter::<Self>(&dh, is_unrestricted);
        let xdg_foreign_state = XdgForeignState::new::<Self>(&dh);
        let xdg_dialog_state = XdgDialogState::new::<Self>(&dh);
        let mut toplevel_icon_manager = XdgToplevelIconManager::new::<Self>(&dh);
        toplevel_icon_manager.replace_icon_sizes([64, 128]);
        let system_bell_state = XdgSystemBellState::new::<Self>(&dh);
        let appmenu_state = AppmenuState::new(&dh);
        let shortcuts_inhibit_state = KeyboardShortcutsInhibitState::new::<Self>(&dh);
        let pointer_constraints_state = PointerConstraintsState::new::<Self>(&dh);
        let relative_pointer_state = RelativePointerManagerState::new::<Self>(&dh);
        let pointer_gestures_state = PointerGesturesState::new::<Self>(&dh);
        let text_input_state = TextInputManagerState::new::<Self>(&dh);
        // Any unsandboxed client may be the input method: fcitx5 and ibus are ordinary
        // clients.
        let input_method_state = InputMethodManagerState::new::<Self, _>(&dh, is_unrestricted);
        let clock = Clock::<Monotonic>::new();
        let presentation_state = PresentationState::new::<Self>(&dh, clock.id() as u32);
        let tearing_control_state = TearingControlState::new(&dh);
        // A sandboxed client must not create (and so escape into) a context of its own.
        let security_context_state = SecurityContextState::new::<Self, _>(&dh, is_unrestricted);
        let screencopy = Screencopy::new(&dh);

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");
        let keyboard = seat
            .add_keyboard(XkbConfig::default(), REPEAT_DELAY_MS, REPEAT_RATE_HZ)
            .map_err(Error::init("loading the default xkb keymap"))?;
        let pointer = seat.add_pointer();

        let socket_name = if listen {
            Some(listen_on_socket(event_loop)?)
        } else {
            None
        };

        let initial_outputs = backend.outputs();
        let mut state = Self {
            start_time: Instant::now(),
            clock,
            socket_name,
            display_handle: dh,
            loop_signal: event_loop.get_signal(),
            loop_handle: event_loop.handle(),
            backend,
            outputs: Vec::new(),
            reported_outputs: Vec::new(),
            space: Space::default(),
            unmapped: Vec::new(),
            minimized: Vec::new(),
            remembered: HashMap::new(),
            popups: PopupManager::default(),
            next_window_id: 1,
            ipc: IpcServer::new(event_loop.handle()),
            config: ConfigRuntime::new(event_loop.handle()),
            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            kde_decoration_state,
            chrome: Chrome::default(),
            shm_state,
            dmabuf_state,
            output_manager_state,
            data_device_state,
            primary_selection_state,
            ext_data_control_state,
            wlr_data_control_state,
            dnd_icon: None,
            dnd_active: false,
            xdg_activation_state,
            fractional_scale_state,
            cursor_shape_state,
            viewporter_state,
            layer_shell_state,
            lock_manager_state,
            session_lock: SessionLockStatus::default(),
            idle_notifier_state,
            idle_inhibitors: Vec::new(),
            idle_inhibit_state,
            foreign_toplevel_list_state,
            xdg_foreign_state,
            xdg_dialog_state,
            toplevel_icon_manager,
            system_bell_state,
            appmenu_state,
            shortcuts_inhibit_state,
            active_inhibitor: None,
            pointer_constraints_state,
            constrained_surface: None,
            constraint_count: 0,
            pointer_pos: Point::default(),
            relative_pointer_state,
            pointer_gestures_state,
            text_input_state,
            input_method_state,
            presentation_state,
            tearing_control_state,
            security_context_state,
            screencopy,
            seat_state,
            seat,
            keyboard,
            pointer,
            cursor_status: CursorImageStatus::default_named(),
            cursor_override: None,
        };
        for output in initial_outputs {
            state.add_output(output);
        }
        Ok(state)
    }

    /// Add a client connected through `stream`, e.g. one end of a socket pair.
    pub fn insert_client(&mut self, stream: UnixStream) -> std::io::Result<Client> {
        self.display_handle
            .insert_client(stream, Arc::new(ClientState::default()))
    }

    /// Give `window` the next window id. Called once per toplevel, when it is created.
    pub fn assign_window_id(&mut self, window: &Window) {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;
        window.user_data().insert_if_missing_threadsafe(|| id);
    }

    /// The id of `window`, if it has one.
    pub fn window_id(&self, window: &Window) -> Option<WindowId> {
        window.user_data().get::<WindowId>().copied()
    }

    /// Mapped windows bottom to top, after minimised ones.
    fn all_windows(&self) -> impl Iterator<Item = &Window> {
        self.minimized
            .iter()
            .map(|(w, _)| w)
            .chain(self.space.elements())
    }

    /// The mapped or minimised window with `id`.
    pub fn window_by_id(&self, id: WindowId) -> Option<Window> {
        self.all_windows()
            .find(|w| self.window_id(w) == Some(id))
            .cloned()
    }

    /// The id of the window with keyboard focus.
    pub fn focused_window_id(&self) -> Option<WindowId> {
        let focus = self.keyboard.current_focus()?;
        self.window_for_surface(&focus)
            .and_then(|w| self.window_id(&w))
    }

    /// The windows: minimised ones first, then mapped ones bottom to top.
    pub fn windows(&self) -> Vec<WindowInfo> {
        let focus = self.keyboard.current_focus();
        self.all_windows()
            .filter_map(|window| self.window_info(window, focus.as_ref()))
            .collect()
    }

    /// `window` as seen from outside, with `focus` the surface that has keyboard focus.
    pub fn window_info(&self, window: &Window, focus: Option<&WlSurface>) -> Option<WindowInfo> {
        let toplevel = window.toplevel()?;
        let (title, app_id) = crate::wm::title_and_app_id(window);
        let geometry = self.window_geometry(window)?;
        let (zoomed, demands_attention) =
            Self::with_wm_data(window, |d| (d.zoomed, d.demands_attention));
        Some(WindowInfo {
            id: self.window_id(window)?,
            title,
            app_id,
            x: geometry.loc.x,
            y: geometry.loc.y,
            width: geometry.size.w,
            height: geometry.size.h,
            focused: focus == Some(toplevel.wl_surface()),
            minimized: self.is_minimized(window),
            zoomed,
            fullscreen: Self::is_fullscreen(window),
            demands_attention,
            parent: self.parent_window(window).and_then(|p| self.window_id(&p)),
            menu: crate::appmenu::menu_address(toplevel.wl_surface()),
        })
    }

    /// The surface under `pos` and its origin, in global logical coordinates.
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        // A locked session shows (and hears from) the lock surfaces only.
        if self.is_locked() {
            return self.lock_surface_under(pos);
        }
        // A full-screen window covers its output's top layer (ADR 0018).
        let covered = self
            .space
            .output_under(pos)
            .next()
            .is_some_and(|o| crate::fullscreen::fullscreen_window_on(&self.space, o).is_some());
        let (above, below): (&[Layer], &[Layer]) = if covered {
            (
                &[Layer::Overlay],
                &[Layer::Top, Layer::Bottom, Layer::Background],
            )
        } else {
            (&crate::layer::ABOVE_WINDOWS, &crate::layer::BELOW_WINDOWS)
        };
        if let Some(hit) = self.layer_surface_under(pos, above) {
            return Some(hit);
        }
        // Title bars and resize edges belong to the compositor, not to the client.
        if self.chrome_hit_at(pos).is_some() {
            return None;
        }
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, offset)| (surface, (offset + location).to_f64()))
            })
            .or_else(|| self.layer_surface_under(pos, below))
    }

    /// The mapped or minimised window whose toplevel is `surface`.
    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<Window> {
        self.all_windows()
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
            .cloned()
    }
}

/// Listen on a new socket in `XDG_RUNTIME_DIR` and return its name.
fn listen_on_socket(event_loop: &EventLoop<'static, State>) -> Result<OsString, Error> {
    let socket =
        ListeningSocketSource::new_auto().map_err(Error::init("creating the Wayland socket"))?;
    let socket_name = socket.socket_name().to_os_string();
    event_loop
        .handle()
        .insert_source(socket, |stream, _, state: &mut State| {
            if let Err(err) = state.insert_client(stream) {
                warn!(%err, "failed to add client");
            }
        })
        .map_err(|e| e.error)
        .map_err(Error::init("adding the Wayland socket source"))?;
    Ok(socket_name)
}

/// Per-client data.
#[derive(Debug, Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    /// The sandbox the client connected through, if any (see `security.rs`).
    pub security_context: Option<SecurityContext>,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
