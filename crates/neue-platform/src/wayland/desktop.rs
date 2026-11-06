//! Outputs and `neue_shell_v1` for the shell (task 3.2a, ADR 0067).
//!
//! Outputs come from sctk's `OutputState`; each gets an [`OutputId`] for its lifetime and
//! is reported to the app once its description is complete. `neue_shell_manager_v1` is
//! bound when the compositor offers it, which it does only to the shell it started
//! (ADR 0066); its window list becomes [`ShellEvent`]s and [`ShellRequest`]s become
//! requests.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use neue_protocols::neue_shell::client::{
    neue_shell_action_v1::{self, NeueShellActionV1},
    neue_shell_manager_v1::{self, NeueShellManagerV1},
    neue_shell_toplevel_v1::{self, NeueShellToplevelV1},
};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::RegistrationToken;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use wayland_client::backend::{ObjectData, ObjectId};
use wayland_client::protocol::wl_output;
use wayland_client::{Connection, Proxy, QueueHandle, WEnum};

use super::{AppEvent, State};
use crate::desktop::{
    ChordModifiers, OutputEvent, OutputId, OutputInfo, ShellEvent, ShellRequest, SystemKey,
    SystemKeyState, Toplevel, ToplevelId, ToplevelMenu,
};

/// The driver's view of outputs and of the shell protocol.
#[derive(Default)]
pub(super) struct Desktop {
    /// Outputs the app has been told about.
    outputs: Vec<(wl_output::WlOutput, OutputId)>,
    next_output: u64,
    /// The bound shell manager, if the compositor offers it.
    manager: Option<NeueShellManagerV1>,
    toplevels: HashMap<ObjectId, ShellToplevel>,
    next_toplevel: u64,
    /// The stepping system key being held, and the timer that repeats it.
    repeating: Option<(SystemKey, RegistrationToken)>,
    /// The keyboard's repeat delay and interval (`wl_keyboard.repeat_info`); `None` when
    /// the compositor turned repeat off.
    pub(super) repeat: Option<(Duration, Duration)>,
}

/// Until `wl_keyboard.repeat_info` says otherwise: 600 ms, then 25 a second (xkb's and
/// neue-comp's defaults).
const DEFAULT_REPEAT: (Duration, Duration) =
    (Duration::from_millis(600), Duration::from_millis(40));

/// One window of the shell's window list.
struct ShellToplevel {
    id: ToplevelId,
    proxy: NeueShellToplevelV1,
    /// State applied by the last `done`, and changes since.
    current: Toplevel,
    pending: Toplevel,
    /// Whether the app has been told about it (its first `done` arrived).
    announced: bool,
}

impl Desktop {
    /// Binds `neue_shell_manager_v1` if `globals` has it.
    pub(super) fn bind(
        globals: &wayland_client::globals::GlobalList,
        qh: &QueueHandle<State>,
    ) -> Self {
        // Version 2: `focus_menu_bar` (task 3.7b); version 3: `command_key` (task 3.10c).
        let manager = globals.bind(qh, 1..=3, ShellManagerData).ok();
        if manager.is_some() {
            tracing::info!("bound neue_shell_manager_v1: this client is the shell");
        }
        Self {
            manager,
            repeat: Some(DEFAULT_REPEAT),
            ..Self::default()
        }
    }

    pub(super) fn has_shell(&self) -> bool {
        self.manager.is_some()
    }

    /// The `wl_output` of `id`, if it's still connected.
    pub(super) fn wl_output(&self, id: OutputId) -> Option<&wl_output::WlOutput> {
        self.outputs
            .iter()
            .find_map(|(output, oid)| (*oid == id).then_some(output))
    }

    fn proxy(&self, id: ToplevelId) -> Option<&NeueShellToplevelV1> {
        self.toplevels
            .values()
            .find_map(|t| (t.id == id).then_some(&t.proxy))
    }

    /// Sends `request`; requests naming a window that has closed are dropped.
    pub(super) fn send(&self, request: ShellRequest, qh: &QueueHandle<State>) {
        let Some(manager) = &self.manager else {
            return;
        };
        let window = |id| {
            let proxy = self.proxy(id);
            if proxy.is_none() {
                tracing::debug!(?id, "shell request for a window that has closed");
            }
            proxy
        };
        match request {
            ShellRequest::Activate(id) => window(id).map(NeueShellToplevelV1::activate),
            ShellRequest::Minimize(id) => window(id).map(NeueShellToplevelV1::minimize),
            ShellRequest::Unminimize(id) => window(id).map(NeueShellToplevelV1::unminimize),
            ShellRequest::Close(id) => window(id).map(NeueShellToplevelV1::close),
            ShellRequest::RunAction { name, window: id } => {
                let target = match id {
                    Some(id) => match window(id) {
                        Some(proxy) => Some(proxy),
                        None => return,
                    },
                    None => None,
                };
                manager.run_action(name.clone(), target, qh, ActionData { name });
                Some(())
            }
            ShellRequest::SendShortcut { modifiers, keysym } => {
                manager.send_shortcut(protocol_modifiers(modifiers), keysym);
                Some(())
            }
        };
    }
}

/// Reports a system key and starts or stops its repeat.
fn system_key_event(state: &mut State, key: SystemKey, pressed: bool, modifiers: ChordModifiers) {
    // A new press, or the held key's release, ends the repeat. Another key's release
    // doesn't (one key repeats at a time, as on a keyboard).
    let ends = pressed || state.desktop.repeating.is_some_and(|(held, _)| held == key);
    if ends && let Some((_, token)) = state.desktop.repeating.take() {
        state.loop_handle.remove(token);
    }
    let event = move |state: SystemKeyState| {
        AppEvent::Shell(ShellEvent::SystemKey {
            key,
            state,
            modifiers,
        })
    };
    state.events.push(event(if pressed {
        SystemKeyState::Pressed
    } else {
        SystemKeyState::Released
    }));
    let Some((delay, interval)) = state.desktop.repeat.filter(|_| pressed && key.repeats()) else {
        return;
    };
    let timer =
        state
            .loop_handle
            .insert_source(Timer::from_duration(delay), move |_, (), state| {
                state.events.push(event(SystemKeyState::Repeated));
                TimeoutAction::ToDuration(interval)
            });
    match timer {
        Ok(token) => state.desktop.repeating = Some((key, token)),
        Err(err) => tracing::warn!(%err, "can't repeat the system key"),
    }
}

fn system_key(key: neue_shell_manager_v1::SystemKey) -> Option<SystemKey> {
    use neue_shell_manager_v1::SystemKey as K;
    Some(match key {
        K::VolumeUp => SystemKey::VolumeUp,
        K::VolumeDown => SystemKey::VolumeDown,
        K::Mute => SystemKey::Mute,
        K::MicMute => SystemKey::MicMute,
        K::BrightnessUp => SystemKey::BrightnessUp,
        K::BrightnessDown => SystemKey::BrightnessDown,
        K::KeyboardBrightnessUp => SystemKey::KeyboardBrightnessUp,
        K::KeyboardBrightnessDown => SystemKey::KeyboardBrightnessDown,
        K::Play => SystemKey::Play,
        K::Pause => SystemKey::Pause,
        K::Next => SystemKey::Next,
        K::Previous => SystemKey::Previous,
        K::Stop => SystemKey::Stop,
        K::Eject => SystemKey::Eject,
        _ => return None,
    })
}

fn chord_modifiers(m: neue_shell_manager_v1::Modifiers) -> ChordModifiers {
    use neue_shell_manager_v1::Modifiers as M;
    ChordModifiers {
        shift: m.contains(M::Shift),
        ctrl: m.contains(M::Ctrl),
        alt: m.contains(M::Alt),
        logo: m.contains(M::Logo),
    }
}

fn protocol_modifiers(m: ChordModifiers) -> neue_shell_manager_v1::Modifiers {
    use neue_shell_manager_v1::Modifiers as M;
    let mut out = M::empty();
    out.set(M::Shift, m.shift);
    out.set(M::Ctrl, m.ctrl);
    out.set(M::Alt, m.alt);
    out.set(M::Logo, m.logo);
    out
}

/// Builds the app-facing description of `output` from sctk's, or `None` while the
/// compositor hasn't described it.
fn output_info(
    state: &OutputState,
    output: &wl_output::WlOutput,
    id: OutputId,
) -> Option<OutputInfo> {
    let info = state.info(output)?;
    let scale = info.scale_factor.max(1);
    // xdg-output's logical size; without it the current mode divided by the scale.
    let (w, h) = info.logical_size.unwrap_or_else(|| {
        let mode = info
            .modes
            .iter()
            .find(|m| m.current)
            .map_or((0, 0), |m| m.dimensions);
        (mode.0 / scale, mode.1 / scale)
    });
    Some(OutputInfo {
        id,
        name: info.name.clone().unwrap_or_default(),
        description: info.description.clone().unwrap_or_default(),
        logical_size: (w.max(0).unsigned_abs(), h.max(0).unsigned_abs()),
        scale,
    })
}

impl State {
    /// Records the app-facing description of `output` and queues `event` for it.
    fn report_output(&mut self, output: &wl_output::WlOutput, added: bool) {
        let id = match self.desktop.outputs.iter().find(|(o, _)| o == output) {
            Some((_, id)) => *id,
            None => {
                let id = OutputId::from_raw(self.desktop.next_output);
                self.desktop.next_output += 1;
                self.desktop.outputs.push((output.clone(), id));
                id
            }
        };
        let Some(info) = output_info(&self.outputs, output, id) else {
            return;
        };
        let known = self.reg.outputs.iter().position(|o| o.id == id);
        match known {
            Some(i) => self.reg.outputs[i] = info.clone(),
            None => self.reg.outputs.push(info.clone()),
        }
        let event = if added && known.is_none() {
            OutputEvent::Added(info)
        } else {
            OutputEvent::Changed(info)
        };
        self.events.push(AppEvent::Output(event));
    }

    /// Sends the shell requests the app queued.
    pub(super) fn send_shell_requests(&mut self) {
        for request in std::mem::take(&mut self.reg.shell_requests) {
            self.desktop.send(request, &self.qh);
        }
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.report_output(&output, true);
    }

    fn update_output(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.report_output(&output, false);
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let Some(i) = self.desktop.outputs.iter().position(|(o, _)| *o == output) else {
            return;
        };
        let (_, id) = self.desktop.outputs.remove(i);
        self.reg.outputs.retain(|o| o.id != id);
        self.events.push(AppEvent::Output(OutputEvent::Removed(id)));
    }
}

/// User data for `neue_shell_manager_v1`.
struct ShellManagerData;

impl Dispatch2<NeueShellManagerV1, State> for ShellManagerData {
    fn event(
        &self,
        state: &mut State,
        _: &NeueShellManagerV1,
        event: neue_shell_manager_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use neue_shell_manager_v1::Event;
        match event {
            Event::Toplevel { toplevel } => {
                let id = ToplevelId::from_raw(state.desktop.next_toplevel);
                state.desktop.next_toplevel += 1;
                state.desktop.toplevels.insert(
                    toplevel.id(),
                    ShellToplevel {
                        id,
                        proxy: toplevel,
                        current: Toplevel::default(),
                        pending: Toplevel::default(),
                        announced: false,
                    },
                );
            }
            Event::Active { toplevel } => {
                let active = toplevel
                    .and_then(|t| state.desktop.toplevels.get(&t.id()))
                    .map(|t| t.id);
                state
                    .events
                    .push(AppEvent::Shell(ShellEvent::Active(active)));
            }
            Event::SystemKey {
                key,
                state: key_state,
                modifiers,
            } => {
                let (WEnum::Value(key), WEnum::Value(key_state)) = (key, key_state) else {
                    return;
                };
                let Some(key) = system_key(key) else {
                    return;
                };
                let modifiers = match modifiers {
                    WEnum::Value(m) => chord_modifiers(m),
                    WEnum::Unknown(_) => ChordModifiers::default(),
                };
                let pressed = key_state == neue_shell_manager_v1::KeyState::Pressed;
                system_key_event(state, key, pressed, modifiers);
            }
            Event::FocusMenuBar { serial, output } => {
                // The menu it opens grabs with this serial.
                state.input.last_serial = Some(serial);
                let output = output.and_then(|o| {
                    let outputs = &state.desktop.outputs;
                    outputs.iter().find(|(w, _)| *w == o).map(|&(_, id)| id)
                });
                state
                    .events
                    .push(AppEvent::Shell(ShellEvent::FocusMenuBar { output }));
            }
            _ => {}
        }
    }

    fn event_created_child(opcode: u16, qh: &QueueHandle<State>) -> Arc<dyn ObjectData> {
        assert_eq!(
            opcode,
            neue_shell_manager_v1::EVT_TOPLEVEL_OPCODE,
            "neue_shell_manager_v1 creates objects only in its toplevel event"
        );
        qh.make_data::<NeueShellToplevelV1, _>(ShellToplevelData)
    }
}

/// User data for `neue_shell_toplevel_v1`; its state lives in [`Desktop::toplevels`].
struct ShellToplevelData;

impl Dispatch2<NeueShellToplevelV1, State> for ShellToplevelData {
    fn event(
        &self,
        state: &mut State,
        proxy: &NeueShellToplevelV1,
        event: neue_shell_toplevel_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use neue_shell_toplevel_v1::{Event, State as Flags};
        let key = proxy.id();
        if let Event::Closed = event {
            if let Some(t) = state.desktop.toplevels.remove(&key) {
                t.proxy.destroy();
                if t.announced {
                    state.events.push(AppEvent::Shell(ShellEvent::Closed(t.id)));
                }
            }
            return;
        }
        let Some(t) = state.desktop.toplevels.get_mut(&key) else {
            return;
        };
        match event {
            Event::Identifier { identifier } => t.pending.identifier = identifier,
            Event::Title { title } => t.pending.title = title,
            Event::AppId { app_id } => t.pending.app_id = app_id,
            Event::State { state: flags } => {
                // An unknown bit (a later version's) is ignored.
                let flags = Flags::from_bits_truncate(u32::from(flags));
                t.pending.minimized = flags.contains(Flags::Minimized);
                t.pending.attention = flags.contains(Flags::Attention);
            }
            Event::Done => {
                let changed = !t.announced || t.pending != t.current;
                t.current = t.pending.clone();
                t.announced = true;
                if changed {
                    let event = ShellEvent::Toplevel(t.id, t.current.clone());
                    state.events.push(AppEvent::Shell(event));
                }
            }
            Event::MenuDbusmenu { service, path } => {
                t.pending.menu = ToplevelMenu::DbusMenu { service, path };
            }
            Event::MenuGmenu {
                bus_name,
                menubar_path,
                app_menu_path,
                application_path,
                window_path,
            } => {
                t.pending.menu = ToplevelMenu::GMenu {
                    bus_name,
                    menubar_path,
                    app_menu_path,
                    application_path,
                    window_path,
                };
            }
            Event::MenuNone => t.pending.menu = ToplevelMenu::None,
            Event::CommandKey { key } => {
                t.pending.super_as_ctrl =
                    key == WEnum::Value(neue_shell_toplevel_v1::CommandKey::Ctrl);
            }
            // Icons wait for the dock (3.16).
            _ => {}
        }
    }
}

/// User data for `neue_shell_action_v1`: the action's name, for the failure event.
struct ActionData {
    name: String,
}

impl Dispatch2<NeueShellActionV1, State> for ActionData {
    fn event(
        &self,
        state: &mut State,
        _: &NeueShellActionV1,
        event: neue_shell_action_v1::Event,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let neue_shell_action_v1::Event::Failed { message } = event {
            tracing::warn!(action = self.name, message, "compositor action failed");
            state.events.push(AppEvent::Shell(ShellEvent::ActionFailed {
                name: self.name.clone(),
                message,
            }));
        }
    }
}
