//! Conversions from Wayland input (sctk, xkb, evdev) to Masonry's `ui-events` types.

use masonry_core::core::keyboard::{Code, Key, KeyState, Location, NamedKey};
use masonry_core::core::{
    KeyboardEvent, Modifiers, PointerButton, PointerId, PointerInfo, PointerType,
};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym, Modifiers as XkbModifiers};

/// The one pointer this driver reports (no touch or tablets yet).
pub(super) const MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};

/// Maps a Linux evdev button code (`BTN_*`) to a pointer button.
pub(super) fn button(code: u32) -> Option<PointerButton> {
    match code {
        0x110 => Some(PointerButton::Primary),
        0x111 => Some(PointerButton::Secondary),
        0x112 => Some(PointerButton::Auxiliary),
        0x113 => Some(PointerButton::X1),
        0x114 => Some(PointerButton::X2),
        _ => None,
    }
}

pub(super) fn modifiers(m: &XkbModifiers) -> Modifiers {
    let mut out = Modifiers::empty();
    out.set(Modifiers::SHIFT, m.shift);
    out.set(Modifiers::CONTROL, m.ctrl);
    out.set(Modifiers::ALT, m.alt);
    out.set(Modifiers::META, m.logo);
    out.set(Modifiers::CAPS_LOCK, m.caps_lock);
    out
}

/// Maps an xkb key event. Only the keys text editing needs are named; anything else that
/// produces text becomes `Key::Character`. Task 2.5 replaces this with a full table.
pub(super) fn key_event(event: &KeyEvent, state: KeyState, mods: Modifiers) -> KeyboardEvent {
    KeyboardEvent {
        state,
        key: key(event.keysym, event.utf8.as_deref()),
        code: Code::Unidentified,
        location: Location::Standard,
        modifiers: mods,
        repeat: false,
        is_composing: false,
    }
}

fn key(sym: Keysym, utf8: Option<&str>) -> Key {
    let named = match sym {
        Keysym::BackSpace => NamedKey::Backspace,
        Keysym::Return | Keysym::KP_Enter => NamedKey::Enter,
        Keysym::Tab | Keysym::ISO_Left_Tab => NamedKey::Tab,
        Keysym::Escape => NamedKey::Escape,
        Keysym::Left => NamedKey::ArrowLeft,
        Keysym::Right => NamedKey::ArrowRight,
        Keysym::Up => NamedKey::ArrowUp,
        Keysym::Down => NamedKey::ArrowDown,
        Keysym::Home => NamedKey::Home,
        Keysym::End => NamedKey::End,
        Keysym::Delete => NamedKey::Delete,
        Keysym::Insert => NamedKey::Insert,
        Keysym::Page_Up => NamedKey::PageUp,
        Keysym::Page_Down => NamedKey::PageDown,
        Keysym::Shift_L | Keysym::Shift_R => NamedKey::Shift,
        Keysym::Control_L | Keysym::Control_R => NamedKey::Control,
        Keysym::Alt_L | Keysym::Alt_R => NamedKey::Alt,
        Keysym::Super_L | Keysym::Super_R => NamedKey::Meta,
        Keysym::Caps_Lock => NamedKey::CapsLock,
        _ => {
            return match utf8 {
                Some(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
                    Key::Character(text.to_owned())
                }
                _ => Key::Named(NamedKey::Unidentified),
            };
        }
    };
    Key::Named(named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_keys_are_named() {
        assert_eq!(
            key(Keysym::BackSpace, Some("\u{8}")),
            Key::Named(NamedKey::Backspace)
        );
        assert_eq!(key(Keysym::Return, Some("\r")), Key::Named(NamedKey::Enter));
        assert_eq!(key(Keysym::Left, None), Key::Named(NamedKey::ArrowLeft));
    }

    #[test]
    fn text_keys_carry_their_text() {
        assert_eq!(key(Keysym::a, Some("a")), Key::Character("a".into()));
        assert_eq!(
            key(Keysym::udiaeresis, Some("ü")),
            Key::Character("ü".into())
        );
        assert_eq!(key(Keysym::F13, None), Key::Named(NamedKey::Unidentified));
    }

    #[test]
    fn evdev_buttons() {
        assert_eq!(button(0x110), Some(PointerButton::Primary));
        assert_eq!(button(0x111), Some(PointerButton::Secondary));
        assert_eq!(button(0x200), None);
    }
}
