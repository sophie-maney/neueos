//! The smaller protocols the shell and apps rely on (task 1.12, ADR 0015):
//! `ext-idle-notify` and `idle-inhibit`, `ext-foreign-toplevel-list`, `xdg-foreign`,
//! `xdg-dialog`, `xdg-toplevel-icon`, `xdg-system-bell` and `keyboard-shortcuts-inhibit`.
//!
//! The layer shell and the session lock are bigger and have their own modules
//! (`layer.rs`, `lock.rs`).

use smithay::desktop::Window;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::foreign_toplevel_list::{
    ForeignToplevelHandle, ForeignToplevelListHandler, ForeignToplevelListState,
};
use smithay::wayland::idle_inhibit::IdleInhibitHandler;
use smithay::wayland::idle_notify::{IdleNotifierHandler, IdleNotifierState};
use smithay::wayland::keyboard_shortcuts_inhibit::{
    KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
    KeyboardShortcutsInhibitorSeat,
};
use smithay::wayland::shell::xdg::dialog::{ToplevelDialogHint, XdgDialogHandler};
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData};
use smithay::wayland::xdg_foreign::{XdgForeignHandler, XdgForeignState};
use smithay::wayland::xdg_system_bell::XdgSystemBellHandler;
use smithay::wayland::xdg_toplevel_icon::XdgToplevelIconHandler;
use tracing::debug;

use crate::state::State;

impl IdleNotifierHandler for State {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Self> {
        &mut self.idle_notifier_state
    }
}

impl IdleInhibitHandler for State {
    fn inhibit(&mut self, surface: WlSurface) {
        self.idle_inhibitors.push(surface);
        self.refresh_idle_inhibit();
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        // One entry per inhibitor: a surface may hold several.
        if let Some(index) = self.idle_inhibitors.iter().position(|s| s == &surface) {
            self.idle_inhibitors.swap_remove(index);
        }
        self.refresh_idle_inhibit();
    }
}

impl ForeignToplevelListHandler for State {
    fn foreign_toplevel_list_state(&mut self) -> &mut ForeignToplevelListState {
        &mut self.foreign_toplevel_list_state
    }
}

impl XdgForeignHandler for State {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.xdg_foreign_state
    }
}

impl XdgDialogHandler for State {
    fn dialog_hint_changed(&mut self, toplevel: ToplevelSurface, hint: ToplevelDialogHint) {
        debug!(?hint, "dialog hint changed");
        if let Some(window) = self.window_for_surface(toplevel.wl_surface()) {
            // A modal dialog becomes a sheet (task 2.23c).
            self.attach_sheets();
            self.notify_window_changed(&window);
        }
    }
}

impl XdgToplevelIconHandler for State {
    // Icons are stored on the surface by Smithay; the dock reads them (Phase 3).
}

impl XdgSystemBellHandler for State {
    /// The bell asks for attention, like a dock bounce on the original. A bell with no
    /// surface, or one that isn't a window, has nowhere to point at yet (the sound comes
    /// with the audio service).
    fn ring(&mut self, surface: Option<WlSurface>) {
        let window = surface.and_then(|s| self.window_for_surface(&s));
        match window {
            Some(window) => self.request_attention(&window),
            None => debug!("system bell"),
        }
    }
}

impl KeyboardShortcutsInhibitHandler for State {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.shortcuts_inhibit_state
    }

    /// Accept at once when the surface has the keyboard, as the protocol allows; otherwise
    /// it is activated when it gets it (see [`State::update_shortcut_inhibitor`]).
    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        if self.keyboard.current_focus().as_ref() == Some(inhibitor.wl_surface()) {
            inhibitor.activate();
            self.active_inhibitor = Some(inhibitor);
        }
    }

    fn inhibitor_destroyed(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        if self
            .active_inhibitor
            .as_ref()
            .is_some_and(|active| active.wl_surface() == inhibitor.wl_surface())
        {
            self.active_inhibitor = None;
        }
    }
}

impl State {
    /// Tell `ext-idle-notify` that the user did something.
    pub(crate) fn notify_activity(&mut self) {
        self.idle_notifier_state.notify_activity(&self.seat);
    }

    /// Idle notifications pause while any surface holds an idle inhibitor. Not tied to
    /// visibility yet (ADR 0015).
    fn refresh_idle_inhibit(&mut self) {
        self.idle_inhibitors.retain(Resource::is_alive);
        let inhibited = !self.idle_inhibitors.is_empty();
        self.idle_notifier_state.set_is_inhibited(inhibited);
    }

    /// Keyboard focus moved to `focused`: the old surface's shortcut inhibitor goes quiet
    /// and the new one's (if it has one) becomes active.
    pub(crate) fn update_shortcut_inhibitor(&mut self, focused: Option<&WlSurface>) {
        if let Some(old) = self.active_inhibitor.take()
            && old.is_active()
        {
            old.inactivate();
        }
        let Some(surface) = focused else {
            return;
        };
        if let Some(inhibitor) = self.seat.keyboard_shortcuts_inhibitor_for_surface(surface) {
            inhibitor.activate();
            self.active_inhibitor = Some(inhibitor);
        }
    }

    /// Whether key bindings are off because the focused client asked to keep its keys.
    pub(crate) fn shortcuts_inhibited(&self) -> bool {
        self.active_inhibitor.is_some()
    }

    /// Announce a window that just mapped on `ext-foreign-toplevel-list`. The identifier
    /// is `window-<id>`, so the shell can match it to `neuectl`'s window id.
    pub(crate) fn announce_toplevel(&mut self, window: &Window) {
        let Some(id) = self.window_id(window) else {
            return;
        };
        let (title, app_id) = crate::wm::title_and_app_id(window);
        let handle = self
            .foreign_toplevel_list_state
            .new_toplevel_with_identifier::<State>(
                title.unwrap_or_default(),
                app_id.unwrap_or_default(),
                format!("window-{}", id.0),
            );
        window.user_data().insert_if_missing_threadsafe(|| handle);
    }

    /// Send a changed title or app id of `window` on `ext-foreign-toplevel-list`.
    pub(crate) fn update_foreign_toplevel(&mut self, window: &Window) {
        let Some(handle) = window.user_data().get::<ForeignToplevelHandle>() else {
            return;
        };
        let (title, app_id) = crate::wm::title_and_app_id(window);
        let (title, app_id) = (title.unwrap_or_default(), app_id.unwrap_or_default());
        let mut changed = false;
        if handle.title() != title {
            handle.send_title(&title);
            changed = true;
        }
        if handle.app_id() != app_id {
            handle.send_app_id(&app_id);
            changed = true;
        }
        if changed {
            handle.send_done();
        }
    }

    /// Tell `ext-foreign-toplevel-list` clients that `window` is gone.
    pub(crate) fn retire_foreign_toplevel(&mut self, window: &Window) {
        if let Some(handle) = window.user_data().get::<ForeignToplevelHandle>() {
            self.stop_toplevel_captures(handle);
            self.foreign_toplevel_list_state.remove_toplevel(handle);
        }
    }

    /// The `xdg-dialog` hint of `window`'s toplevel: the client says it is a dialog, or a
    /// modal one. A modal dialog with a parent is placed as a sheet (`sheets.rs`).
    pub fn dialog_hint(&self, window: &Window) -> ToplevelDialogHint {
        let Some(toplevel) = window.toplevel() else {
            return ToplevelDialogHint::Unknown;
        };
        with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok())
                .map_or(ToplevelDialogHint::Unknown, |data| data.dialog_hint)
        })
    }
}
