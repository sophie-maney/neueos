//! Compositor state shared by all backends.
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/state.rs`.

use std::ffi::OsString;
use std::sync::Arc;
use std::time::Instant;

use smithay::desktop::{PopupManager, Space, Window, WindowSurfaceType};
use smithay::input::keyboard::{KeyboardHandle, XkbConfig};
use smithay::input::pointer::PointerHandle;
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::{EventLoop, LoopSignal};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::dmabuf::DmabufState;
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::warn;

use crate::Error;

/// Key repeat delay and rate until keyboard settings come from config (task 1.6).
const REPEAT_DELAY_MS: i32 = 300;
const REPEAT_RATE_HZ: i32 = 30;

/// The backend that owns the renderer and outputs.
#[derive(Debug)]
pub enum Backend {
    #[cfg(feature = "winit")]
    Winit(crate::winit::WinitData),
}

/// Everything the compositor knows. One instance, owned by the event loop.
#[derive(Debug)]
pub struct State {
    pub start_time: Instant,
    /// Name of our Wayland socket (e.g. `wayland-1`), relative to `XDG_RUNTIME_DIR`.
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
    pub backend: Backend,

    /// Mapped windows, their positions and their stacking order (last is topmost).
    pub space: Space<Window>,
    /// Toplevels that exist but have not committed a buffer yet. They are placed and mapped
    /// on their first buffer commit, once their size is known.
    pub unmapped: Vec<Window>,
    /// Number of windows placed so far; drives the cascade in [`crate::placement`].
    pub placed: usize,
    pub popups: PopupManager,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub dmabuf_state: DmabufState,
    pub output_manager_state: OutputManagerState,
    pub data_device_state: DataDeviceState,
    pub seat_state: SeatState<State>,
    pub seat: Seat<State>,
    pub keyboard: KeyboardHandle<State>,
    pub pointer: PointerHandle<State>,
}

impl State {
    /// Create the protocol globals and the listening socket.
    pub fn new(
        event_loop: &EventLoop<'static, State>,
        dh: DisplayHandle,
        backend: Backend,
        dmabuf_state: DmabufState,
    ) -> Result<Self, Error> {
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        // Clipboard. Some clients (foot) refuse to start without it. Primary selection,
        // data-control and DnD icons are task 1.8.
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");
        let keyboard = seat
            .add_keyboard(XkbConfig::default(), REPEAT_DELAY_MS, REPEAT_RATE_HZ)
            .map_err(Error::init("loading the default xkb keymap"))?;
        let pointer = seat.add_pointer();

        let socket = ListeningSocketSource::new_auto()
            .map_err(Error::init("creating the Wayland socket"))?;
        let socket_name = socket.socket_name().to_os_string();
        event_loop
            .handle()
            .insert_source(socket, |stream, _, state: &mut State| {
                if let Err(err) = state
                    .display_handle
                    .insert_client(stream, Arc::new(ClientState::default()))
                {
                    warn!(%err, "failed to add client");
                }
            })
            .map_err(|e| e.error)
            .map_err(Error::init("adding the Wayland socket source"))?;

        let mut space = Space::default();
        match &backend {
            #[cfg(feature = "winit")]
            Backend::Winit(data) => space.map_output(data.output(), (0, 0)),
        }

        Ok(Self {
            start_time: Instant::now(),
            socket_name,
            display_handle: dh,
            loop_signal: event_loop.get_signal(),
            backend,
            space,
            unmapped: Vec::new(),
            placed: 0,
            popups: PopupManager::default(),
            compositor_state,
            xdg_shell_state,
            shm_state,
            dmabuf_state,
            output_manager_state,
            data_device_state,
            seat_state,
            seat,
            keyboard,
            pointer,
        })
    }

    /// The surface under `pos` and its origin, in global logical coordinates.
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space
            .element_under(pos)
            .and_then(|(window, location)| {
                window
                    .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                    .map(|(surface, offset)| (surface, (offset + location).to_f64()))
            })
    }

    /// The mapped window whose toplevel is `surface`.
    pub fn window_for_surface(&self, surface: &WlSurface) -> Option<Window> {
        self.space
            .elements()
            .find(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
            .cloned()
    }

    /// Raise `window`, give it keyboard focus and mark it activated (and every other
    /// window deactivated).
    pub fn focus_window(&mut self, window: &Window) {
        let serial = SERIAL_COUNTER.next_serial();
        self.space.raise_element(window, true);
        let surface = window.toplevel().map(|t| t.wl_surface().clone());
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, surface, serial);
        self.send_pending_configures();
    }

    /// Focus the topmost window, or nothing when no window is left.
    pub fn focus_topmost(&mut self) {
        let topmost = self.space.elements().next_back().cloned();
        match topmost {
            Some(window) => self.focus_window(&window),
            None => self.clear_focus(),
        }
    }

    /// Deactivate every window and drop keyboard focus.
    pub fn clear_focus(&mut self) {
        let serial = SERIAL_COUNTER.next_serial();
        for window in self.space.elements() {
            window.set_activated(false);
        }
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, None, serial);
        self.send_pending_configures();
    }

    fn send_pending_configures(&self) {
        for window in self.space.elements() {
            if let Some(toplevel) = window.toplevel() {
                toplevel.send_pending_configure();
            }
        }
    }
}

/// Per-client data.
#[derive(Debug, Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
