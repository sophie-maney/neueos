//! `neue_shell_v1` (task 3.1, ADR 0066, `docs/protocols/neue-shell-v1.md`): the private
//! protocol between us and `neue-shell`.
//!
//! The global is only visible to the shell client, the one `shell_launch.rs` spawns on a
//! private connection ([`ClientState::shell`]). Every bound manager gets a
//! `neue_shell_toplevel_v1` per announced window (mapped or minimised, the same set as
//! `ext-foreign-toplevel-list`), and the window's state as a [`Snapshot`]. Changes are found
//! by diffing snapshots wherever `neuectl` subscribers are told about a window
//! (`notify_window_changed`), plus on the commit that applies a new `xdg-toplevel-icon`.
//! Nothing is tracked while no shell is bound.

use std::collections::HashMap;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use neue_ipc::{ActionParams, MenuAddress, WindowId};
use neue_protocols::neue_shell::server::{
    neue_shell_action_v1::NeueShellActionV1,
    neue_shell_manager_v1::{self, Modifiers, NeueShellManagerV1, SystemKey},
    neue_shell_toplevel_v1::{self, NeueShellToplevelV1},
};
use smithay::backend::input::{InputTime, KeyState};
use smithay::desktop::{Window, layer_map_for_output};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId, ObjectId};
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DataInit, DisplayHandle, New, Resource, Weak};
use smithay::utils::{Logical, Rectangle, SERIAL_COUNTER};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shm::{BufferData, with_buffer_contents};
use smithay::wayland::xdg_toplevel_icon::ToplevelIconCachedState;
use smithay::wayland::{Dispatch2, GlobalDispatch2};
use tracing::{debug, warn};

use crate::state::{ClientState, State};

/// Version of `neue_shell_manager_v1` we implement.
/// Version 2 adds `focus_menu_bar` (task 3.7b, ADR 0077); version 3 the toplevel's
/// `command_key` (task 3.10c, ADR 0081).
const VERSION: u32 = 3;

/// Icon images larger than this (per side) are not passed on.
const MAX_ICON_SIZE: i32 = 512;

/// Whether `client` is the shell: the client `shell_launch.rs` (or a test) inserted with
/// [`State::insert_shell_client`].
pub fn is_shell(client: &Client) -> bool {
    client
        .get_data::<ClientState>()
        .is_some_and(|data| data.shell)
}

/// The global and what bound managers have been told. Part of [`State`].
#[derive(Debug)]
pub struct NeueShellState {
    _global: GlobalId,
    managers: Vec<Manager>,
    /// What the managers were last told about each announced window.
    known: HashMap<WindowId, Snapshot>,
    /// The window the shell treats as active (see [`State::shell_refresh_active`]).
    active: Option<Window>,
    /// The id last sent in `active`.
    sent_active: Option<WindowId>,
    /// Keycodes whose press went to the shell as a system key; their release goes too.
    held_keys: HashMap<u32, SystemKey>,
    /// `run_action` outcomes not sent yet: a destructor event can't go out during the
    /// request that created its object, so they go after dispatch
    /// ([`State::shell_send_action_results`]).
    results: Vec<(NeueShellActionV1, Result<(), String>)>,
}

impl NeueShellState {
    pub fn new(dh: &DisplayHandle) -> Self {
        Self {
            _global: dh.create_global::<State, NeueShellManagerV1, _>(VERSION, ShellGlobal),
            managers: Vec::new(),
            known: HashMap::new(),
            active: None,
            sent_active: None,
            held_keys: HashMap::new(),
            results: Vec::new(),
        }
    }

    /// Whether a shell is listening.
    pub fn is_bound(&self) -> bool {
        !self.managers.is_empty()
    }
}

/// One bound `neue_shell_manager_v1` and its toplevel objects.
#[derive(Debug)]
struct Manager {
    resource: NeueShellManagerV1,
    toplevels: HashMap<WindowId, NeueShellToplevelV1>,
}

/// What the shell knows about one window.
#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    title: String,
    app_id: String,
    state: neue_shell_toplevel_v1::State,
    menu: Option<MenuAddress>,
    /// Whether Super reaches the window's app as Control (`command_key`, version 3).
    super_as_ctrl: bool,
    icon: Icon,
}

/// A window's `xdg-toplevel-icon`, with its images copied out of the client's buffers.
#[derive(Debug, Clone, Default)]
struct Icon {
    name: Option<String>,
    /// The client's buffers and scales: an icon is immutable once set, so these identify
    /// it and the copies are only made when they change.
    buffers: Vec<(ObjectId, i32)>,
    images: Vec<Arc<IconImage>>,
}

impl PartialEq for Icon {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.buffers == other.buffers
    }
}

/// One icon image: a sealed memfd of tightly packed ARGB8888 pixels.
#[derive(Debug)]
struct IconImage {
    fd: OwnedFd,
    width: i32,
    height: i32,
    scale: i32,
}

/// Marks a window whose next commit applies a new icon (set by `set_icon`).
#[derive(Debug, Default)]
pub(crate) struct IconPending(AtomicBool);

/// A rectangle on one of the shell's surfaces.
type SurfaceRect = (Weak<WlSurface>, Rectangle<i32, Logical>);

/// Where a window minimises to.
#[derive(Debug, Default)]
struct MinimizeTarget(Mutex<Option<SurfaceRect>>);

/// User data of the global and of bound managers.
#[derive(Debug)]
pub struct ShellGlobal;

/// User data of a `neue_shell_toplevel_v1`: its window's id.
#[derive(Debug)]
pub struct ShellToplevel(WindowId);

/// User data of a `neue_shell_action_v1` (it has no requests).
#[derive(Debug)]
pub struct ShellAction;

impl GlobalDispatch2<NeueShellManagerV1, State> for ShellGlobal {
    fn bind(
        &self,
        state: &mut State,
        _dh: &DisplayHandle,
        _client: &Client,
        resource: New<NeueShellManagerV1>,
        data_init: &mut DataInit<'_, State>,
    ) {
        let resource = data_init.init(resource, ShellGlobal);
        state.shell_manager_bound(resource);
    }

    /// Only the shell sees the global; anyone else binding it gets a protocol error from
    /// libwayland's registry, before [`Self::bind`] runs.
    fn can_view(&self, client: &Client) -> bool {
        is_shell(client)
    }
}

impl Dispatch2<NeueShellManagerV1, State> for ShellGlobal {
    fn request(
        &self,
        state: &mut State,
        _client: &Client,
        _manager: &NeueShellManagerV1,
        request: neue_shell_manager_v1::Request,
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            neue_shell_manager_v1::Request::RunAction {
                result,
                name,
                toplevel,
            } => {
                let result = data_init.init(result, ShellAction);
                let window = toplevel.map(|t| t.data::<ShellToplevel>().map(|d| d.0));
                let outcome = state.shell_run_action(name, window);
                state.neue_shell.results.push((result, outcome));
            }
            neue_shell_manager_v1::Request::SendShortcut { modifiers, keysym } => {
                let modifiers = modifiers.into_result().unwrap_or(Modifiers::empty());
                state.shell_send_shortcut(modifiers, keysym);
            }
            // Handled in `destroyed`, which also runs for a dead client.
            neue_shell_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &NeueShellManagerV1) {
        let shell = &mut state.neue_shell;
        shell.managers.retain(|m| &m.resource != resource);
        if shell.managers.is_empty() {
            shell.known.clear();
            shell.active = None;
            shell.sent_active = None;
            shell.held_keys.clear();
        }
    }
}

impl Dispatch2<NeueShellToplevelV1, State> for ShellToplevel {
    fn request(
        &self,
        state: &mut State,
        client: &Client,
        resource: &NeueShellToplevelV1,
        request: neue_shell_toplevel_v1::Request,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        let Some(window) = state.window_by_id(self.0) else {
            return; // closed: the object is inert
        };
        match request {
            neue_shell_toplevel_v1::Request::Activate => state.focus_window(&window),
            neue_shell_toplevel_v1::Request::Minimize => state.minimize_window(&window),
            neue_shell_toplevel_v1::Request::Unminimize => state.unminimize_window(&window),
            neue_shell_toplevel_v1::Request::Close => {
                if let Some(toplevel) = window.toplevel() {
                    toplevel.send_close();
                }
            }
            neue_shell_toplevel_v1::Request::SetMinimizeRectangle {
                surface,
                x,
                y,
                width,
                height,
            } => {
                if surface.client().as_ref() != Some(client) {
                    debug!(?resource, "minimise rectangle on another client's surface");
                    return;
                }
                debug!(?resource, x, y, width, height, "minimise rectangle");
                let target = (width > 0 && height > 0).then(|| {
                    (
                        surface.downgrade(),
                        Rectangle::new((x, y).into(), (width, height).into()),
                    )
                });
                let data = window
                    .user_data()
                    .get_or_insert_threadsafe(MinimizeTarget::default);
                if let Ok(mut slot) = data.0.lock() {
                    *slot = target;
                }
            }
            neue_shell_toplevel_v1::Request::Destroy => {}
            _ => {}
        }
    }

    fn destroyed(&self, state: &mut State, _client: ClientId, resource: &NeueShellToplevelV1) {
        for manager in &mut state.neue_shell.managers {
            if manager.toplevels.get(&self.0) == Some(resource) {
                manager.toplevels.remove(&self.0);
            }
        }
    }
}

impl Dispatch2<NeueShellActionV1, State> for ShellAction {
    fn request(
        &self,
        _state: &mut State,
        _client: &Client,
        _resource: &NeueShellActionV1,
        _request: <NeueShellActionV1 as Resource>::Request,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
    }
}

impl State {
    /// Add the shell, connected through `stream`: the one client that sees
    /// `neue_shell_manager_v1`.
    pub fn insert_shell_client(
        &mut self,
        stream: std::os::unix::net::UnixStream,
    ) -> std::io::Result<Client> {
        let data = ClientState {
            shell: true,
            ..ClientState::default()
        };
        self.display_handle.insert_client(stream, Arc::new(data))
    }

    /// Send the outcomes of the `run_action` requests just dispatched.
    pub(crate) fn shell_send_action_results(&mut self) {
        for (result, outcome) in self.neue_shell.results.drain(..) {
            match outcome {
                Ok(()) => result.done(),
                Err(message) => result.failed(message),
            }
        }
    }

    /// Where the shell said `window` minimises to, in global logical coordinates. `None`
    /// when it said nothing, or its surface is gone or not a layer surface.
    pub fn minimize_target(&self, window: &Window) -> Option<Rectangle<i32, Logical>> {
        let (surface, rect) = window
            .user_data()
            .get::<MinimizeTarget>()?
            .0
            .lock()
            .ok()?
            .clone()?;
        let surface = surface.upgrade().ok()?;
        let (layer, output) = self.layer_for_surface(&surface)?;
        let layer_loc = layer_map_for_output(&output).layer_geometry(&layer)?.loc;
        let output_loc = self.space.output_geometry(&output)?.loc;
        Some(Rectangle::new(rect.loc + layer_loc + output_loc, rect.size))
    }

    /// A manager was bound: tell it about every announced window, then the active one.
    fn shell_manager_bound(&mut self, resource: NeueShellManagerV1) {
        debug!("the shell bound neue_shell_manager_v1");
        let first = self.neue_shell.managers.is_empty();
        if first {
            let focus = self.keyboard_focus();
            self.neue_shell.active = self.shell_active_window(focus.as_ref(), None);
        }
        let mut manager = Manager {
            resource,
            toplevels: HashMap::new(),
        };
        let announced: Vec<Window> = self
            .minimized
            .iter()
            .map(|(w, _)| w)
            .chain(self.space.elements())
            .filter(|w| Self::with_wm_data(w, |d| d.reported.is_some()))
            .cloned()
            .collect();
        for window in &announced {
            let Some(id) = self.window_id(window) else {
                continue;
            };
            let snapshot = match self.neue_shell.known.get(&id) {
                Some(snapshot) => snapshot.clone(),
                None => {
                    let Some(snapshot) = self.shell_snapshot(window, None) else {
                        continue;
                    };
                    self.neue_shell.known.insert(id, snapshot.clone());
                    snapshot
                }
            };
            self.announce_to(&mut manager, id, &snapshot);
        }
        let active = self
            .neue_shell
            .active
            .as_ref()
            .and_then(|w| self.window_id(w));
        manager
            .resource
            .active(active.and_then(|id| manager.toplevels.get(&id)));
        self.neue_shell.sent_active = active;
        self.neue_shell.managers.push(manager);
    }

    /// Make the toplevel object for window `id` on `manager` and send all of `snapshot`.
    fn announce_to(&self, manager: &mut Manager, id: WindowId, snapshot: &Snapshot) {
        let resource = &manager.resource;
        let Ok(client) = self.display_handle.get_client(resource.id()) else {
            return;
        };
        let Ok(toplevel) = client.create_resource::<NeueShellToplevelV1, _, State>(
            &self.display_handle,
            resource.version(),
            ShellToplevel(id),
        ) else {
            return;
        };
        resource.toplevel(&toplevel);
        toplevel.identifier(format!("window-{}", id.0));
        send_changes(&toplevel, None, snapshot);
        toplevel.done();
        manager.toplevels.insert(id, toplevel);
    }

    /// What the shell should know about `window`. The icon images are reused from
    /// `previous` when the icon did not change.
    fn shell_snapshot(&self, window: &Window, previous: Option<&Icon>) -> Option<Snapshot> {
        let info = self.window_info(window, None)?;
        let mut state = neue_shell_toplevel_v1::State::empty();
        state.set(neue_shell_toplevel_v1::State::Minimized, info.minimized);
        state.set(
            neue_shell_toplevel_v1::State::Attention,
            info.demands_attention,
        );
        let icon = window
            .toplevel()
            .map(|t| read_icon(t.wl_surface(), previous))
            .unwrap_or_default();
        Some(Snapshot {
            title: info.title.unwrap_or_default(),
            app_id: info.app_id.unwrap_or_default(),
            state,
            menu: info.menu,
            super_as_ctrl: window
                .toplevel()
                .is_some_and(|t| self.window_super_as_ctrl(t.wl_surface())),
            icon,
        })
    }

    /// A window was mapped (`map_new_window`).
    pub(crate) fn shell_window_opened(&mut self, window: &Window) {
        if !self.neue_shell.is_bound() {
            return;
        }
        let (Some(id), Some(snapshot)) =
            (self.window_id(window), self.shell_snapshot(window, None))
        else {
            return;
        };
        let mut managers = std::mem::take(&mut self.neue_shell.managers);
        for manager in &mut managers {
            self.announce_to(manager, id, &snapshot);
        }
        self.neue_shell.managers = managers;
        self.neue_shell.known.insert(id, snapshot);
    }

    /// Something about `window` may have changed: send what did.
    pub(crate) fn shell_window_changed(&mut self, window: &Window) {
        if !self.neue_shell.is_bound() {
            return;
        }
        let Some(id) = self.window_id(window) else {
            return;
        };
        let Some(old) = self.neue_shell.known.get(&id) else {
            return; // not announced yet
        };
        let Some(new) = self.shell_snapshot(window, Some(&old.icon)) else {
            return;
        };
        if &new != old {
            for manager in &self.neue_shell.managers {
                if let Some(toplevel) = manager.toplevels.get(&id) {
                    send_changes(toplevel, Some(old), &new);
                    toplevel.done();
                }
            }
            self.neue_shell.known.insert(id, new);
        }
        // A minimised active window stops being active.
        self.shell_refresh_active();
    }

    /// The remap settings changed (config reload): send each window whose `command_key`
    /// did.
    pub(crate) fn shell_command_keys_changed(&mut self) {
        if !self.neue_shell.is_bound() {
            return;
        }
        let windows: Vec<Window> = self
            .minimized
            .iter()
            .map(|(w, _)| w.clone())
            .chain(self.space.elements().cloned())
            .collect();
        for window in &windows {
            self.shell_window_changed(window);
        }
    }

    /// `window`'s root surface committed: send a new icon if this commit applied one.
    pub(crate) fn shell_window_committed(&mut self, window: &Window) {
        let pending = window
            .user_data()
            .get::<IconPending>()
            .is_some_and(|p| p.0.swap(false, Ordering::AcqRel));
        if pending {
            self.shell_window_changed(window);
        }
    }

    /// The client set a new icon on `surface`; it applies on the next commit.
    pub(crate) fn shell_icon_set(&mut self, surface: &WlSurface) {
        if let Some(window) = self.window_for_surface(surface) {
            window
                .user_data()
                .get_or_insert_threadsafe(IconPending::default)
                .0
                .store(true, Ordering::Release);
        }
    }

    /// A window is gone (`window_destroyed`).
    pub(crate) fn shell_window_closed(&mut self, window: &Window) {
        if self.neue_shell.active.as_ref() == Some(window) {
            self.neue_shell.active = None;
            self.send_active();
        }
        let Some(id) = self.window_id(window) else {
            return;
        };
        self.neue_shell.known.remove(&id);
        for manager in &mut self.neue_shell.managers {
            if let Some(toplevel) = manager.toplevels.remove(&id) {
                toplevel.closed();
            }
        }
    }

    /// Recompute the active window and tell the shell if it changed: after window
    /// changes, and where focus is dropped without `focus_changed`. Must not run while
    /// Smithay holds the keyboard's lock (use [`Self::shell_focus_changed`] there).
    pub(crate) fn shell_refresh_active(&mut self) {
        let focus = self.keyboard_focus();
        self.shell_focus_changed(focus.as_ref());
    }

    /// Keyboard focus is now `focus`: recompute the active window and tell the shell if
    /// it changed.
    pub(crate) fn shell_focus_changed(&mut self, focus: Option<&WlSurface>) {
        if !self.neue_shell.is_bound() {
            return;
        }
        let previous = self.neue_shell.active.take();
        self.neue_shell.active = self.shell_active_window(focus, previous);
        self.send_active();
    }

    /// The window whose app owns the menu bar: the focused window; while something else
    /// (a layer surface, a popup, the lock screen) has the keyboard, `previous` if it is
    /// still shown; with no keyboard focus, none.
    fn shell_active_window(
        &self,
        focus: Option<&WlSurface>,
        previous: Option<Window>,
    ) -> Option<Window> {
        let focus = focus?;
        if let Some(window) = self.window_for_surface(focus) {
            return Some(window);
        }
        previous.filter(|w| self.space.elements().any(|m| m == w) && !self.is_minimized(w))
    }

    /// Send `active` to every manager if it changed.
    fn send_active(&mut self) {
        let id = self
            .neue_shell
            .active
            .as_ref()
            .and_then(|w| self.window_id(w));
        if id == self.neue_shell.sent_active {
            return;
        }
        self.neue_shell.sent_active = id;
        for manager in &self.neue_shell.managers {
            let toplevel = id.and_then(|id| manager.toplevels.get(&id));
            manager.resource.active(toplevel);
        }
    }

    /// `run_action`: like `neuectl action`, but no window means the active one.
    fn shell_run_action(
        &mut self,
        name: String,
        window: Option<Option<WindowId>>,
    ) -> Result<(), String> {
        let window = match window {
            Some(Some(id)) => Some(id),
            Some(None) => return Err("that window is gone".into()),
            None => self
                .neue_shell
                .active
                .as_ref()
                .and_then(|w| self.window_id(w)),
        };
        let params = ActionParams {
            name,
            window,
            ..ActionParams::default()
        };
        self.run_action(params).map_err(|err| err.message)
    }

    /// The window the shell treats as active (it stays while a shell menu has the
    /// keyboard).
    pub(crate) fn shell_active(&self) -> Option<Window> {
        self.neue_shell.active.clone()
    }

    /// The `focus-menu-bar` action: ask the shell to open its menu bar's first menu from
    /// the keyboard, on the active window's output (else the pointer's). Fails when no
    /// shell that knows the event (version 2) is bound, or while the session is locked.
    pub(crate) fn shell_focus_menu_bar(&mut self) -> Result<(), String> {
        if self.is_locked() {
            return Err("the session is locked".into());
        }
        let output = self
            .neue_shell
            .active
            .as_ref()
            .and_then(|w| self.space.outputs_for_element(w).into_iter().next())
            .or_else(|| {
                let pos = self.pointer.current_location();
                self.space.output_under(pos).next().cloned()
            });
        let serial = SERIAL_COUNTER.next_serial();
        let mut sent = false;
        for manager in &self.neue_shell.managers {
            if manager.resource.version() < 2 {
                continue;
            }
            let wl_output = output.as_ref().and_then(|o| {
                let client = manager.resource.client()?;
                o.client_outputs(&client).next()
            });
            manager
                .resource
                .focus_menu_bar(serial.into(), wl_output.as_ref());
            sent = true;
        }
        if sent {
            Ok(())
        } else {
            Err("no shell with a menu bar is running".into())
        }
    }

    /// `send_shortcut`: type `modifiers` + `keysym` into the active window.
    fn shell_send_shortcut(&mut self, modifiers: Modifiers, keysym: u32) {
        if self.is_locked() {
            return;
        }
        let Some(window) = self.neue_shell.active.clone() else {
            debug!("send_shortcut with no active window");
            return;
        };
        let Some(surface) = window.toplevel().map(|t| t.wl_surface().clone()) else {
            return;
        };
        let keyboard = self.keyboard.clone();
        // A shell menu's popup grab still has the keyboard if the shell sends the chord
        // in the update that closed the menu (the popup is destroyed after it): end the
        // grab, or focus can't move and the chord would go to the popup.
        if keyboard.is_grabbed() {
            keyboard.unset_grab(self);
        }
        if self.keyboard_focus().as_ref() != Some(&surface) {
            self.focus_window(&window);
        }
        let mut keys = Vec::new();
        for (flag, sym) in [
            (Modifiers::Shift, Keysym::Shift_L),
            (Modifiers::Ctrl, Keysym::Control_L),
            (Modifiers::Alt, Keysym::Alt_L),
            (Modifiers::Logo, Keysym::Super_L),
        ] {
            if modifiers.contains(flag) {
                let Some(code) = keyboard.keycode_for_keysym(sym) else {
                    warn!(
                        ?sym,
                        "send_shortcut: the layout has no key for this modifier"
                    );
                    return;
                };
                keys.push(code);
            }
        }
        let Some(code) = keyboard.keycode_for_keysym(Keysym::new(keysym)) else {
            debug!(
                keysym,
                "send_shortcut: the layout has no key for this keysym"
            );
            return;
        };
        keys.push(code);
        let presses = keys.iter().map(|&k| (k, KeyState::Pressed));
        let releases = keys.iter().rev().map(|&k| (k, KeyState::Released));
        for (code, key_state) in presses.chain(releases) {
            // Straight to the focused client: our own bindings must not see these.
            keyboard.input::<(), _>(
                self,
                code,
                key_state,
                SERIAL_COUNTER.next_serial(),
                InputTime::now(),
                |_, _, _| FilterResult::Forward,
            );
        }
    }

    /// A key was pressed: if it is a system key and a shell is bound, it goes to the shell.
    /// Returns whether it was taken.
    pub(crate) fn shell_system_key_press(
        &mut self,
        keycode: u32,
        sym: Keysym,
        modifiers: &ModifiersState,
    ) -> bool {
        if !self.neue_shell.is_bound() {
            return false;
        }
        let Some(key) = system_key(sym) else {
            return false;
        };
        self.neue_shell.held_keys.insert(keycode, key);
        self.send_system_key(key, neue_shell_manager_v1::KeyState::Pressed, modifiers);
        true
    }

    /// A key was released: if its press went to the shell, so does this. Returns whether
    /// it was taken.
    pub(crate) fn shell_system_key_release(
        &mut self,
        keycode: u32,
        modifiers: &ModifiersState,
    ) -> bool {
        let Some(key) = self.neue_shell.held_keys.remove(&keycode) else {
            return false;
        };
        self.send_system_key(key, neue_shell_manager_v1::KeyState::Released, modifiers);
        true
    }

    fn send_system_key(
        &self,
        key: SystemKey,
        key_state: neue_shell_manager_v1::KeyState,
        modifiers: &ModifiersState,
    ) {
        let mut mods = Modifiers::empty();
        mods.set(Modifiers::Shift, modifiers.shift);
        mods.set(Modifiers::Ctrl, modifiers.ctrl);
        mods.set(Modifiers::Alt, modifiers.alt);
        mods.set(Modifiers::Logo, modifiers.logo);
        for manager in &self.neue_shell.managers {
            manager.resource.system_key(key, key_state, mods);
        }
    }
}

/// The system key `sym` is, if any.
fn system_key(sym: Keysym) -> Option<SystemKey> {
    Some(match sym {
        Keysym::XF86_AudioRaiseVolume => SystemKey::VolumeUp,
        Keysym::XF86_AudioLowerVolume => SystemKey::VolumeDown,
        Keysym::XF86_AudioMute => SystemKey::Mute,
        Keysym::XF86_AudioMicMute => SystemKey::MicMute,
        Keysym::XF86_MonBrightnessUp => SystemKey::BrightnessUp,
        Keysym::XF86_MonBrightnessDown => SystemKey::BrightnessDown,
        Keysym::XF86_KbdBrightnessUp => SystemKey::KeyboardBrightnessUp,
        Keysym::XF86_KbdBrightnessDown => SystemKey::KeyboardBrightnessDown,
        Keysym::XF86_AudioPlay => SystemKey::Play,
        Keysym::XF86_AudioPause => SystemKey::Pause,
        Keysym::XF86_AudioNext => SystemKey::Next,
        Keysym::XF86_AudioPrev => SystemKey::Previous,
        Keysym::XF86_AudioStop => SystemKey::Stop,
        Keysym::XF86_Eject => SystemKey::Eject,
        _ => return None,
    })
}

/// Send the state events where `new` differs from `old` (everything when `old` is `None`).
/// The caller sends `done`.
fn send_changes(toplevel: &NeueShellToplevelV1, old: Option<&Snapshot>, new: &Snapshot) {
    if old.is_none_or(|o| o.title != new.title) {
        toplevel.title(new.title.clone());
    }
    if old.is_none_or(|o| o.app_id != new.app_id) {
        toplevel.app_id(new.app_id.clone());
    }
    if old.is_none_or(|o| o.state != new.state) {
        toplevel.state(new.state);
    }
    if old.is_none_or(|o| o.icon != new.icon) {
        toplevel.icon(new.icon.name.clone());
        for image in &new.icon.images {
            toplevel.icon_buffer(image.fd.as_fd(), image.width, image.height, image.scale);
        }
    }
    if toplevel.version() >= 3 && old.is_none_or(|o| o.super_as_ctrl != new.super_as_ctrl) {
        toplevel.command_key(if new.super_as_ctrl {
            neue_shell_toplevel_v1::CommandKey::Ctrl
        } else {
            neue_shell_toplevel_v1::CommandKey::Super
        });
    }
    if old.is_none_or(|o| o.menu != new.menu) {
        match &new.menu {
            Some(MenuAddress::DbusMenu { service, path }) => {
                toplevel.menu_dbusmenu(service.clone(), path.clone());
            }
            Some(MenuAddress::Gmenu {
                bus_name,
                menubar_path,
                app_menu_path,
                application_path,
                window_path,
            }) => toplevel.menu_gmenu(
                bus_name.clone(),
                menubar_path.clone(),
                app_menu_path.clone(),
                application_path.clone(),
                window_path.clone(),
            ),
            None => toplevel.menu_none(),
        }
    }
}

/// The current icon of toplevel `surface`; its images are copied unless `previous` is
/// the same icon.
fn read_icon(surface: &WlSurface, previous: Option<&Icon>) -> Icon {
    let (name, buffers) = with_states(surface, |states| {
        let mut cached = states.cached_state.get::<ToplevelIconCachedState>();
        let current = cached.current();
        (
            current.icon_name().map(ToOwned::to_owned),
            current.buffers().to_vec(),
        )
    });
    let mut icon = Icon {
        name,
        buffers: buffers.iter().map(|(b, s)| (b.id(), *s)).collect(),
        images: Vec::new(),
    };
    if let Some(previous) = previous
        && previous == &icon
    {
        icon.images = previous.images.clone();
        return icon;
    }
    for (buffer, scale) in &buffers {
        match with_buffer_contents(buffer, copy_icon) {
            Ok(Ok((pixels, width, height))) => match sealed_memfd(&pixels) {
                Ok(fd) => icon.images.push(Arc::new(IconImage {
                    fd,
                    width,
                    height,
                    scale: *scale,
                })),
                Err(err) => warn!(%err, "passing an icon image to the shell failed"),
            },
            Ok(Err(reason)) => debug!(reason, "icon image skipped"),
            Err(err) => debug!(?err, "icon buffer unreadable"),
        }
    }
    icon
}

/// Copy an ARGB8888 or XRGB8888 icon out of a client's shm pool into tightly packed
/// ARGB8888 rows.
fn copy_icon(
    ptr: *const u8,
    len: usize,
    data: BufferData,
) -> Result<(Vec<u8>, i32, i32), &'static str> {
    let opaque = match data.format {
        wl_shm::Format::Argb8888 => false,
        wl_shm::Format::Xrgb8888 => true,
        _ => return Err("not ARGB8888 or XRGB8888"),
    };
    if data.width <= 0 || data.height <= 0 {
        return Err("empty");
    }
    if data.width > MAX_ICON_SIZE || data.height > MAX_ICON_SIZE {
        return Err("too large");
    }
    let (width, height) = (data.width as usize, data.height as usize);
    let offset = usize::try_from(data.offset).map_err(|_| "negative offset")?;
    let stride = usize::try_from(data.stride).map_err(|_| "negative stride")?;
    let row = width * 4;
    let end = offset + (height - 1) * stride + row;
    if stride < row || end > len {
        return Err("does not fit its pool");
    }
    let mut pixels = vec![0_u8; row * height];
    for (y, dest) in pixels.chunks_exact_mut(row).enumerate() {
        // SAFETY: `offset + y * stride + row <= end <= len` (checked above), so the source
        // lies inside the pool mapping Smithay hands us, which stays mapped while this
        // callback runs; `dest` is our own buffer, so they cannot overlap. We copy through
        // a raw pointer and never make a reference into the client-shared memory
        // (Smithay's safety note on `with_buffer_contents`); a client truncating the pool
        // raises SIGBUS, which Smithay catches and reports as an error.
        #[allow(unsafe_code)] // see SAFETY
        unsafe {
            std::ptr::copy_nonoverlapping(ptr.add(offset + y * stride), dest.as_mut_ptr(), row);
        }
    }
    if opaque {
        // Little-endian ARGB: alpha is the fourth byte.
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 0xff;
        }
    }
    Ok((pixels, data.width, data.height))
}

/// A memfd holding `bytes`, sealed so neither we nor the shell can change it.
fn sealed_memfd(bytes: &[u8]) -> std::io::Result<OwnedFd> {
    use rustix::fs::{MemfdFlags, SealFlags};
    let fd = rustix::fs::memfd_create(
        "neue-shell-icon",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )?;
    let mut file = std::fs::File::from(fd);
    std::io::Write::write_all(&mut file, bytes)?;
    let fd = OwnedFd::from(file);
    rustix::fs::fcntl_add_seals(
        &fd,
        SealFlags::SEAL | SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE,
    )?;
    Ok(fd)
}
