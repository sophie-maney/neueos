//! `compositor.toml`: loading, applying and hot reload (ADR 0004, ADR 0009).
//!
//! The active config lives in [`ConfigRuntime`] on the [`State`]. A reload loads every
//! layer, checks what only the compositor can check (key names, action names, the xkb
//! keymap) and then applies only what changed. Any failure leaves the previous config in
//! place and reports a `config_error` event. Directories are watched with inotify through a
//! calloop source; events are debounced for [`DEBOUNCE`], so editors that write a file in
//! several steps cause one reload.

use std::collections::{BTreeMap, HashSet};
use std::mem::MaybeUninit;
use std::os::fd::AsFd;
use std::path::PathBuf;
use std::time::Duration;

use neue_config::{CompositorConfig, ConfigError, ConfigFile, Layers, compositor::UNBOUND};
use neue_ipc::{ConfigErrorInfo, Event};
use rustix::fs::inotify::{self, CreateFlags, ReadFlags, WatchFlags};
use rustix::io::Errno;
use smithay::input::keyboard::{Keysym, ModifiersState, XkbConfig, xkb};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use tracing::{info, warn};

use crate::state::State;

/// Quiet time after the last file event before the config is re-read.
pub const DEBOUNCE: Duration = Duration::from_millis(100);

/// One key binding, resolved: the keysym and what to run.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Binding {
    logo: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
    /// Keysym with ASCII letters folded to lower case (see [`fold`]).
    keysym: u32,
    action: String,
}

/// The `[keybindings]` table, ready for matching key presses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bindings(Vec<Binding>);

impl Bindings {
    /// Resolve `table`. Fails on a key name xkb does not know or an action the compositor
    /// does not have. `"none"` entries are dropped.
    pub fn build(table: &BTreeMap<neue_config::Chord, String>) -> Result<Self, String> {
        let mut bindings = Vec::new();
        for (chord, action) in table {
            if action == UNBOUND {
                continue;
            }
            let keysym = xkb::keysym_from_name(&chord.key, xkb::KEYSYM_CASE_INSENSITIVE);
            if keysym.raw() == 0 {
                return Err(format!("`{chord}`: `{}` is not a key name", chord.key));
            }
            if !crate::ipc::is_action(action) {
                return Err(format!(
                    "`{chord}`: unknown action `{action}` (see `neuectl actions`)"
                ));
            }
            bindings.push(Binding {
                logo: chord.logo,
                ctrl: chord.ctrl,
                alt: chord.alt,
                shift: chord.shift,
                keysym: fold(keysym.raw()),
                action: action.clone(),
            });
        }
        Ok(Self(bindings))
    }

    /// The action bound to `keysym` with exactly `modifiers` held (Caps and Num Lock are
    /// ignored).
    pub fn lookup(&self, modifiers: &ModifiersState, keysym: Keysym) -> Option<&str> {
        let keysym = fold(keysym.raw());
        self.0
            .iter()
            .find(|b| {
                b.keysym == keysym
                    && (b.logo, b.ctrl, b.alt, b.shift)
                        == (
                            modifiers.logo,
                            modifiers.ctrl,
                            modifiers.alt,
                            modifiers.shift,
                        )
            })
            .map(|b| b.action.as_str())
    }
}

/// ASCII upper-case letters to lower case, so `Shift+Q` matches however the layout reports
/// the shifted key. Other keysyms are unchanged.
fn fold(raw: u32) -> u32 {
    if (0x41..=0x5a).contains(&raw) {
        raw + 0x20
    } else {
        raw
    }
}

/// The active config and what is needed to reload it.
pub struct ConfigRuntime {
    layers: Layers,
    handle: LoopHandle<'static, State>,
    /// The config that is applied.
    pub active: CompositorConfig,
    /// `active.keybindings`, resolved.
    pub bindings: Bindings,
    /// Keys whose press was taken by a binding; their release is taken too, so clients never
    /// see a release without a press.
    pub swallowed: HashSet<u32>,
    /// The pending debounce timer, if file events arrived recently.
    debounce: Option<RegistrationToken>,
}

impl std::fmt::Debug for ConfigRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigRuntime")
            .field("layers", &self.layers)
            .field("active", &self.active)
            .finish_non_exhaustive()
    }
}

impl ConfigRuntime {
    /// Built-in defaults, no files read yet.
    pub fn new(handle: LoopHandle<'static, State>) -> Self {
        let active = CompositorConfig::default();
        Self {
            layers: Layers::none(),
            handle,
            // The defaults only name known keys and actions.
            bindings: Bindings::build(&active.keybindings).unwrap_or_default(),
            active,
            swallowed: HashSet::new(),
            debounce: None,
        }
    }

    /// The file errors are blamed on when no parser position exists: the user's file.
    fn user_file(&self) -> PathBuf {
        let name = CompositorConfig::FILE_NAME;
        self.layers
            .user_dir()
            .map_or_else(|| PathBuf::from(name), |dir| dir.join(name))
    }
}

impl State {
    /// Read the config from `layers`, apply it, and watch the files for changes. Problems
    /// are logged and leave the defaults active; the compositor always starts.
    pub fn init_config(&mut self, layers: Layers) {
        self.config.layers = layers;
        // Errors are already logged and emitted by `reload_config`.
        let _ = self.reload_config();
        if let Err(err) = self.watch_config() {
            warn!(%err, "not watching the config files: changes need `neuectl reload-config`");
        }
    }

    /// Re-read the config files and apply what changed. On failure the previous config stays
    /// active, the error is logged and sent as `config_error`, and returned.
    pub fn reload_config(&mut self) -> Result<(), ConfigErrorInfo> {
        let file = CompositorConfig::FILE_NAME;
        let result =
            neue_config::load::<CompositorConfig>(&self.config.layers).and_then(|loaded| {
                for key in &loaded.unknown_keys {
                    warn!(file, key, "ignoring unknown config key");
                }
                self.apply_config(loaded.config)
                    .map_err(|message| ConfigError::in_file(self.config.user_file(), message))
            });
        match result {
            Ok(()) => {
                info!(file, "config loaded");
                self.emit(Event::ConfigReloaded {
                    file: file.to_owned(),
                });
                Ok(())
            }
            Err(err) => {
                warn!(%err, "config rejected, keeping the previous one");
                let info = ConfigErrorInfo {
                    file: err.file.display().to_string(),
                    line: err.line.and_then(|l| u32::try_from(l).ok()),
                    column: err.column.and_then(|c| u32::try_from(c).ok()),
                    message: err.message,
                };
                self.emit(Event::ConfigError(info.clone()));
                Err(info)
            }
        }
    }

    /// Make `new` the active config. Everything that can fail runs first, so a failure
    /// changes nothing.
    fn apply_config(&mut self, new: CompositorConfig) -> Result<(), String> {
        let bindings = Bindings::build(&new.keybindings)?;

        let old = &self.config.active;
        let outputs_changed = new.outputs != old.outputs;
        let touchpad_changed = new.touchpad != old.touchpad;
        if new.keyboard != old.keyboard {
            let keyboard = &new.keyboard;
            let layout = keyboard.xkb_layout();
            let variant = keyboard.xkb_variant();
            let options = keyboard.xkb_options();
            let handle = self.keyboard.clone();
            handle
                .set_xkb_config(
                    self,
                    XkbConfig {
                        rules: "",
                        model: &keyboard.model,
                        layout: &layout,
                        variant: &variant,
                        options: (!options.is_empty()).then_some(options),
                    },
                )
                .map_err(|_| {
                    format!(
                        "xkb cannot build a keymap from layouts `{layout}`, variants \
                         `{variant}`, model `{}` and the given options",
                        keyboard.model
                    )
                })?;
            // Both fit in i32: validated to at most 5000 and 200.
            self.keyboard
                .change_repeat_info(keyboard.repeat_rate as i32, keyboard.repeat_delay as i32);
        }

        self.config.bindings = bindings;
        self.config.active = new;
        #[cfg(feature = "tty")]
        if touchpad_changed && let crate::state::Backend::Tty(tty) = &mut self.backend {
            tty.configure_touchpads(&self.config.active.touchpad);
        }
        #[cfg(not(feature = "tty"))]
        let _ = touchpad_changed;
        if outputs_changed {
            self.apply_output_config();
        }
        Ok(())
    }

    /// Start watching the config directories (a calloop source). The user directory is
    /// created if missing, so a file added later is noticed.
    fn watch_config(&mut self) -> std::io::Result<()> {
        let layers = &self.config.layers;
        if layers.dirs().is_empty() {
            return Ok(());
        }
        if let Some(user) = layers.user_dir()
            && let Err(err) = std::fs::create_dir_all(user)
        {
            warn!(dir = %user.display(), %err, "cannot create the config directory");
        }
        let fd = inotify::init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC)?;
        let mut watched = 0;
        for dir in layers.dirs() {
            let flags = WatchFlags::CLOSE_WRITE
                | WatchFlags::MOVED_TO
                | WatchFlags::MOVED_FROM
                | WatchFlags::CREATE
                | WatchFlags::DELETE;
            match inotify::add_watch(&fd, dir, flags) {
                Ok(_) => watched += 1,
                // System layers may not exist; they are not worth a warning.
                Err(err) => tracing::debug!(dir = %dir.display(), %err, "not watching"),
            }
        }
        if watched == 0 {
            return Ok(());
        }
        let handle = self.config.handle.clone();
        handle
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                |_, fd, state| {
                    if drain_events(fd.as_fd()) {
                        state.schedule_config_reload();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| std::io::Error::other(e.error))?;
        Ok(())
    }

    /// Reload once [`DEBOUNCE`] has passed, unless a reload is already scheduled.
    fn schedule_config_reload(&mut self) {
        if self.config.debounce.is_some() {
            return;
        }
        let timer = Timer::from_duration(DEBOUNCE);
        let token = self.config.handle.insert_source(timer, |_, _, state| {
            state.config.debounce = None;
            // Errors are logged and emitted inside.
            let _ = state.reload_config();
            TimeoutAction::Drop
        });
        match token {
            Ok(token) => self.config.debounce = Some(token),
            Err(err) => warn!(err = %err.error, "cannot schedule a config reload"),
        }
    }
}

/// Read every pending inotify event; true if one concerns `compositor.toml` (or events
/// were lost).
fn drain_events(fd: std::os::fd::BorrowedFd<'_>) -> bool {
    let mut buffer = [MaybeUninit::<u8>::uninit(); 4096];
    let mut reader = inotify::Reader::new(fd, &mut buffer);
    let name = CompositorConfig::FILE_NAME.as_bytes();
    let mut relevant = false;
    loop {
        match reader.next() {
            Ok(event) => {
                relevant |= event.events().contains(ReadFlags::QUEUE_OVERFLOW)
                    || event.file_name().is_some_and(|n| n.to_bytes() == name);
            }
            Err(Errno::AGAIN) => return relevant,
            Err(err) => {
                warn!(%err, "reading config file events failed");
                return true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neue_config::Chord;

    fn table(entries: &[(&str, &str)]) -> BTreeMap<Chord, String> {
        entries
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), (*v).to_owned()))
            .collect()
    }

    fn mods(logo: bool, shift: bool) -> ModifiersState {
        ModifiersState {
            logo,
            shift,
            ..ModifiersState::default()
        }
    }

    #[test]
    fn default_bindings_quit_on_super_q() {
        let bindings = Bindings::build(&CompositorConfig::default().keybindings).unwrap();
        assert_eq!(bindings.lookup(&mods(true, false), Keysym::q), Some("quit"));
        assert_eq!(bindings.lookup(&mods(true, false), Keysym::Q), Some("quit"));
        assert_eq!(bindings.lookup(&mods(false, false), Keysym::q), None);
        assert_eq!(bindings.lookup(&mods(true, true), Keysym::Q), None);
    }

    #[test]
    fn shift_chords_match_the_shifted_keysym() {
        let bindings = Bindings::build(&table(&[("Super+Shift+W", "close-window")])).unwrap();
        assert_eq!(
            bindings.lookup(&mods(true, true), Keysym::W),
            Some("close-window")
        );
        assert_eq!(bindings.lookup(&mods(true, false), Keysym::w), None);
    }

    #[test]
    fn named_keys_resolve() {
        let bindings =
            Bindings::build(&table(&[("Super+Return", "quit"), ("F5", "quit")])).unwrap();
        assert_eq!(
            bindings.lookup(&mods(true, false), Keysym::Return),
            Some("quit")
        );
        assert_eq!(
            bindings.lookup(&mods(false, false), Keysym::F5),
            Some("quit")
        );
    }

    #[test]
    fn unbound_entries_are_dropped() {
        let bindings = Bindings::build(&table(&[("Super+Q", UNBOUND)])).unwrap();
        assert_eq!(bindings, Bindings::default());
    }

    #[test]
    fn unknown_keys_and_actions_are_errors() {
        let err = Bindings::build(&table(&[("Super+Nonsense", "quit")])).unwrap_err();
        assert!(err.contains("not a key name"), "{err}");
        let err = Bindings::build(&table(&[("Super+X", "no-such-action")])).unwrap_err();
        assert!(err.contains("unknown action"), "{err}");
    }
}
