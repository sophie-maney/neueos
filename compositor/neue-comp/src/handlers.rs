//! Wayland protocol handlers.
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/handlers/` and, for dmabuf,
//! `anvil/src/winit.rs`.

use smithay::backend::allocator::dmabuf::Dmabuf;
#[cfg(any(feature = "winit", feature = "tty"))]
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::utils::{RendererSurfaceStateUserData, on_commit_buffer_handler};
use std::time::Duration;

use smithay::desktop::{
    PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy, Window,
    find_popup_root_surface, get_popup_toplevel_coords,
};
use smithay::input::dnd::{DnDGrab, DndGrabHandler, DndTarget, GrabType, Source};
use smithay::input::pointer::{CursorImageStatus, Focus};
use smithay::input::tablet::TabletSeatHandler;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_seat, wl_surface::WlSurface};
use smithay::reexports::wayland_server::{Client, Resource};
use smithay::utils::{Logical, Point, Serial};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, SurfaceAttributes, get_parent,
    is_sync_subsurface, with_states,
};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::fractional_scale::FractionalScaleHandler;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};
use smithay::wayland::selection::ext_data_control::{
    DataControlHandler as ExtDataControlHandler, DataControlState as ExtDataControlState,
};
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState, set_primary_focus,
};
use smithay::wayland::selection::wlr_data_control::{
    DataControlHandler as WlrDataControlHandler, DataControlState as WlrDataControlState,
};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};
use tracing::{debug, warn};

use crate::state::{Backend, ClientState, DndIcon, State};

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // Every client is inserted with `ClientState` in `State::new`, so the downcast
        // cannot fail for clients of this display.
        match client.get_data::<ClientState>() {
            Some(data) => &data.compositor_state,
            None => unreachable!("client inserted without ClientState"),
        }
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self.window_for_surface(&root) {
                window.on_commit();
                if &root == surface {
                    self.window_committed(&window);
                }
            }
        }

        self.commit_layer(surface);
        self.commit_unmapped(surface);
        self.commit_popup(surface);
        self.commit_dnd_icon(surface);
    }
}

impl State {
    /// Handle commits of toplevels that are not mapped yet: send the initial configure,
    /// then place and map the window once it has a buffer.
    fn commit_unmapped(&mut self, surface: &WlSurface) {
        let Some(index) = self
            .unmapped
            .iter()
            .position(|w| w.toplevel().is_some_and(|t| t.wl_surface() == surface))
        else {
            return;
        };
        let window = self.unmapped[index].clone();
        let Some(toplevel) = window.toplevel() else {
            return;
        };

        if !toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
            return;
        }

        window.on_commit();
        let has_buffer = with_states(surface, |states| {
            states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .is_some_and(|data| data.lock().is_ok_and(|d| d.buffer().is_some()))
        });
        if !has_buffer {
            return;
        }

        self.unmapped.swap_remove(index);
        self.map_new_window(window);
    }

    /// A drag icon that attaches a buffer with an offset moves relative to the pointer.
    fn commit_dnd_icon(&mut self, surface: &WlSurface) {
        let Some(icon) = self.dnd_icon.as_mut().filter(|i| &i.surface == surface) else {
            return;
        };
        with_states(surface, |states| {
            let delta = states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .buffer_delta
                .take()
                .unwrap_or_default();
            icon.offset += delta;
        });
    }

    fn commit_popup(&mut self, surface: &WlSurface) {
        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface)
            && !popup.is_initial_configure_sent()
            && let Err(err) = popup.send_configure()
        {
            // The initial configure is always allowed, so this is not expected.
            warn!(?err, "initial popup configure failed");
        }
    }

    /// Keep a popup inside the output its window is mostly on.
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.window_for_surface(&root) else {
            return;
        };
        let Some(output_geo) = crate::outputs::primary_output(&self.space, &window)
            .or_else(|| self.space.outputs().next().cloned())
            .and_then(|o| self.space.output_geometry(&o))
        else {
            return;
        };
        let Some(window_geo) = self.space.element_geometry(&window) else {
            return;
        };

        // The positioner works relative to the parent's geometry.
        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        let imported = match &mut self.backend {
            #[cfg(feature = "winit")]
            Backend::Winit(data) => data.renderer().import_dmabuf(&dmabuf, None).is_ok(),
            #[cfg(feature = "tty")]
            Backend::Tty(data) => data.renderer().import_dmabuf(&dmabuf, None).is_ok(),
            // No dmabuf global is advertised without a renderer.
            Backend::Headless(_) => {
                let _ = dmabuf;
                false
            }
        };
        if imported {
            let _ = notifier.successful::<State>();
        } else {
            notifier.failed();
        }
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<State> {
        &mut self.seat_state
    }

    // The tty backend draws it; winit sets the host window's cursor.
    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        // Not while the compositor has picked a shape (also swallows the reset to the
        // arrow that Smithay does when the pointer leaves a surface).
        if self.cursor_override.is_none() {
            self.set_cursor(image);
        }
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        // The clipboard follows keyboard focus.
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client.clone());
        set_primary_focus(dh, seat, client);

        self.update_shortcut_inhibitor(focused);
        // A pointer constraint follows keyboard focus too.
        self.refresh_pointer_constraint_for(focused);

        // Tell `neuectl` subscribers, once per actual change.
        let window = focused
            .and_then(|s| self.window_for_surface(s))
            .and_then(|w| self.window_id(&w));
        self.report_focus(window);
    }
}

// `cursor-shape-v1` needs the tablet handler trait; the tablet protocols are not enabled.
impl TabletSeatHandler for State {
    type ToolFocus = WlSurface;
}

impl OutputHandler for State {}

// Preferred scales are sent from `State::refresh_surface_scales`; a new fractional-scale
// object gets the stored one from Smithay.
impl FractionalScaleHandler for State {}

impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl PrimarySelectionHandler for State {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        &mut self.primary_selection_state
    }
}

impl ExtDataControlHandler for State {
    fn data_control_state(&mut self) -> &mut ExtDataControlState {
        &mut self.ext_data_control_state
    }
}

impl WlrDataControlHandler for State {
    fn data_control_state(&mut self) -> &mut WlrDataControlState {
        &mut self.wlr_data_control_state
    }
}

impl DndGrabHandler for State {
    fn dropped(
        &mut self,
        _target: Option<DndTarget<'_, Self>>,
        _validated: bool,
        _seat: Seat<Self>,
        _location: Point<f64, Logical>,
    ) {
        self.dnd_icon = None;
        self.dnd_active = false;
    }

    fn cancelled(&mut self, _seat: Seat<Self>, _location: Point<f64, Logical>) {
        self.dnd_icon = None;
        self.dnd_active = false;
    }
}

impl WaylandDndGrabHandler for State {
    // Pointer drag-and-drop. The icon is drawn at the pointer until the drop; touch DnD
    // waits for touch input.
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        let pointer = seat.get_pointer();
        let start_data = pointer.as_ref().and_then(|p| p.grab_start_data());
        match (type_, pointer, start_data) {
            (GrabType::Pointer, Some(pointer), Some(start_data)) => {
                self.dnd_active = true;
                self.dnd_icon = icon.map(|surface| DndIcon {
                    surface,
                    offset: Point::default(),
                });
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                pointer.set_grab(self, grab, serial, Focus::Keep);
            }
            _ => source.cancel(),
        }
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new_wayland_window(surface);
        self.assign_window_id(&window);
        self.unmapped.push(window);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let surface = surface.wl_surface();
        self.unmapped
            .retain(|w| w.toplevel().is_none_or(|t| t.wl_surface() != surface));
        if let Some(window) = self.window_for_surface(surface) {
            self.window_destroyed(&window);
        }
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        if let Some(seat) = Seat::<State>::from_resource(&seat) {
            self.start_move(surface.wl_surface(), &seat, serial);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        if let Some(seat) = Seat::<State>::from_resource(&seat) {
            self.start_resize(surface.wl_surface(), &seat, serial, edges);
        }
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.any_window_for_toplevel(&surface) {
            self.set_zoomed(&window, true, None);
        } else {
            surface.send_configure();
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.any_window_for_toplevel(&surface) {
            self.set_zoomed(&window, false, None);
        } else {
            surface.send_configure();
        }
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(window) = self.window_for_surface(surface.wl_surface()) {
            self.minimize_window(&window);
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_changed(&surface);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_changed(&surface);
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
        self.toplevel_changed(&surface);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        if let Err(err) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            warn!(?err, "failed to track popup");
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    /// Popup grabs: menus get the keyboard and close on a click outside them.
    /// Adapted from Smithay's `anvil` (MIT): `anvil/src/shell/xdg.rs`.
    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(seat) = Seat::<State>::from_resource(&seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        if self
            .space
            .elements()
            .all(|w| w.toplevel().is_none_or(|t| t.wl_surface() != &root))
        {
            return;
        }
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else {
            return;
        };
        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }
}

impl State {
    /// The window for `surface`, mapped, minimised or not mapped yet.
    fn any_window_for_toplevel(&self, surface: &ToplevelSurface) -> Option<Window> {
        self.window_for_surface(surface.wl_surface()).or_else(|| {
            self.unmapped
                .iter()
                .find(|w| w.toplevel() == Some(surface))
                .cloned()
        })
    }

    fn toplevel_changed(&mut self, surface: &ToplevelSurface) {
        if let Some(window) = self.window_for_surface(surface.wl_surface()) {
            self.update_foreign_toplevel(&window);
            self.notify_window_changed(&window);
        }
    }
}

/// How long an activation token stays usable.
const ACTIVATION_TOKEN_LIFETIME: Duration = Duration::from_secs(10);

/// Whether a token may take focus, decided when it is created (see `token_created`).
#[derive(Debug, Clone, Copy)]
struct TokenGrant(bool);

impl XdgActivationHandler for State {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    /// A token may take focus if the client that asked for it had keyboard focus at the
    /// time, and any serial it passed is no older than that focus. Tokens are always
    /// accepted so that refused activations can still ask for attention.
    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        let focused_client = self
            .keyboard
            .current_focus()
            .and_then(|s| s.client())
            .map(|c| c.id());
        let from_focused = data.client_id.is_some() && data.client_id == focused_client;
        let serial_ok = match &data.serial {
            None => true,
            Some((serial, seat)) => {
                Seat::<State>::from_resource(seat).as_ref() == Some(&self.seat)
                    && self
                        .keyboard
                        .last_enter()
                        .is_some_and(|enter| serial.is_no_older_than(&enter))
            }
        };
        let granted = from_focused && serial_ok;
        debug!(granted, app_id = ?data.app_id, "activation token created");
        data.user_data
            .insert_if_missing_threadsafe(|| TokenGrant(granted));
        self.xdg_activation_state
            .retain_tokens(|_, d| d.timestamp.elapsed() < ACTIVATION_TOKEN_LIFETIME);
        true
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        let granted = token_data
            .user_data
            .get::<TokenGrant>()
            .is_some_and(|g| g.0)
            && token_data.timestamp.elapsed() < ACTIVATION_TOKEN_LIFETIME;
        self.xdg_activation_state.remove_token(&token);
        let Some(window) = self.window_for_surface(&surface) else {
            return;
        };
        debug!(granted, "activation requested");
        if granted {
            self.focus_window(&window);
        } else {
            self.request_attention(&window);
        }
    }
}

smithay::delegate_dispatch2!(State);
