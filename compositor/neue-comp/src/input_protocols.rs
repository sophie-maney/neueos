//! Input protocols (task 1.13, ADR 0016): `pointer-constraints`, `relative-pointer`,
//! `pointer-gestures`, `text-input-v3` and `input-method-v2`.
//!
//! Smithay implements the protocols; this module holds the policy: when a constraint is
//! active, how motion is limited by it, which gestures reach clients, and where an input
//! method's popup goes.

use smithay::backend::input::InputTime;
use smithay::desktop::{PopupKind, PopupManager};
use smithay::input::pointer::{
    GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
    GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent,
    PointerHandle, RelativeMotionEvent,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, SERIAL_COUNTER};
use smithay::wayland::compositor::{RegionAttributes, get_parent};
use smithay::wayland::input_method::{InputMethodHandler, PopupSurface};
use smithay::wayland::pointer_constraints::{
    ConstraintRemove, PointerConstraint, PointerConstraintsHandler, with_pointer_constraint,
};
use smithay::wayland::text_input::TextInputActivation;
use tracing::{debug, warn};

use crate::state::State;

/// A touchpad gesture, from libinput or scripted by a test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    SwipeBegin {
        fingers: u32,
    },
    SwipeUpdate {
        delta: Point<f64, Logical>,
    },
    SwipeEnd {
        cancelled: bool,
    },
    PinchBegin {
        fingers: u32,
    },
    PinchUpdate {
        delta: Point<f64, Logical>,
        /// Scale relative to the start of the gesture.
        scale: f64,
        /// Degrees clockwise since the previous update.
        rotation: f64,
    },
    PinchEnd {
        cancelled: bool,
    },
    HoldBegin {
        fingers: u32,
    },
    HoldEnd {
        cancelled: bool,
    },
}

/// What an active constraint does to the pointer.
enum Limit {
    /// The pointer does not move; clients get relative motion only.
    Lock,
    /// The pointer stays inside the surface and, if set, the region (surface coordinates).
    Confine(Option<RegionAttributes>),
}

/// The topmost parent of `surface` (a subsurface's toplevel).
fn root_surface(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

/// The point furthest from `from` towards `to` for which `inside` holds, found by halving
/// the step. `from` itself counts as inside.
fn farthest(
    inside: &impl Fn(Point<f64, Logical>) -> bool,
    from: Point<f64, Logical>,
    to: Point<f64, Logical>,
) -> Point<f64, Logical> {
    if from == to || inside(to) {
        return to;
    }
    let (mut near, mut far) = (0.0, 1.0);
    for _ in 0..12 {
        let mid = (near + far) / 2.0;
        if inside(from + (to - from).upscale(mid)) {
            near = mid;
        } else {
            far = mid;
        }
    }
    from + (to - from).upscale(near)
}

/// Whether `region` (none: the whole surface) contains `point`, in surface coordinates.
fn region_contains(region: Option<&RegionAttributes>, point: Point<f64, Logical>) -> bool {
    region.is_none_or(|region| region.contains(point.to_i32_round()))
}

impl State {
    /// Send the pointer's focused client a relative motion of `delta` and move the pointer
    /// by it, as far as the outputs and an active pointer constraint allow. Backends call
    /// this for mice and touchpads; tests use it to script input.
    pub fn move_pointer_by(
        &mut self,
        delta: Point<f64, Logical>,
        delta_unaccel: Point<f64, Logical>,
        time: InputTime,
    ) {
        self.notify_activity();
        let from = self.pointer.current_location();
        let pointer = self.pointer.clone();
        // Relative motion goes out even when the pointer cannot move (a locked pointer).
        let under = self.surface_under(from);
        pointer.relative_motion(
            self,
            under,
            &RelativeMotionEvent {
                delta,
                delta_unaccel,
                time,
            },
        );
        let rects = self.outputs().filter_map(|o| self.space.output_geometry(o));
        let moved = crate::outputs::clamp_to_nearest(from + delta, rects)
            .and_then(|target| self.constrain_motion(from, target));
        match moved {
            Some(pos) => self.move_pointer(pos, time),
            None => pointer.frame(self),
        }
    }

    /// Move the pointer towards `target` (global logical coordinates), as far as an active
    /// pointer constraint allows: a locked pointer stays, a confined one stops at its edge.
    /// Backends call this for absolute motion; tests use it to script input.
    pub fn move_pointer_towards(&mut self, target: Point<f64, Logical>, time: InputTime) {
        let from = self.pointer.current_location();
        if let Some(pos) = self.constrain_motion(from, target) {
            self.move_pointer(pos, time);
        }
    }

    /// Where the pointer ends up when it tries to go from `from` to `target`: `target`
    /// unless an active constraint holds it back. `None`: it stays.
    pub(crate) fn constrain_motion(
        &self,
        from: Point<f64, Logical>,
        target: Point<f64, Logical>,
    ) -> Option<Point<f64, Logical>> {
        let Some((surface, origin, limit)) = self.active_constraint(from) else {
            return Some(target);
        };
        let Limit::Confine(region) = limit else {
            return None;
        };
        let inside = |pos: Point<f64, Logical>| {
            self.surface_under(pos)
                .is_some_and(|(hit, _)| hit == surface)
                && region_contains(region.as_ref(), pos - origin)
        };
        // As far along the way as the confinement allows, then slide along the edge: each
        // axis of what is left on its own.
        let reached = farthest(&inside, from, target);
        let reached = farthest(&inside, reached, Point::from((target.x, reached.y)));
        let reached = farthest(&inside, reached, Point::from((reached.x, target.y)));
        (reached != from).then_some(reached)
    }

    /// The constraint that holds the pointer at `pos`, with the origin of its surface.
    fn active_constraint(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>, Limit)> {
        let surface = self.constrained_surface.clone()?;
        let (hit, origin) = self.surface_under(pos)?;
        if hit != surface {
            return None;
        }
        let limit = with_pointer_constraint(&surface, &self.pointer, |constraint| {
            let constraint = constraint.filter(|c| c.is_active())?;
            let region = constraint.region().cloned();
            Some(match &*constraint {
                PointerConstraint::Locked(_) => Limit::Lock,
                PointerConstraint::Confined(_) => Limit::Confine(region),
            })
        })?;
        Some((surface, origin, limit))
    }

    /// The surface whose constraint should be active now: the one under the pointer, if it
    /// belongs to the window with keyboard focus (a window in the background cannot hold
    /// the pointer), the session is not locked, and the pointer is inside the region.
    fn constraint_candidate(&self, keyboard_focus: Option<&WlSurface>) -> Option<WlSurface> {
        if self.is_locked() {
            return None;
        }
        let (focus, origin) = self.surface_under(self.pointer_pos)?;
        if root_surface(&focus) != root_surface(keyboard_focus?) {
            return None;
        }
        let pos = self.pointer_pos - origin;
        with_pointer_constraint(&focus, &self.pointer, |constraint| {
            constraint
                .filter(|c| region_contains(c.region(), pos))
                .map(|_| ())
        })
        .map(|()| focus)
    }

    /// Activate the pointer constraint that applies now and deactivate the one that no
    /// longer does. Called when the pointer moves, focus changes and constraints are made.
    pub fn refresh_pointer_constraint(&mut self) {
        let focus = self.keyboard_focus();
        self.refresh_pointer_constraint_for(focus.as_ref());
    }

    /// [`State::refresh_pointer_constraint`] for code that runs while Smithay changes the
    /// keyboard focus and holds its lock: the focus is passed in, not read.
    pub fn refresh_pointer_constraint_for(&mut self, keyboard_focus: Option<&WlSurface>) {
        if self.constraint_count == 0 && self.constrained_surface.is_none() {
            return;
        }
        let wanted = self.constraint_candidate(keyboard_focus);
        if wanted != self.constrained_surface
            && let Some(old) = self.constrained_surface.take()
        {
            with_pointer_constraint(&old, &self.pointer, |constraint| {
                if let Some(constraint) = constraint {
                    constraint.deactivate();
                }
            });
        }
        if let Some(surface) = &wanted {
            with_pointer_constraint(surface, &self.pointer, |constraint| {
                if let Some(constraint) = constraint {
                    constraint.activate();
                }
            });
        }
        self.constrained_surface = wanted;
    }

    /// Where `surface`, a toplevel, has its origin in global coordinates.
    fn toplevel_origin(&self, surface: &WlSurface) -> Option<Point<f64, Logical>> {
        let window = self.window_for_surface(surface)?;
        let location = self.space.element_location(&window)?;
        Some((location - window.geometry().loc).to_f64())
    }

    /// Send a touchpad gesture to the focused client. Dropped when `[touchpad] gestures`
    /// is off.
    pub fn gesture(&mut self, gesture: Gesture, time: InputTime) {
        self.notify_activity();
        if !self.config.active.touchpad.gestures {
            return;
        }
        let pointer = self.pointer.clone();
        let serial = SERIAL_COUNTER.next_serial();
        match gesture {
            Gesture::SwipeBegin { fingers } => pointer.gesture_swipe_begin(
                self,
                &GestureSwipeBeginEvent {
                    serial,
                    time,
                    fingers,
                },
            ),
            Gesture::SwipeUpdate { delta } => {
                pointer.gesture_swipe_update(self, &GestureSwipeUpdateEvent { time, delta });
            }
            Gesture::SwipeEnd { cancelled } => pointer.gesture_swipe_end(
                self,
                &GestureSwipeEndEvent {
                    serial,
                    time,
                    cancelled,
                },
            ),
            Gesture::PinchBegin { fingers } => pointer.gesture_pinch_begin(
                self,
                &GesturePinchBeginEvent {
                    serial,
                    time,
                    fingers,
                },
            ),
            Gesture::PinchUpdate {
                delta,
                scale,
                rotation,
            } => pointer.gesture_pinch_update(
                self,
                &GesturePinchUpdateEvent {
                    time,
                    delta,
                    scale,
                    rotation,
                },
            ),
            Gesture::PinchEnd { cancelled } => pointer.gesture_pinch_end(
                self,
                &GesturePinchEndEvent {
                    serial,
                    time,
                    cancelled,
                },
            ),
            Gesture::HoldBegin { fingers } => pointer.gesture_hold_begin(
                self,
                &GestureHoldBeginEvent {
                    serial,
                    time,
                    fingers,
                },
            ),
            Gesture::HoldEnd { cancelled } => pointer.gesture_hold_end(
                self,
                &GestureHoldEndEvent {
                    serial,
                    time,
                    cancelled,
                },
            ),
        }
    }
}

impl PointerConstraintsHandler for State {
    fn new_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {
        self.constraint_count += 1;
        self.refresh_pointer_constraint();
    }

    fn remove_constraint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        remove: ConstraintRemove,
    ) {
        self.constraint_count = self.constraint_count.saturating_sub(1);
        if self.constrained_surface.as_ref() == Some(surface) {
            self.constrained_surface = None;
        }
        // A client that destroys an active lock may say where it drew its own cursor: the
        // pointer goes there, so it does not jump back to where the lock began.
        if let ConstraintRemove::Destroyed(constraint @ PointerConstraint::Locked(locked)) = &remove
            && constraint.is_active()
            && let Some(hint) = locked.cursor_position_hint()
            && let Some(origin) = self.toplevel_origin(surface)
        {
            debug!(?hint, "pointer lock ended with a cursor position hint");
            self.pointer_pos = origin + hint;
            pointer.set_location(self.pointer_pos);
        }
    }
}

impl TextInputActivation for State {
    // Nothing to do on (de)activation: the input method is told by Smithay.
}

impl InputMethodHandler for State {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            warn!(%err, "cannot track an input method popup");
        }
    }

    fn popup_repositioned(&mut self, _surface: PopupSurface) {
        // The popup follows the cursor rectangle; it is drawn with its window.
    }

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            // A popup that is already gone is not an error.
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.window_for_surface(parent)
            .map(|window| window.geometry())
            .unwrap_or_default()
    }
}
