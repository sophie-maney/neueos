//! `compositor.toml` and `appearance.toml`: loading, applying and hot reload (ADR 0004,
//! ADR 0009, ADR 0075).
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

use neue_config::{
    AppearanceConfig, CompositorConfig, ConfigError, ConfigFile, Layers, compositor::UNBOUND,
};
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
    /// The applied `appearance.toml`: title bars and the cursor follow it.
    pub appearance: AppearanceConfig,
    /// `active.keybindings`, resolved.
    pub bindings: Bindings,
    /// Keys whose press was taken by a binding; their release is taken too, so clients never
    /// see a release without a press.
    pub swallowed: HashSet<u32>,
    /// The pending debounce timer, if file events arrived recently.
    debounce: Option<RegistrationToken>,
    /// Which files the events since the last reload concerned.
    pending: Changed,
}

/// Which of the two watched files changed.
#[derive(Debug, Clone, Copy, Default)]
struct Changed {
    compositor: bool,
    appearance: bool,
}

impl Changed {
    const BOTH: Self = Self {
        compositor: true,
        appearance: true,
    };

    fn any(self) -> bool {
        self.compositor || self.appearance
    }
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
            appearance: AppearanceConfig::default(),
            swallowed: HashSet::new(),
            debounce: None,
            pending: Changed::default(),
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
        // Errors are already logged and emitted by the reloads.
        let _ = self.reload_appearance();
        let _ = self.reload_compositor();
        self.refresh_command_key_masks();
        if let Err(err) = self.watch_config() {
            warn!(%err, "not watching the config files: changes need `neuectl reload-config`");
        }
    }

    /// Re-read `appearance.toml` and `compositor.toml` and apply what changed. On failure
    /// the previous config stays active, the error is logged and sent as `config_error`,
    /// and (the first one) returned.
    pub fn reload_config(&mut self) -> Result<(), ConfigErrorInfo> {
        let appearance = self.reload_appearance();
        self.reload_compositor().and(appearance)
    }

    /// Re-read `appearance.toml` (every layer) and apply it if it changed. Says nothing
    /// when it didn't.
    fn reload_appearance(&mut self) -> Result<(), ConfigErrorInfo> {
        let file = AppearanceConfig::FILE_NAME;
        match neue_config::load::<AppearanceConfig>(&self.config.layers) {
            Ok(loaded) => {
                for key in &loaded.unknown_keys {
                    warn!(file, key, "ignoring unknown config key");
                }
                if loaded.config != self.config.appearance {
                    self.apply_appearance(loaded.config);
                    info!(file, "config loaded");
                    self.emit(Event::ConfigReloaded {
                        file: file.to_owned(),
                    });
                }
                Ok(())
            }
            Err(err) => Err(self.reject_config(err)),
        }
    }

    /// Make `new` the active appearance: title bars take its colours, the cursor its theme
    /// and size.
    fn apply_appearance(&mut self, new: AppearanceConfig) {
        self.chrome.variant = new.variant();
        #[cfg(feature = "tty")]
        if new.cursor != self.config.appearance.cursor
            && let crate::state::Backend::Tty(tty) = &mut self.backend
        {
            tty.configure_cursor(&new.cursor);
        }
        self.config.appearance = new;
        self.queue_redraw_all();
    }

    /// Log a rejected config and send it as `config_error`.
    fn reject_config(&mut self, err: ConfigError) -> ConfigErrorInfo {
        warn!(%err, "config rejected, keeping the previous one");
        let info = ConfigErrorInfo {
            file: err.file.display().to_string(),
            line: err.line.and_then(|l| u32::try_from(l).ok()),
            column: err.column.and_then(|c| u32::try_from(c).ok()),
            message: err.message,
        };
        self.emit(Event::ConfigError(info.clone()));
        info
    }

    /// Re-read `compositor.toml` and apply what changed.
    fn reload_compositor(&mut self) -> Result<(), ConfigErrorInfo> {
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
            Err(err) => Err(self.reject_config(err)),
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
            self.refresh_command_key_masks();
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
        self.queue_redraw_all();
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
                    let changed = drain_events(fd.as_fd());
                    if changed.any() {
                        state.schedule_config_reload(changed);
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| std::io::Error::other(e.error))?;
        Ok(())
    }

    /// Reload `changed` once [`DEBOUNCE`] has passed; a reload already scheduled takes it
    /// along.
    fn schedule_config_reload(&mut self, changed: Changed) {
        self.config.pending.compositor |= changed.compositor;
        self.config.pending.appearance |= changed.appearance;
        if self.config.debounce.is_some() {
            return;
        }
        let timer = Timer::from_duration(DEBOUNCE);
        let token = self.config.handle.insert_source(timer, |_, _, state| {
            state.config.debounce = None;
            let pending = std::mem::take(&mut state.config.pending);
            // Errors are logged and emitted inside.
            if pending.appearance {
                let _ = state.reload_appearance();
            }
            if pending.compositor {
                let _ = state.reload_compositor();
            }
            TimeoutAction::Drop
        });
        match token {
            Ok(token) => self.config.debounce = Some(token),
            Err(err) => warn!(err = %err.error, "cannot schedule a config reload"),
        }
    }
}

/// Read every pending inotify event; which of the two files they concern (both if events
/// were lost).
fn drain_events(fd: std::os::fd::BorrowedFd<'_>) -> Changed {
    let mut buffer = [MaybeUninit::<u8>::uninit(); 4096];
    let mut reader = inotify::Reader::new(fd, &mut buffer);
    let mut changed = Changed::default();
    loop {
        match reader.next() {
            Ok(event) => {
                if event.events().contains(ReadFlags::QUEUE_OVERFLOW) {
                    changed = Changed::BOTH;
                } else if let Some(name) = event.file_name() {
                    changed.compositor |= name.to_bytes() == CompositorConfig::FILE_NAME.as_bytes();
                    changed.appearance |= name.to_bytes() == AppearanceConfig::FILE_NAME.as_bytes();
                }
            }
            Err(Errno::AGAIN) => return changed,
            Err(err) => {
                warn!(%err, "reading config file events failed");
                return Changed::BOTH;
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
