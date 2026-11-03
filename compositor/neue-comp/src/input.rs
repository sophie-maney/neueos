//! Input dispatch: pointer and keyboard events to clients, click to focus, and the
//! compositor's own key bindings.
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/input.rs`.

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent,
};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER};
use tracing::{debug, info};

use crate::state::State;

/// Actions bound to keys in the compositor itself. Configurable bindings come with 1.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Quit the compositor (Super+Q).
    Quit,
    /// Switch to virtual terminal N (Ctrl+Alt+FN; xkb turns these into `XF86Switch_VT_N`).
    /// Only the tty backend acts on it.
    SwitchVt(i32),
}

/// The compositor binding for a key press, if any.
pub fn binding(modifiers: &ModifiersState, keysym: Keysym) -> Option<KeyAction> {
    let only_logo = modifiers.logo && !modifiers.ctrl && !modifiers.alt && !modifiers.shift;
    let vt = keysym
        .raw()
        .checked_sub(Keysym::XF86_Switch_VT_1.raw())
        .filter(|offset| *offset < 12);
    if let Some(offset) = vt {
        // offset < 12, so the cast is lossless.
        return Some(KeyAction::SwitchVt(offset as i32 + 1));
    }
    match keysym {
        Keysym::q | Keysym::Q if only_logo => Some(KeyAction::Quit),
        _ => None,
    }
}

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event } => self.on_key::<I>(event),
            InputEvent::PointerMotion { event } => self.on_pointer_motion::<I>(event),
            InputEvent::PointerMotionAbsolute { event } => {
                self.on_pointer_motion_absolute::<I>(event)
            }
            InputEvent::PointerButton { event } => self.on_pointer_button::<I>(event),
            InputEvent::PointerAxis { event } => self.on_pointer_axis::<I>(event),
            _ => {}
        }
    }

    fn on_key<I: InputBackend>(&mut self, event: I::KeyboardKeyEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        let time = event.time();
        let pressed = event.state() == KeyState::Pressed;
        let keyboard = self.keyboard.clone();
        let action = keyboard.input::<KeyAction, _>(
            self,
            event.key_code(),
            event.state(),
            serial,
            time,
            |_, modifiers, handle| {
                // Bindings fire on press. The release is forwarded; clients ignore releases of
                // keys they never saw pressed. (Suppressing releases comes with 1.6 bindings.)
                match binding(modifiers, handle.modified_sym()) {
                    Some(action) if pressed => FilterResult::Intercept(action),
                    _ => FilterResult::Forward,
                }
            },
        );
        if let Some(action) = action {
            self.run_key_action(action);
        }
    }

    fn run_key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Quit => {
                info!("quit requested (Super+Q)");
                self.loop_signal.stop();
            }
            KeyAction::SwitchVt(vt) => match &mut self.backend {
                #[cfg(feature = "tty")]
                crate::state::Backend::Tty(data) => data.switch_vt(vt),
                _ => debug!(vt, "VT switch ignored: not on a tty"),
            },
        }
    }

    /// Relative motion (mice and touchpads on the tty backend). The pointer stays on the
    /// output; with several outputs (task 1.7) this clamps to their union instead.
    fn on_pointer_motion<I: InputBackend>(&mut self, event: I::PointerMotionEvent) {
        let Some(output_geo) = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };
        let pos = clamp_to(self.pointer.current_location() + event.delta(), output_geo);
        self.move_pointer(pos, event.time());
    }

    fn on_pointer_motion_absolute<I: InputBackend>(
        &mut self,
        event: I::PointerMotionAbsoluteEvent,
    ) {
        let Some(output_geo) = self
            .space
            .outputs()
            .next()
            .and_then(|o| self.space.output_geometry(o))
        else {
            return;
        };
        let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
        self.move_pointer(pos, event.time());
    }

    /// Move the pointer to `pos` (global logical coordinates) and update its focus.
    fn move_pointer(&mut self, pos: Point<f64, Logical>, time: InputTime) {
        let under = self.surface_under(pos);
        let pointer = self.pointer.clone();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
    }

    fn on_pointer_button<I: InputBackend>(&mut self, event: I::PointerButtonEvent) {
        let serial = SERIAL_COUNTER.next_serial();
        let state = event.state();
        let pointer = self.pointer.clone();

        // Click to focus: a press on a window raises and focuses it; a press on the
        // background clears focus.
        if state == ButtonState::Pressed && !pointer.is_grabbed() {
            let clicked = self
                .space
                .element_under(pointer.current_location())
                .map(|(window, _)| window.clone());
            debug!(location = ?pointer.current_location(), hit = clicked.is_some(), "click");
            match clicked {
                Some(window) => self.focus_window(&window),
                None => self.clear_focus(),
            }
        }

        pointer.button(
            self,
            &ButtonEvent {
                button: event.button_code(),
                state,
                serial,
                time: event.time(),
            },
        );
        pointer.frame(self);
    }

    fn on_pointer_axis<I: InputBackend>(&mut self, event: I::PointerAxisEvent) {
        let source = event.source();
        let mut frame = AxisFrame::new(event.time()).source(source);
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let v120 = event.amount_v120(axis);
            let amount = event
                .amount(axis)
                .unwrap_or_else(|| v120.unwrap_or(0.0) * 15.0 / 120.0);
            if amount != 0.0 {
                frame = frame.value(axis, amount);
                if let Some(discrete) = v120 {
                    // v120 values are small integers carried as f64.
                    frame = frame.v120(axis, discrete as i32);
                }
            }
            if source == AxisSource::Finger && event.amount(axis) == Some(0.0) {
                frame = frame.stop(axis);
            }
        }
        let pointer = self.pointer.clone();
        pointer.axis(self, frame);
        pointer.frame(self);
    }
}

/// `pos` moved inside `rect` (the last pixel row and column included).
pub fn clamp_to(pos: Point<f64, Logical>, rect: Rectangle<i32, Logical>) -> Point<f64, Logical> {
    let min = rect.loc.to_f64();
    let max = (rect.loc + rect.size).to_f64() - Point::from((1.0, 1.0));
    Point::from((pos.x.clamp(min.x, max.x), pos.y.clamp(min.y, max.y)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_alt_f_keys_switch_vt() {
        let none = ModifiersState::default();
        assert_eq!(
            binding(&none, Keysym::XF86_Switch_VT_1),
            Some(KeyAction::SwitchVt(1))
        );
        assert_eq!(
            binding(&none, Keysym::XF86_Switch_VT_12),
            Some(KeyAction::SwitchVt(12))
        );
        assert_eq!(binding(&none, Keysym::F1), None);
    }

    #[test]
    fn pointer_is_clamped_to_the_output() {
        let output = Rectangle::new((0, 0).into(), (1920, 1080).into());
        assert_eq!(
            clamp_to(Point::from((-5.0, 2000.0)), output),
            Point::from((0.0, 1079.0))
        );
        assert_eq!(
            clamp_to(Point::from((10.5, 20.25)), output),
            Point::from((10.5, 20.25))
        );
    }

    fn mods(logo: bool, shift: bool) -> ModifiersState {
        ModifiersState {
            logo,
            shift,
            ..ModifiersState::default()
        }
    }

    #[test]
    fn super_q_quits() {
        assert_eq!(
            binding(&mods(true, false), Keysym::q),
            Some(KeyAction::Quit)
        );
    }

    #[test]
    fn plain_q_is_forwarded() {
        assert_eq!(binding(&mods(false, false), Keysym::q), None);
    }

    #[test]
    fn super_shift_q_is_not_quit() {
        assert_eq!(binding(&mods(true, true), Keysym::Q), None);
    }
}
