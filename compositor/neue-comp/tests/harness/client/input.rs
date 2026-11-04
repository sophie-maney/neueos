//! The input protocols on the test client (task 1.13): pointer constraints, relative
//! pointer, pointer gestures, text-input-v3 and, for a second client, input-method-v2.
//!
//! Objects are kept in [`InputData`]; tests read what the compositor sent through the
//! `*_events` methods.

use wayland_client::protocol::{wl_region::WlRegion, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols::wp::pointer_constraints::zv1::client::{
    zwp_confined_pointer_v1::{self, ZwpConfinedPointerV1},
    zwp_locked_pointer_v1::{self, ZwpLockedPointerV1},
    zwp_pointer_constraints_v1::{Lifetime, ZwpPointerConstraintsV1},
};
use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_hold_v1::{self, ZwpPointerGestureHoldV1},
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gesture_swipe_v1::{self, ZwpPointerGestureSwipeV1},
    zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    zwp_relative_pointer_v1::{self, ZwpRelativePointerV1},
};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{self, ZwpInputMethodV2},
    zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
};

use super::{ClientData, TestClient, Toplevel};

/// A pointer lock or confinement the client asked for.
pub struct Constraint {
    index: usize,
    locked: Option<ZwpLockedPointerV1>,
    confined: Option<ZwpConfinedPointerV1>,
}

impl Constraint {
    /// Destroy the constraint object (the client lets go of the pointer).
    pub fn destroy(&self) {
        if let Some(locked) = &self.locked {
            locked.destroy();
        }
        if let Some(confined) = &self.confined {
            confined.destroy();
        }
    }

    /// Tell the compositor where the client drew its own cursor (locks only); takes effect
    /// on the next commit of the surface.
    pub fn set_cursor_position_hint(&self, x: f64, y: f64) {
        if let Some(locked) = &self.locked {
            locked.set_cursor_position_hint(x, y);
        }
    }
}

/// What the compositor told one constraint.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ConstraintEvents {
    /// Active now (the last event was `locked`/`confined`).
    pub active: bool,
    /// How many times it became active.
    pub activations: u32,
    /// How many times it ended (`unlocked`/`unconfined`).
    pub deactivations: u32,
}

/// One relative motion: `[dx, dy, dx_unaccel, dy_unaccel]`.
pub type RelativeMotion = [f64; 4];

/// A gesture event as the client saw it.
#[derive(Debug, Clone, PartialEq)]
pub enum GestureSeen {
    SwipeBegin(u32),
    SwipeUpdate(f64, f64),
    SwipeEnd {
        cancelled: bool,
    },
    PinchBegin(u32),
    PinchUpdate {
        dx: f64,
        dy: f64,
        scale: f64,
        rotation: f64,
    },
    PinchEnd {
        cancelled: bool,
    },
    HoldBegin(u32),
    HoldEnd {
        cancelled: bool,
    },
}

/// What the compositor told a text input.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TextInputEvents {
    /// The surface has the text input (`enter` without a later `leave`).
    pub entered: bool,
    /// Strings the input method committed, in order.
    pub committed: Vec<String>,
    /// The last preedit string.
    pub preedit: Option<String>,
    /// How many `done` events arrived.
    pub done: u32,
}

/// What the compositor told an input method.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InputMethodEvents {
    pub active: bool,
    pub activations: u32,
    pub deactivations: u32,
    /// `done` events; `commit` must carry this number.
    pub done: u32,
    pub unavailable: bool,
}

#[derive(Default)]
pub(super) struct InputData {
    relative: Vec<RelativeMotion>,
    constraints: Vec<ConstraintEvents>,
    gestures: Vec<GestureSeen>,
    text: TextInputEvents,
    im: InputMethodEvents,
    /// Objects that must stay alive for their effect to last.
    relative_pointer: Option<ZwpRelativePointerV1>,
    gesture_objects: Vec<Box<dyn std::any::Any>>,
    text_input: Option<ZwpTextInputV3>,
    input_method: Option<ZwpInputMethodV2>,
    popups: Vec<ZwpInputPopupSurfaceV2>,
}

impl TestClient {
    // ---- pointer-constraints ----

    fn constraints_manager(&self) -> ZwpPointerConstraintsV1 {
        self._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zwp_pointer_constraints_v1")
    }

    fn region(&self, (x, y, w, h): (i32, i32, i32, i32)) -> WlRegion {
        let region = self.compositor.create_region(&self.qh, ());
        region.add(x, y, w, h);
        region
    }

    /// Lock the pointer to `toplevel`'s surface. `region` (surface coordinates) limits where
    /// the lock may start; a `oneshot` lock ends for good when it is deactivated.
    pub fn lock_pointer(
        &mut self,
        toplevel: &Toplevel,
        region: Option<(i32, i32, i32, i32)>,
        oneshot: bool,
    ) -> Constraint {
        let index = self.data.input.constraints.len();
        self.data
            .input
            .constraints
            .push(ConstraintEvents::default());
        let region = region.map(|r| self.region(r));
        let lifetime = if oneshot {
            Lifetime::Oneshot
        } else {
            Lifetime::Persistent
        };
        let locked = self.constraints_manager().lock_pointer(
            &toplevel.surface,
            &self._pointer,
            region.as_ref(),
            lifetime,
            &self.qh,
            index,
        );
        self.roundtrip();
        Constraint {
            index,
            locked: Some(locked),
            confined: None,
        }
    }

    /// Confine the pointer to `toplevel`'s surface, or to `region` of it.
    pub fn confine_pointer(
        &mut self,
        toplevel: &Toplevel,
        region: Option<(i32, i32, i32, i32)>,
        oneshot: bool,
    ) -> Constraint {
        let index = self.data.input.constraints.len();
        self.data
            .input
            .constraints
            .push(ConstraintEvents::default());
        let region = region.map(|r| self.region(r));
        let lifetime = if oneshot {
            Lifetime::Oneshot
        } else {
            Lifetime::Persistent
        };
        let confined = self.constraints_manager().confine_pointer(
            &toplevel.surface,
            &self._pointer,
            region.as_ref(),
            lifetime,
            &self.qh,
            index,
        );
        self.roundtrip();
        Constraint {
            index,
            locked: None,
            confined: Some(confined),
        }
    }

    /// What the compositor told `constraint` so far.
    pub fn constraint_events(&self, constraint: &Constraint) -> ConstraintEvents {
        self.data.input.constraints[constraint.index]
    }

    // ---- relative-pointer ----

    /// Ask for relative motion.
    pub fn track_relative_pointer(&mut self) {
        let manager: ZwpRelativePointerManagerV1 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zwp_relative_pointer_manager_v1");
        let relative = manager.get_relative_pointer(&self._pointer, &self.qh, ());
        self.data.input.relative_pointer = Some(relative);
        self.roundtrip();
    }

    /// The relative motions received, oldest first.
    pub fn relative_motions(&self) -> &[RelativeMotion] {
        &self.data.input.relative
    }

    // ---- pointer-gestures ----

    /// Ask for swipe, pinch and hold gestures.
    pub fn track_gestures(&mut self) {
        let gestures: ZwpPointerGesturesV1 = self
            ._globals
            .bind(&self.qh, 1..=3, ())
            .expect("binding zwp_pointer_gestures_v1");
        let pointer = &self._pointer;
        let objects: Vec<Box<dyn std::any::Any>> = vec![
            Box::new(gestures.get_swipe_gesture(pointer, &self.qh, ())),
            Box::new(gestures.get_pinch_gesture(pointer, &self.qh, ())),
            Box::new(gestures.get_hold_gesture(pointer, &self.qh, ())),
        ];
        self.data.input.gesture_objects.extend(objects);
        self.roundtrip();
    }

    /// The gesture events received, oldest first.
    pub fn gestures(&self) -> &[GestureSeen] {
        &self.data.input.gestures
    }

    // ---- text-input-v3 ----

    /// Create a text input for the seat.
    pub fn track_text_input(&mut self) {
        let manager: ZwpTextInputManagerV3 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zwp_text_input_manager_v3");
        let input = manager.get_text_input(&self.seat, &self.qh, ());
        self.data.input.text_input = Some(input);
        self.roundtrip();
    }

    /// Enable the text input with its caret at `rect` (`x, y, w, h`, surface coordinates),
    /// as a toolkit does when a text field gets focus.
    pub fn enable_text_input(&mut self, rect: (i32, i32, i32, i32)) {
        let input = self.data.input.text_input.as_ref().expect("no text input");
        input.enable();
        input.set_cursor_rectangle(rect.0, rect.1, rect.2, rect.3);
        input.commit();
        self.roundtrip();
    }

    /// Disable the text input (the field lost focus).
    pub fn disable_text_input(&mut self) {
        let input = self.data.input.text_input.as_ref().expect("no text input");
        input.disable();
        input.commit();
        self.roundtrip();
    }

    /// What the compositor told the text input.
    pub fn text_input_events(&self) -> &TextInputEvents {
        &self.data.input.text
    }

    // ---- input-method-v2 ----

    /// Become the seat's input method. `unavailable` is set if another one already is.
    pub fn become_input_method(&mut self) {
        let manager: ZwpInputMethodManagerV2 = self
            ._globals
            .bind(&self.qh, 1..=1, ())
            .expect("binding zwp_input_method_manager_v2");
        let method = manager.get_input_method(&self.seat, &self.qh, ());
        self.data.input.input_method = Some(method);
        self.roundtrip();
    }

    /// What the compositor told the input method.
    pub fn input_method_events(&self) -> InputMethodEvents {
        self.data.input.im
    }

    /// Commit `text` into the focused text input, with the serial protocol rules ask for.
    pub fn input_method_commit_string(&mut self, text: &str) {
        let method = self
            .data
            .input
            .input_method
            .as_ref()
            .expect("no input method");
        method.commit_string(text.to_owned());
        method.commit(self.data.input.im.done);
        self.roundtrip();
    }

    /// Show a candidate popup: a surface with an `input_popup_surface` role and a buffer.
    pub fn input_method_popup(&mut self, size: (i32, i32)) -> WlSurface {
        let method = self
            .data
            .input
            .input_method
            .as_ref()
            .expect("no input method");
        let surface = self.compositor.create_surface(&self.qh, ());
        let popup = method.get_input_popup_surface(&surface, &self.qh, ());
        self.data.input.popups.push(popup);
        let buffer = self.shm_buffer(size);
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, size.0, size.1);
        surface.commit();
        self.roundtrip();
        surface
    }
}

impl Dispatch<ZwpLockedPointerV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpLockedPointerV1,
        event: zwp_locked_pointer_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let events = &mut data.input.constraints[*index];
        match event {
            zwp_locked_pointer_v1::Event::Locked => {
                events.active = true;
                events.activations += 1;
            }
            zwp_locked_pointer_v1::Event::Unlocked => {
                events.active = false;
                events.deactivations += 1;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpConfinedPointerV1, usize> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpConfinedPointerV1,
        event: zwp_confined_pointer_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let events = &mut data.input.constraints[*index];
        match event {
            zwp_confined_pointer_v1::Event::Confined => {
                events.active = true;
                events.activations += 1;
            }
            zwp_confined_pointer_v1::Event::Unconfined => {
                events.active = false;
                events.deactivations += 1;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpRelativePointerV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpRelativePointerV1,
        event: zwp_relative_pointer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_relative_pointer_v1::Event::RelativeMotion {
            dx,
            dy,
            dx_unaccel,
            dy_unaccel,
            ..
        } = event
        {
            data.input.relative.push([dx, dy, dx_unaccel, dy_unaccel]);
        }
    }
}

impl Dispatch<ZwpPointerGestureSwipeV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_pointer_gesture_swipe_v1::Event;
        let seen = match event {
            Event::Begin { fingers, .. } => GestureSeen::SwipeBegin(fingers),
            Event::Update { dx, dy, .. } => GestureSeen::SwipeUpdate(dx, dy),
            Event::End { cancelled, .. } => GestureSeen::SwipeEnd {
                cancelled: cancelled != 0,
            },
            _ => return,
        };
        data.input.gestures.push(seen);
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_pointer_gesture_pinch_v1::Event;
        let seen = match event {
            Event::Begin { fingers, .. } => GestureSeen::PinchBegin(fingers),
            Event::Update {
                dx,
                dy,
                scale,
                rotation,
                ..
            } => GestureSeen::PinchUpdate {
                dx,
                dy,
                scale,
                rotation,
            },
            Event::End { cancelled, .. } => GestureSeen::PinchEnd {
                cancelled: cancelled != 0,
            },
            _ => return,
        };
        data.input.gestures.push(seen);
    }
}

impl Dispatch<ZwpPointerGestureHoldV1, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpPointerGestureHoldV1,
        event: zwp_pointer_gesture_hold_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_pointer_gesture_hold_v1::Event;
        let seen = match event {
            Event::Begin { fingers, .. } => GestureSeen::HoldBegin(fingers),
            Event::End { cancelled, .. } => GestureSeen::HoldEnd {
                cancelled: cancelled != 0,
            },
            _ => return,
        };
        data.input.gestures.push(seen);
    }
}

impl Dispatch<ZwpTextInputV3, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let text = &mut data.input.text;
        match event {
            zwp_text_input_v3::Event::Enter { .. } => text.entered = true,
            zwp_text_input_v3::Event::Leave { .. } => text.entered = false,
            zwp_text_input_v3::Event::CommitString { text: string } => {
                text.committed.extend(string);
            }
            zwp_text_input_v3::Event::PreeditString { text: string, .. } => text.preedit = string,
            zwp_text_input_v3::Event::Done { .. } => text.done += 1,
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for ClientData {
    fn event(
        data: &mut Self,
        _: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let im = &mut data.input.im;
        match event {
            zwp_input_method_v2::Event::Activate => {
                im.active = true;
                im.activations += 1;
            }
            zwp_input_method_v2::Event::Deactivate => {
                im.active = false;
                im.deactivations += 1;
            }
            zwp_input_method_v2::Event::Done => im.done += 1,
            zwp_input_method_v2::Event::Unavailable => im.unavailable = true,
            _ => {}
        }
    }
}

delegate_noop!(ClientData: ignore WlRegion);
delegate_noop!(ClientData: ZwpPointerConstraintsV1);
delegate_noop!(ClientData: ZwpRelativePointerManagerV1);
delegate_noop!(ClientData: ZwpPointerGesturesV1);
delegate_noop!(ClientData: ZwpTextInputManagerV3);
delegate_noop!(ClientData: ZwpInputMethodManagerV2);
delegate_noop!(ClientData: ignore ZwpInputPopupSurfaceV2);
