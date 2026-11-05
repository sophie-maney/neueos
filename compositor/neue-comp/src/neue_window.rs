//! `neue_window_v1` (task 2.35, ADR 0064, `docs/protocols/neue-window-v1.md`): what a
//! window that draws its own chrome tells us about it.
//!
//! Version 1 carries the chrome's height: where its title bar and toolbar end, in
//! surface-local coordinates. It is double-buffered like the protocol says, kept in the
//! surface's cached state, and read by [`chrome_height`]; sheets hang from that line
//! (`sheets.rs`). Unprivileged: sandboxed apps draw chrome too.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use neue_protocols::neue_window::server::{
    neue_window_manager_v1::{self, NeueWindowManagerV1},
    neue_window_v1::{self, NeueWindowV1},
};
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, DisplayHandle, New, Resource, Weak, backend::GlobalId,
};
use smithay::wayland::compositor::{Cacheable, with_states};
use smithay::wayland::{Dispatch2, GlobalDispatch2};

use crate::state::State;

/// Version of `neue_window_manager_v1` we implement.
const VERSION: u32 = 1;

/// The window facts of a surface, double-buffered: applied on `wl_surface.commit`.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowCachedState {
    /// `set_chrome_height`, surface-local logical pixels; `None` until set.
    pub chrome_height: Option<u32>,
}

impl Cacheable for WindowCachedState {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        *self
    }

    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

/// Marks a surface that has a `neue_window_v1` (one is allowed at a time).
#[derive(Debug, Default)]
struct HasWindow(AtomicBool);

/// The chrome height `surface` committed, if its client announced one.
pub fn chrome_height(surface: &WlSurface) -> Option<u32> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<WindowCachedState>()
            .current()
            .chrome_height
    })
}

/// The `neue_window_manager_v1` global.
#[derive(Debug)]
pub struct NeueWindowState {
    _global: GlobalId,
}

impl NeueWindowState {
    pub fn new(dh: &DisplayHandle) -> Self {
        Self {
            _global: dh.create_global::<State, NeueWindowManagerV1, _>(VERSION, WindowGlobal),
        }
    }
}

/// User data of the global and of bound managers.
#[derive(Debug)]
pub struct WindowGlobal;

/// User data of a `neue_window_v1`: its surface (inert once the surface is gone).
#[derive(Debug)]
pub struct WindowObject(Mutex<Weak<WlSurface>>);

impl WindowObject {
    fn surface(&self) -> Option<WlSurface> {
        self.0.lock().ok()?.upgrade().ok()
    }
}

impl GlobalDispatch2<NeueWindowManagerV1, State> for WindowGlobal {
    fn bind(
        &self,
        _state: &mut State,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<NeueWindowManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        data_init.init(resource, WindowGlobal);
    }
}

impl Dispatch2<NeueWindowManagerV1, State> for WindowGlobal {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        manager: &NeueWindowManagerV1,
        request: neue_window_manager_v1::Request,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            neue_window_manager_v1::Request::GetWindow { id, surface } => {
                let taken = with_states(&surface, |states| {
                    states
                        .data_map
                        .get_or_insert_threadsafe(HasWindow::default)
                        .0
                        .swap(true, Ordering::AcqRel)
                });
                if taken {
                    manager.post_error(
                        neue_window_manager_v1::Error::WindowExists,
                        "the surface already has a neue_window_v1",
                    );
                } else {
                    data_init.init(id, WindowObject(Mutex::new(surface.downgrade())));
                }
            }
            neue_window_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch2<NeueWindowV1, State> for WindowObject {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _resource: &NeueWindowV1,
        request: neue_window_v1::Request,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let Some(surface) = self.surface() else {
            return;
        };
        if let neue_window_v1::Request::SetChromeHeight { height } = request {
            with_states(&surface, |states| {
                states
                    .cached_state
                    .get::<WindowCachedState>()
                    .pending()
                    .chrome_height = Some(height);
            });
        }
        // `destroy` is handled in `destroyed`, which also runs for a dead client.
    }

    /// Every fact goes back to unset on the next commit, and the surface may get a new
    /// object (spec).
    fn destroyed(&self, _state: &mut State, _client: ClientId, _resource: &NeueWindowV1) {
        if let Some(surface) = self.surface() {
            with_states(&surface, |states| {
                if let Some(marker) = states.data_map.get::<HasWindow>() {
                    marker.0.store(false, Ordering::Release);
                }
                *states.cached_state.get::<WindowCachedState>().pending() =
                    WindowCachedState::default();
            });
        }
    }
}
