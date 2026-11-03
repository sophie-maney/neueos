//! Input dispatch: pointer and keyboard events to clients, click to focus, and the
//! compositor's own key bindings.
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/input.rs`.

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent,
};
use smithay::input::keyboard::{FilterResult, Keysym};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent};
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER};
use tracing::{debug, info, warn};

use crate::state::State;

/// What a key press that the compositor takes for itself does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// Switch to virtual terminal N (Ctrl+Alt+FN; xkb turns these into `XF86Switch_VT_N`).
    /// Only the tty backend acts on it. Not configurable.
    SwitchVt(i32),
    /// Run the named action of `[keybindings]` (see `neuectl actions`).
    Run(String),
    /// The release of a key whose press was a binding: dropped, not forwarded.
    Swallow,
}

/// The VT switch for `keysym`, if it is one.
fn vt_switch(keysym: Keysym) -> Option<KeyAction> {
    keysym
        .raw()
        .checked_sub(Keysym::XF86_Switch_VT_1.raw())
        .filter(|offset| *offset < 12)
        // offset < 12, so the cast is lossless.
        .map(|offset| KeyAction::SwitchVt(offset as i32 + 1))
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
        let keycode = event.key_code().raw();
        let action = keyboard.input::<KeyAction, _>(
            self,
            event.key_code(),
            event.state(),
            serial,
            time,
            |state, modifiers, handle| {
                // Bindings fire on press and take the matching release with them.
                if !pressed {
                    return if state.config.swallowed.remove(&keycode) {
                        FilterResult::Intercept(KeyAction::Swallow)
                    } else {
                        FilterResult::Forward
                    };
                }
                let sym = handle.modified_sym();
                let action = vt_switch(sym).or_else(|| {
                    let name = state.config.bindings.lookup(modifiers, sym)?;
                    Some(KeyAction::Run(name.to_owned()))
                });
                match action {
                    Some(action) => {
                        state.config.swallowed.insert(keycode);
                        FilterResult::Intercept(action)
                    }
                    None => FilterResult::Forward,
                }
            },
        );
        if let Some(action) = action {
            self.run_key_action(action);
        }
    }

    fn run_key_action(&mut self, action: KeyAction) {
        match action {
            KeyAction::Swallow => {}
            KeyAction::Run(name) => {
                info!(action = name, "key binding");
                let params = neue_ipc::ActionParams {
                    name,
                    ..neue_ipc::ActionParams::default()
                };
                if let Err(err) = self.run_action(params) {
                    warn!(message = err.message, "key binding failed");
                }
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
    /// Backends call this for absolute and relative motion; tests use it to script input.
    pub fn move_pointer(&mut self, pos: Point<f64, Logical>, time: InputTime) {
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
        self.press_button(event.button_code(), event.state(), event.time());
    }

    /// Press or release `button` (a `linux/input-event-codes.h` code) at the pointer.
    /// Pressing on a window raises and focuses it. Tests use this to script input.
    pub fn press_button(&mut self, button: u32, state: ButtonState, time: InputTime) {
        let serial = SERIAL_COUNTER.next_serial();
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
                button,
                state,
                serial,
                time,
            },
        );
        pointer.frame(self);
    }

    fn on_pointer_axis<I: InputBackend>(&mut self, event: I::PointerAxisEvent) {
        let source = event.source();
        let mut frame = AxisFrame::new(event.time()).source(source);
        let pointer_config = &self.config.active.pointer;
        let factor = if pointer_config.natural_scroll {
            -pointer_config.scroll_factor
        } else {
            pointer_config.scroll_factor
        };
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let v120 = event.amount_v120(axis);
            let amount = event
                .amount(axis)
                .unwrap_or_else(|| v120.unwrap_or(0.0) * 15.0 / 120.0);
            if amount != 0.0 {
                frame = frame.value(axis, amount * factor);
                if let Some(discrete) = v120 {
                    // v120 values are small integers carried as f64.
                    frame = frame.v120(axis, (discrete * factor).round() as i32);
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
        assert_eq!(
            vt_switch(Keysym::XF86_Switch_VT_1),
            Some(KeyAction::SwitchVt(1))
        );
        assert_eq!(
            vt_switch(Keysym::XF86_Switch_VT_12),
            Some(KeyAction::SwitchVt(12))
        );
        assert_eq!(vt_switch(Keysym::F1), None);
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
}
