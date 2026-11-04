//! Conversions from Wayland input (sctk, xkb, evdev) to Masonry's `ui-events` types.

use masonry_core::core::keyboard::{Key, KeyState, NamedKey};
use masonry_core::core::{
    KeyboardEvent, Modifiers, PointerButton, PointerId, PointerInfo, PointerType, ScrollDelta,
};
use masonry_core::dpi::PhysicalPosition;
use masonry_core::kurbo::Point;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym, Modifiers as XkbModifiers};
use smithay_client_toolkit::seat::pointer::AxisScroll;

use super::keymap;

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
    out.set(Modifiers::NUM_LOCK, m.num_lock);
    out
}

/// Maps an xkb key event: the physical key from the evdev scancode, the logical key from
/// the keysym (named keys) or the text it produces.
pub(super) fn key_event(
    event: &KeyEvent,
    state: KeyState,
    mods: Modifiers,
    repeat: bool,
) -> KeyboardEvent {
    KeyboardEvent {
        state,
        key: key(event.keysym, event.utf8.as_deref()),
        code: keymap::code(event.raw_code),
        location: keymap::location(event.keysym),
        modifiers: mods,
        repeat,
        is_composing: false,
    }
}

fn key(sym: Keysym, utf8: Option<&str>) -> Key {
    if let Some(named) = keymap::named_key(sym) {
        return Key::Named(named);
    }
    match utf8 {
        Some(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
            Key::Character(text.to_owned())
        }
        // Ctrl turns letters into control characters, and releases carry no text: use
        // the keysym's own character, so shortcuts see "a" for Ctrl-A.
        _ => match sym.key_char() {
            Some(c) if !c.is_control() => Key::Character(c.to_string()),
            _ => Key::Named(NamedKey::Unidentified),
        },
    }
}

/// Scroll from one `wl_pointer` frame: pixels from touchpads, lines from wheels.
pub(super) fn scroll_delta(
    horizontal: &AxisScroll,
    vertical: &AxisScroll,
    scale: f64,
) -> ScrollDelta {
    let lines = |a: &AxisScroll| {
        if a.value120 != 0 {
            f64::from(a.value120) / 120.0
        } else {
            f64::from(a.discrete)
        }
    };
    let wheel = |a: &AxisScroll| a.value120 != 0 || a.discrete != 0;
    if wheel(horizontal) || wheel(vertical) {
        ScrollDelta::LineDelta(-lines(horizontal) as f32, -lines(vertical) as f32)
    } else {
        ScrollDelta::PixelDelta(PhysicalPosition::new(
            -horizontal.absolute * scale,
            -vertical.absolute * scale,
        ))
    }
}

/// Counts consecutive presses of one button close in time and place (double and triple
/// clicks). Wayland leaves this to clients.
#[derive(Debug, Default)]
pub(super) struct ClickCounter {
    last: Option<(PointerButton, Point, u32)>,
    count: u8,
}

impl ClickCounter {
    /// Longest gap between the presses of a multiple click, in milliseconds.
    const INTERVAL_MS: u32 = 500;
    /// How far the pointer may move between them, in logical pixels.
    const SLOP: f64 = 4.0;

    pub(super) fn press(&mut self, button: PointerButton, pos: Point, time: u32) {
        let repeated = self.last.is_some_and(|(b, p, t)| {
            b == button
                && time.wrapping_sub(t) <= Self::INTERVAL_MS
                && (pos - p).hypot() <= Self::SLOP
        });
        self.count = if repeated {
            self.count.saturating_add(1)
        } else {
            1
        };
        self.last = Some((button, pos, time));
    }

    /// The click count of the latest press (1 before any).
    pub(super) fn count(&self) -> u8 {
        self.count.max(1)
    }
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
        assert_eq!(key(Keysym::F13, None), Key::Named(NamedKey::F13));
        assert_eq!(
            key(Keysym::NoSymbol, None),
            Key::Named(NamedKey::Unidentified)
        );
    }

    #[test]
    fn control_letters_keep_their_letter() {
        // Ctrl-A: xkb's text is U+0001.
        assert_eq!(key(Keysym::a, Some("\u{1}")), Key::Character("a".into()));
        // Releases carry no text.
        assert_eq!(key(Keysym::A, None), Key::Character("A".into()));
    }

    #[test]
    fn physical_keys_and_locations() {
        use masonry_core::core::keyboard::{Code, Location};
        // evdev: KEY_A = 30, KEY_ENTER = 28, KEY_KP1 = 79, KEY_LEFTSHIFT = 42.
        assert_eq!(keymap::code(30), Code::KeyA);
        assert_eq!(keymap::code(28), Code::Enter);
        assert_eq!(keymap::code(79), Code::Numpad1);
        assert_eq!(keymap::code(0), Code::Unidentified);
        assert_eq!(keymap::location(Keysym::Shift_L), Location::Left);
        assert_eq!(keymap::location(Keysym::Control_R), Location::Right);
        assert_eq!(keymap::location(Keysym::KP_1), Location::Numpad);
        assert_eq!(keymap::location(Keysym::a), Location::Standard);
    }

    #[test]
    fn clicks_count_up_when_close_in_time_and_place() {
        let mut clicks = ClickCounter::default();
        let at = Point::new(10.0, 10.0);
        assert_eq!(clicks.count(), 1);
        clicks.press(PointerButton::Primary, at, 1000);
        assert_eq!(clicks.count(), 1);
        clicks.press(PointerButton::Primary, at + (2.0, 1.0), 1300);
        assert_eq!(clicks.count(), 2);
        clicks.press(PointerButton::Primary, at, 1600);
        assert_eq!(clicks.count(), 3);
        // Too late.
        clicks.press(PointerButton::Primary, at, 2200);
        assert_eq!(clicks.count(), 1);
        // Another button.
        clicks.press(PointerButton::Secondary, at, 2300);
        assert_eq!(clicks.count(), 1);
        // Too far.
        clicks.press(PointerButton::Secondary, at + (10.0, 0.0), 2400);
        assert_eq!(clicks.count(), 1);
    }

    #[test]
    fn wheels_scroll_lines_and_touchpads_pixels() {
        let axis = |absolute, discrete, value120| AxisScroll {
            absolute,
            discrete,
            value120,
            ..AxisScroll::default()
        };
        let none = AxisScroll::default();
        assert_eq!(
            scroll_delta(&none, &axis(15.0, 1, 120), 2.0),
            ScrollDelta::LineDelta(0.0, -1.0)
        );
        assert_eq!(
            scroll_delta(&none, &axis(7.5, 0, 60), 2.0),
            ScrollDelta::LineDelta(0.0, -0.5)
        );
        assert_eq!(
            scroll_delta(&axis(3.0, 0, 0), &none, 2.0),
            ScrollDelta::PixelDelta(PhysicalPosition::new(-6.0, -0.0))
        );
    }

    #[test]
    fn evdev_buttons() {
        assert_eq!(button(0x110), Some(PointerButton::Primary));
        assert_eq!(button(0x111), Some(PointerButton::Secondary));
        assert_eq!(button(0x200), None);
    }
}
