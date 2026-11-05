//! `ext-session-lock-v1`: the lock screen (task 1.12, ADR 0015).
//!
//! While the session is locked the compositor shows nothing but lock surfaces (a plain
//! background on an output without one), and only they get input. The lock stays when its
//! client dies, until another client locks and unlocks (as the protocol asks).
//!
//! Renderers don't see [`State`], so what they need lives in each output's user data
//! ([`OutputLock`]): whether it is locked and its lock surface.

use std::sync::Mutex;

use smithay::input::pointer::PointerHandle;
use smithay::output::Output;
use smithay::reexports::wayland_protocols::ext::session_lock::v1::server::ext_session_lock_v1::ExtSessionLockV1;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::session_lock::{
    LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
};
use tracing::{debug, info};

use crate::command_key::KeyFocus;
use crate::state::State;

/// What an output shows while the session is locked. Kept in the output's user data.
#[derive(Debug, Default)]
pub struct OutputLock {
    /// The session is locked: draw lock surfaces only.
    pub locked: bool,
    /// The lock surface a client made for this output.
    pub surface: Option<LockSurface>,
}

/// The lock data of `output`, locked for the duration of `f`.
pub fn with_output_lock<T>(output: &Output, f: impl FnOnce(&mut OutputLock) -> T) -> T {
    let data = output
        .user_data()
        .get_or_insert_threadsafe(|| Mutex::new(OutputLock::default()));
    // A poisoned lock only means another thread panicked mid-update; the data is plain.
    let mut guard = data.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// The session lock, as far as the compositor tracks it.
#[derive(Debug, Default)]
pub struct SessionLockStatus {
    /// Whether the session is locked.
    pub locked: bool,
    /// The lock object of the client that holds the lock. It outlives the client's
    /// surfaces but not the client: a dead owner leaves the session locked and open to a
    /// new locker.
    owner: Option<ExtSessionLockV1>,
    /// The surface that had keyboard focus when the session locked, given it back on
    /// unlock if it is still there.
    focus_before: Option<WlSurface>,
}

impl State {
    /// Whether the session is locked.
    pub fn is_locked(&self) -> bool {
        self.session_lock.locked
    }

    /// The lock surface under `pos` (global logical coordinates) and its origin.
    pub fn lock_surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let output = self.space.output_under(pos).next()?;
        let geo = self.space.output_geometry(output)?;
        let surface =
            with_output_lock(output, |lock| lock.surface.clone()).filter(LockSurface::alive)?;
        // Lock surfaces fill their output and have no subsurfaces worth hit-testing.
        Some((surface.wl_surface().clone(), geo.loc.to_f64()))
    }

    /// Mark a freshly added output as locked if the session is.
    pub(crate) fn lock_new_output(&self, output: &Output) {
        if self.is_locked() {
            with_output_lock(output, |lock| lock.locked = true);
        }
    }

    /// Size `surface` to fill `output`.
    fn size_lock_surface(surface: &LockSurface, output: &Output) {
        let size = output
            .current_mode()
            .map(|mode| {
                output
                    .current_transform()
                    .transform_size(mode.size)
                    .to_f64()
                    .to_logical(output.current_scale().fractional_scale())
                    .to_i32_ceil::<i32>()
            })
            .unwrap_or_default();
        // Smithay sends the configure once `new_surface` returns.
        surface.with_pending_state(|state| {
            state.size = Some((size.w.max(0) as u32, size.h.max(0) as u32).into());
        });
    }

    /// Re-point pointer focus after the lock changed what is under the pointer.
    fn refresh_pointer_focus(&mut self) {
        let pos = self.pointer.current_location();
        self.move_pointer(pos, smithay::backend::input::InputTime::now());
    }
}

impl SessionLockHandler for State {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.lock_manager_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        // A second locker is refused while the first is alive (dropping the locker tells
        // it `finished`), but may take over from a dead one.
        if self
            .session_lock
            .owner
            .as_ref()
            .is_some_and(|owner| owner.is_alive())
        {
            debug!("session lock refused: already locked");
            return;
        }
        info!("session locked");
        let first = !self.session_lock.locked;
        self.session_lock.owner = Some(confirmation.ext_session_lock().clone());
        self.session_lock.locked = true;
        if first {
            self.session_lock.focus_before = self.keyboard_focus();
        }
        for output in self.outputs().cloned().collect::<Vec<_>>() {
            with_output_lock(&output, |lock| lock.locked = true);
        }

        // Nothing but the lock gets input from here on: end grabs and drags, and take
        // the keyboard away from whoever has it.
        let pointer: PointerHandle<State> = self.pointer.clone();
        pointer.unset_grab(
            self,
            SERIAL_COUNTER.next_serial(),
            smithay::backend::input::InputTime::now(),
        );
        self.dnd_active = false;
        for window in self.space.elements() {
            window.set_activated(false);
        }
        let keyboard = self.keyboard.clone();
        keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
        // Smithay does not call `focus_changed` when focus is dropped.
        self.refresh_pointer_constraint_for(None);
        self.send_pending_configures();
        self.refresh_pointer_focus();

        // The next frame shows only the lock, so tell the client right away.
        confirmation.lock();
    }

    fn unlock(&mut self) {
        info!("session unlocked");
        self.session_lock.locked = false;
        self.session_lock.owner = None;
        for output in self.outputs().cloned().collect::<Vec<_>>() {
            with_output_lock(&output, |lock| {
                lock.locked = false;
                lock.surface = None;
            });
        }
        self.refresh_pointer_focus();
        let before = self.session_lock.focus_before.take();
        match before.filter(Resource::is_alive) {
            Some(surface) => match self.window_for_surface(&surface) {
                Some(window) => self.focus_window(&window),
                None => match self.layer_for_surface(&surface) {
                    Some((layer, _)) if layer.can_receive_keyboard_focus() => {
                        self.focus_layer(&surface);
                    }
                    _ => self.focus_topmost(),
                },
            },
            None => self.focus_topmost(),
        }
        if let Some(exclusive) = self.exclusive_layer_surface() {
            self.focus_layer(&exclusive);
        }
        self.report_focus(self.focused_window_id());
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        let Some(output) = Output::from_resource(&output) else {
            return;
        };
        debug!(output = output.name(), "lock surface");
        with_output_lock(&output, |lock| lock.surface = Some(surface.clone()));
        Self::size_lock_surface(&surface, &output);
        // The first lock surface gets the keyboard; later ones when they are clicked.
        if self.keyboard_focus().is_none() {
            let keyboard = self.keyboard.clone();
            keyboard.set_focus(
                self,
                Some(KeyFocus(surface.wl_surface().clone())),
                SERIAL_COUNTER.next_serial(),
            );
        }
        self.refresh_pointer_focus();
    }
}

/// Frame callbacks for the lock surface of `output`, so the locker can draw.
pub fn send_lock_frames(output: &Output, time: std::time::Duration) {
    let Some(surface) = with_output_lock(output, |lock| lock.surface.clone()) else {
        return;
    };
    if surface.alive() {
        smithay::desktop::utils::send_frames_surface_tree(
            surface.wl_surface(),
            output,
            time,
            Some(std::time::Duration::ZERO),
            |_, _| Some(output.clone()),
        );
    }
}

impl State {
    /// A press while locked: the lock surface under the pointer gets the keyboard.
    pub(crate) fn click_lock_surface(&mut self, location: Point<f64, Logical>) {
        let Some((surface, _)) = self.lock_surface_under(location) else {
            return;
        };
        if self.keyboard_focus().as_ref() != Some(&surface) {
            let keyboard = self.keyboard.clone();
            keyboard.set_focus(self, Some(KeyFocus(surface)), SERIAL_COUNTER.next_serial());
        }
    }
}
