//! Super as ⌘ (task 3.10a, ADR 0079, PLAN.md Q1): Super+key chords reach third-party apps
//! as Ctrl+key, so Super+C copies and Super+V pastes in Firefox as in neue-kit apps.
//!
//! The compositor's own state is left alone: bindings still see Super, and the keys go to
//! clients as pressed. Only the modifier state sent to a remapped client changes: its
//! `wl_keyboard.modifiers` events carry Control where the seat has Super. That happens in
//! [`KeyFocus`], the seat's keyboard focus type, which every `modifiers` event (key
//! presses, focus changes, popup grabs) goes through. The client still sees the Super key
//! itself pressed and released (only its modifier bits change).
//!
//! Who is remapped ([`State::super_as_ctrl`]): every client but
//! - neue-kit apps, which take Super themselves: clients that bound
//!   `neue_window_manager_v1` (every neue-kit app binds it when it starts) and the shell;
//! - apps whose id is in `[super-as-command] exceptions` (terminals by default);
//! - a surface that inhibits shortcuts (a VM or remote-desktop viewer wants the real key).

use std::borrow::Cow;
use std::sync::atomic::Ordering;

use smithay::desktop::{PopupKind, find_popup_root_surface};
use smithay::input::Seat;
use smithay::input::keyboard::{KeyboardTarget, KeysymHandle, ModifiersState, xkb};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Serial};
use smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat;
use smithay::wayland::seat::WaylandFocus;
use tracing::warn;

use crate::state::{ClientState, State};

/// The seat's keyboard focus: a surface, with the modifiers it is sent rewritten for
/// Super as ⌘ (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFocus(pub WlSurface);

impl From<WlSurface> for KeyFocus {
    fn from(surface: WlSurface) -> Self {
        Self(surface)
    }
}

/// Popup grabs give the pointer the surface the keyboard has.
impl From<KeyFocus> for WlSurface {
    fn from(focus: KeyFocus) -> Self {
        focus.0
    }
}

/// Popup keyboard grabs focus the popup's surface.
impl From<PopupKind> for KeyFocus {
    fn from(popup: PopupKind) -> Self {
        Self(popup.wl_surface().clone())
    }
}

impl IsAlive for KeyFocus {
    fn alive(&self) -> bool {
        self.0.alive()
    }
}

impl WaylandFocus for KeyFocus {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        Some(Cow::Borrowed(&self.0))
    }
}

impl KeyboardTarget<State> for KeyFocus {
    fn enter(
        &self,
        seat: &Seat<State>,
        state: &mut State,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        KeyboardTarget::enter(&self.0, seat, state, keys, serial);
    }

    fn leave(&self, seat: &Seat<State>, state: &mut State, serial: Serial) {
        KeyboardTarget::leave(&self.0, seat, state, serial);
    }

    fn key(
        &self,
        seat: &Seat<State>,
        state: &mut State,
        key: KeysymHandle<'_>,
        key_state: smithay::backend::input::KeyState,
        serial: Serial,
        time: smithay::backend::input::InputTime,
    ) {
        KeyboardTarget::key(&self.0, seat, state, key, key_state, serial, time);
    }

    /// Smithay holds the keyboard's lock here: nothing in this path may use
    /// `State::keyboard`.
    fn modifiers(
        &self,
        seat: &Seat<State>,
        state: &mut State,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        let modifiers = if state.super_as_ctrl(&self.0) {
            state.command_key_masks.super_to_ctrl(modifiers)
        } else {
            modifiers
        };
        KeyboardTarget::modifiers(&self.0, seat, state, modifiers, serial);
    }
}

/// Where Super and Control are in the serialized modifier masks of the current keymap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModMasks {
    /// Super: the real modifier `Mod4`, and the virtual `Super` where xkbcommon has one.
    pub logo: u32,
    pub ctrl: u32,
}

impl Default for ModMasks {
    /// xkbcommon's real modifiers always come first and in X11's order (Shift, Lock,
    /// Control, Mod1 … Mod5), so these hold for any keymap until the real one is read.
    fn default() -> Self {
        Self {
            logo: 1 << 6,
            ctrl: 1 << 2,
        }
    }
}

impl ModMasks {
    /// The masks of `keymap`; the defaults for a modifier it lacks.
    pub fn of(keymap: &xkb::Keymap) -> Self {
        let bit = |name: &str| match keymap.mod_get_index(name) {
            index if index < 32 => 1u32 << index,
            _ => 0,
        };
        let defaults = Self::default();
        let logo = bit(xkb::MOD_NAME_LOGO) | bit("Super");
        let ctrl = bit(xkb::MOD_NAME_CTRL);
        Self {
            logo: if logo == 0 { defaults.logo } else { logo },
            ctrl: if ctrl == 0 { defaults.ctrl } else { ctrl },
        }
    }

    /// `modifiers` with Super, wherever it is held (depressed, latched or locked), turned
    /// into Control.
    pub fn super_to_ctrl(self, mut modifiers: ModifiersState) -> ModifiersState {
        let mods = &mut modifiers.serialized;
        let mut any = false;
        for mask in [&mut mods.depressed, &mut mods.latched, &mut mods.locked] {
            if *mask & self.logo != 0 {
                *mask = (*mask & !self.logo) | self.ctrl;
                any = true;
            }
        }
        if any || modifiers.logo {
            modifiers.logo = false;
            modifiers.ctrl = true;
        }
        modifiers
    }
}

impl State {
    /// Whether Super+key reaches `surface`'s client as Ctrl+key (see the module docs).
    /// Runs under the keyboard's lock (from [`KeyFocus::modifiers`]): it must not touch
    /// `self.keyboard`.
    pub fn super_as_ctrl(&self, surface: &WlSurface) -> bool {
        let config = &self.config.active.super_as_command;
        if !config.enabled {
            return false;
        }
        if self
            .seat
            .keyboard_shortcuts_inhibitor_for_surface(surface)
            .is_some()
        {
            return false;
        }
        self.window_super_as_ctrl(surface)
    }

    /// [`Self::super_as_ctrl`] without the keyboard-shortcuts inhibitor, which comes and
    /// goes with focus: what the shell is told about a window (`command_key`, 3.10c).
    pub fn window_super_as_ctrl(&self, surface: &WlSurface) -> bool {
        let config = &self.config.active.super_as_command;
        if !config.enabled {
            return false;
        }
        let native = surface.client().is_none_or(|client| {
            client
                .get_data::<ClientState>()
                .is_some_and(|data| data.shell || data.native_keys.load(Ordering::Relaxed))
        });
        !native && config.remaps(&self.app_id_of(surface).unwrap_or_default())
    }

    /// The app id of the window `surface` belongs to: the toplevel itself, or the window a
    /// popup hangs from. `None` for layer and lock surfaces, and windows without one.
    fn app_id_of(&self, surface: &WlSurface) -> Option<String> {
        let root = match self.popups.find_popup(surface) {
            Some(popup) => find_popup_root_surface(&popup).ok()?,
            None => surface.clone(),
        };
        let window = self.window_for_surface(&root)?;
        crate::wm::title_and_app_id(&window).1
    }

    /// Read where Super and Control are in the keymap. Call after the keymap changes.
    pub fn refresh_command_key_masks(&mut self) {
        let keyboard = self.keyboard.clone();
        let text = keyboard.with_xkb_state(self, |ctx| {
            ctx.xkb().lock().ok().map(|xkb| xkb.keymap_as_string())
        });
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = text.and_then(|text| {
            xkb::Keymap::new_from_string(
                &context,
                text,
                xkb::KEYMAP_FORMAT_TEXT_V1,
                xkb::KEYMAP_COMPILE_NO_FLAGS,
            )
        });
        self.command_key_masks = match keymap {
            Some(keymap) => ModMasks::of(&keymap),
            None => {
                warn!("cannot read the keymap's modifiers: assuming Super is Mod4");
                ModMasks::default()
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::input::keyboard::SerializedMods;

    fn mods(depressed: u32, logo: bool, ctrl: bool) -> ModifiersState {
        ModifiersState {
            logo,
            ctrl,
            serialized: SerializedMods {
                depressed,
                ..SerializedMods::default()
            },
            ..ModifiersState::default()
        }
    }

    #[test]
    fn super_becomes_control_and_the_rest_stays() {
        let masks = ModMasks::default();
        let shift = 1;
        let out = masks.super_to_ctrl(mods(masks.logo | shift, true, false));
        assert_eq!(out.serialized.depressed, masks.ctrl | shift);
        assert!(out.ctrl && !out.logo);

        let plain = mods(shift, false, false);
        assert_eq!(masks.super_to_ctrl(plain), plain);

        let both = masks.super_to_ctrl(mods(masks.logo | masks.ctrl, true, true));
        assert_eq!(both.serialized.depressed, masks.ctrl);
    }

    #[test]
    fn masks_come_from_the_keymap() {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "us",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .unwrap();
        let masks = ModMasks::of(&keymap);
        assert_eq!(masks.ctrl, ModMasks::default().ctrl);
        assert_ne!(masks.logo & ModMasks::default().logo, 0);
    }
}
