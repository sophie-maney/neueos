//! AccessKit on Linux: one `accesskit_unix` adapter (AT-SPI over D-Bus) per window
//! (task 2.6, ADR 0034 §4).
//!
//! The adapter calls its handlers on its own thread. They only forward the request to the
//! event loop through a calloop channel; the loop turns it into Masonry window and access
//! events, and the tree goes out with the next frame (as `accesskit_winit` does).

use masonry_core::accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Rect, TreeUpdate,
};
use masonry_core::core::WindowEvent;
use smithay_client_toolkit::reexports::calloop::channel::Sender;

use super::State;
use crate::WindowId;

/// A request from an assistive technology, for one window.
pub(super) enum AccessEvent {
    /// A screen reader started: Masonry must build the tree.
    InitialTree,
    Action(ActionRequest),
    Deactivated,
}

/// Forwards the adapter's callbacks to the event loop.
#[derive(Clone)]
struct Forward {
    window: WindowId,
    tx: Sender<(WindowId, AccessEvent)>,
}

impl Forward {
    fn send(&self, event: AccessEvent) {
        // The loop has ended when the receiver is gone; nothing is left to tell.
        let _ = self.tx.send((self.window, event));
    }
}

impl ActivationHandler for Forward {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // Built on the loop; the adapter waits for the first `update_if_active`.
        self.send(AccessEvent::InitialTree);
        None
    }
}

impl ActionHandler for Forward {
    fn do_action(&mut self, request: ActionRequest) {
        self.send(AccessEvent::Action(request));
    }
}

impl DeactivationHandler for Forward {
    fn deactivate_accessibility(&mut self) {
        self.send(AccessEvent::Deactivated);
    }
}

/// A window's adapter.
pub(super) struct Access(accesskit_unix::Adapter);

impl Access {
    pub(super) fn new(window: WindowId, tx: Sender<(WindowId, AccessEvent)>) -> Self {
        let forward = Forward { window, tx };
        Self(accesskit_unix::Adapter::new(
            forward.clone(),
            forward.clone(),
            forward,
        ))
    }

    /// Wayland hides window positions, so the window sits at the origin; `size` is in
    /// physical pixels, like the tree's coordinates.
    pub(super) fn set_size(&mut self, size: (u32, u32)) {
        let r = Rect::new(0.0, 0.0, f64::from(size.0), f64::from(size.1));
        self.0.set_root_window_bounds(r, r);
    }

    pub(super) fn set_focused(&mut self, focused: bool) {
        self.0.update_window_focus_state(focused);
    }

    pub(super) fn update(&mut self, update: TreeUpdate) {
        self.0.update_if_active(|| update);
    }
}

impl State {
    pub(super) fn access_event(&mut self, window: WindowId, event: AccessEvent) {
        let Some(w) = self.reg.windows.get_mut(&window) else {
            return;
        };
        match event {
            AccessEvent::InitialTree => {
                tracing::debug!(%window, "assistive technology attached");
                w.root.handle_window_event(WindowEvent::EnableAccessTree);
                w.dirty = true;
            }
            AccessEvent::Action(request) => w.root.handle_access_event(request),
            AccessEvent::Deactivated => {
                w.root.handle_window_event(WindowEvent::DisableAccessTree);
            }
        }
    }
}
