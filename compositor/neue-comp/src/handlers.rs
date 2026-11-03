//! Wayland protocol handlers.
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/handlers/` and, for dmabuf,
//! `anvil/src/winit.rs`.

use neue_ipc::Event;
use smithay::backend::allocator::dmabuf::Dmabuf;
#[cfg(feature = "winit")]
use smithay::backend::renderer::ImportDma;
use smithay::backend::renderer::utils::{RendererSurfaceStateUserData, on_commit_buffer_handler};
use smithay::desktop::{PopupKind, Window, find_popup_root_surface, get_popup_toplevel_coords};
use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::{CursorImageStatus, Focus};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_seat, wl_surface::WlSurface};
use smithay::reexports::wayland_server::{Client, Resource};
use smithay::utils::Serial;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface,
    with_states,
};
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::pointer_constraints::PointerConstraintsHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use tracing::{debug, warn};

use crate::placement;
use crate::state::{Backend, ClientState, State};

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
            }
        }

        self.commit_unmapped(surface);
        self.commit_popup(surface);
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
        let output = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
            .map(|g| (g.loc.x, g.loc.y, g.size.w, g.size.h))
            .unwrap_or((0, 0, 0, 0));
        let size = window.geometry().size;
        let (x, y) = placement::cascade(output, (size.w, size.h), self.placed);
        self.placed += 1;
        debug!(x, y, w = size.w, h = size.h, "mapping window");
        self.space.map_element(window.clone(), (x, y), true);
        if let Some(info) = self.window_info(&window, None) {
            self.emit(Event::WindowOpened(info));
        }
        self.focus_window(&window);
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

    /// Keep a popup inside the output it would otherwise overflow.
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self.window_for_surface(&root) else {
            return;
        };
        let Some(output_geo) = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
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

    // Client cursors come with task 1.9; until then the host (winit) cursor is shown.
    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        // The clipboard follows keyboard focus.
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);

        // Tell `neuectl` subscribers, once per actual change.
        let window = focused
            .and_then(|s| self.window_for_surface(s))
            .and_then(|w| self.window_id(&w));
        self.report_focus(window);
    }
}

impl OutputHandler for State {}

impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for State {}

impl WaylandDndGrabHandler for State {
    // Pointer drag-and-drop, as in smallvil. Drag icons and touch DnD are task 1.8.
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        let pointer = seat.get_pointer();
        let start_data = pointer.as_ref().and_then(|p| p.grab_start_data());
        match (type_, pointer, start_data) {
            (GrabType::Pointer, Some(pointer), Some(start_data)) => {
                let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
                pointer.set_grab(self, grab, serial, Focus::Keep);
            }
            _ => source.cancel(),
        }
    }
}

// Required by the seat machinery; the protocol itself is task 1.13.
impl PointerConstraintsHandler for State {}

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
            let was_focused = self.keyboard.current_focus().as_ref() == Some(surface);
            let id = self.window_id(&window);
            self.space.unmap_elem(&window);
            if let Some(window) = id {
                self.emit(Event::WindowClosed { window });
            }
            if was_focused {
                self.focus_topmost();
                // Smithay does not call `focus_changed` when the destroyed surface already
                // cleared the focus, so report it ourselves (a no-op if it was reported).
                self.report_focus(self.focused_window_id());
            }
        }
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

    // Popup grabs (menus that close on outside click) come with task 1.5.
    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
}

smithay::delegate_dispatch2!(State);
