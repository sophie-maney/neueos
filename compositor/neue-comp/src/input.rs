//! Input dispatch: pointer and keyboard events to clients, click to focus, and the
//! compositor's own key bindings.
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/input.rs`.

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
};
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::utils::SERIAL_COUNTER;
use tracing::{debug, info};

use crate::state::State;

/// Actions bound to keys in the compositor itself. Configurable bindings come with 1.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Quit the compositor (Super+Q).
    Quit,
}

/// The compositor binding for a key press, if any.
pub fn binding(modifiers: &ModifiersState, keysym: Keysym) -> Option<KeyAction> {
    let only_logo = modifiers.logo && !modifiers.ctrl && !modifiers.alt && !modifiers.shift;
    match keysym {
        Keysym::q | Keysym::Q if only_logo => Some(KeyAction::Quit),
        _ => None,
    }
}

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event } => self.on_key::<I>(event),
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
        }
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
        let under = self.surface_under(pos);
        let pointer = self.pointer.clone();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: pos,
                serial: SERIAL_COUNTER.next_serial(),
                time: event.time(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
