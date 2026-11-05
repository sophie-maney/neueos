//! Pointer grabs for interactive move and resize (`xdg_toplevel.move` / `.resize`).
//!
//! Adapted from Smithay's `smallvil` (MIT): `smallvil/src/grabs/`.

use smithay::desktop::Window;
use smithay::input::SeatHandler;
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorIcon, GestureHoldBeginEvent, GestureHoldEndEvent,
    GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
    GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData, MotionEvent, PointerGrab,
    PointerInnerHandle, RelativeMotionEvent,
};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle, Size};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::SurfaceCachedState;

use neue_paint::materials::title_bar::TrafficLight;

use crate::decorations::Hover;
use crate::interaction::ChromeHit;
use crate::state::State;
use crate::wm::ResizeState;

type Focus = Option<(<State as SeatHandler>::PointerFocus, Point<f64, Logical>)>;

/// The `PointerGrab` methods both grabs pass straight through.
macro_rules! forward_grab_events {
    () => {
        fn relative_motion(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            focus: Focus,
            event: &RelativeMotionEvent,
        ) {
            handle.relative_motion(state, focus, event);
        }

        fn axis(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            details: AxisFrame,
        ) {
            handle.axis(state, details);
        }

        fn frame(&mut self, state: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
            handle.frame(state);
        }

        fn gesture_swipe_begin(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureSwipeBeginEvent,
        ) {
            handle.gesture_swipe_begin(state, event);
        }

        fn gesture_swipe_update(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureSwipeUpdateEvent,
        ) {
            handle.gesture_swipe_update(state, event);
        }

        fn gesture_swipe_end(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureSwipeEndEvent,
        ) {
            handle.gesture_swipe_end(state, event);
        }

        fn gesture_pinch_begin(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GesturePinchBeginEvent,
        ) {
            handle.gesture_pinch_begin(state, event);
        }

        fn gesture_pinch_update(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GesturePinchUpdateEvent,
        ) {
            handle.gesture_pinch_update(state, event);
        }

        fn gesture_pinch_end(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GesturePinchEndEvent,
        ) {
            handle.gesture_pinch_end(state, event);
        }

        fn gesture_hold_begin(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureHoldBeginEvent,
        ) {
            handle.gesture_hold_begin(state, event);
        }

        fn gesture_hold_end(
            &mut self,
            state: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureHoldEndEvent,
        ) {
            handle.gesture_hold_end(state, event);
        }
    };
}

/// Which edges a resize moves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl From<xdg_toplevel::ResizeEdge> for Edges {
    fn from(edge: xdg_toplevel::ResizeEdge) -> Self {
        use xdg_toplevel::ResizeEdge as E;
        Self {
            left: matches!(edge, E::Left | E::TopLeft | E::BottomLeft),
            right: matches!(edge, E::Right | E::TopRight | E::BottomRight),
            top: matches!(edge, E::Top | E::TopLeft | E::TopRight),
            bottom: matches!(edge, E::Bottom | E::BottomLeft | E::BottomRight),
        }
    }
}

impl Edges {
    /// The cursor that shows which edges are being dragged.
    pub fn cursor_icon(self) -> CursorIcon {
        match (self.left, self.right, self.top, self.bottom) {
            (true, _, true, _) => CursorIcon::NwResize,
            (_, true, true, _) => CursorIcon::NeResize,
            (true, _, _, true) => CursorIcon::SwResize,
            (_, true, _, true) => CursorIcon::SeResize,
            (true, ..) => CursorIcon::WResize,
            (_, true, ..) => CursorIcon::EResize,
            (_, _, true, _) => CursorIcon::NResize,
            (.., true) => CursorIcon::SResize,
            _ => CursorIcon::Default,
        }
    }

    /// Where a window that started at `initial` goes once it has `size`: the edges that
    /// are not being dragged stay put.
    pub fn anchored_location(
        self,
        initial: Rectangle<i32, Logical>,
        size: Size<i32, Logical>,
    ) -> Point<i32, Logical> {
        let mut loc = initial.loc;
        if self.left {
            loc.x = initial.loc.x + initial.size.w - size.w;
        }
        if self.top {
            loc.y = initial.loc.y + initial.size.h - size.h;
        }
        loc
    }
}

/// Size for a resize of `initial` along `edges` by `delta`, within the client's min/max.
pub fn resized(
    initial: Size<i32, Logical>,
    edges: Edges,
    delta: Point<f64, Logical>,
    (min, max): (Size<i32, Logical>, Size<i32, Logical>),
) -> Size<i32, Logical> {
    // Pointer deltas are small; rounding to whole logical pixels is intended.
    let dx = delta.x.round() as i32;
    let dy = delta.y.round() as i32;
    let mut w = initial.w;
    let mut h = initial.h;
    if edges.left {
        w -= dx;
    } else if edges.right {
        w += dx;
    }
    if edges.top {
        h -= dy;
    } else if edges.bottom {
        h += dy;
    }
    let limit = |v: i32, min: i32, max: i32| {
        let max = if max == 0 { i32::MAX } else { max };
        v.min(max).max(min.max(1))
    };
    Size::from((limit(w, min.w, max.w), limit(h, min.h, max.h)))
}

/// The client's minimum and maximum size (0 means unlimited).
pub fn size_limits(surface: &WlSurface) -> (Size<i32, Logical>, Size<i32, Logical>) {
    with_states(surface, |states| {
        let mut cached = states.cached_state.get::<SurfaceCachedState>();
        let current = cached.current();
        (current.min_size, current.max_size)
    })
}

/// The left button, from `linux/input-event-codes.h`.
pub(crate) const BTN_LEFT: u32 = 0x110;

/// How far (logical pixels) the pointer must travel with the button down on a title bar
/// before it is a drag and not a click.
const DRAG_THRESHOLD: f64 = 3.0;

/// Dragging a window with the pointer: by our title bar ([`TitleBarGrab`]) or for a
/// client's `xdg_toplevel.move`. The move only begins once the pointer has travelled
/// [`DRAG_THRESHOLD`] (task 2.36): until then nothing happens to the window, so a click
/// (or the first click of a double click) on a zoomed window's toolbar leaves it zoomed.
pub struct MoveGrab {
    pub start_data: GrabStartData<State>,
    pub window: Window,
    /// Where the window's geometry started, once the move began ([`State::begin_move`]).
    initial_location: Option<Point<i32, Logical>>,
}

impl MoveGrab {
    /// A press at `start_data.location` that may become a move of `window`.
    pub fn new(start_data: GrabStartData<State>, window: Window) -> Self {
        Self {
            start_data,
            window,
            initial_location: None,
        }
    }

    /// Whether the pointer has travelled far enough for the window to move.
    pub fn started(&self) -> bool {
        self.initial_location.is_some()
    }

    /// The pointer is at `pointer`: begin the move once it is past the threshold, then
    /// put the window where the pointer drags it.
    fn track(&mut self, state: &mut State, pointer: Point<f64, Logical>) {
        if self.initial_location.is_none() {
            let d = pointer - self.start_data.location;
            if d.x.hypot(d.y) < DRAG_THRESHOLD {
                return;
            }
            self.initial_location = state.begin_move(&self.window, self.start_data.location);
        }
        let Some(initial) = self.initial_location else {
            return;
        };
        let delta = pointer - self.start_data.location;
        let location = (initial.to_f64() + delta).to_i32_round();
        state.move_window_to(&self.window, location);
    }

    /// The grab ended: a window that moved is reported again.
    fn finish(&self, state: &mut State) {
        if self.started() {
            state.grab_ended(&self.window);
        }
    }
}

impl PointerGrab<State> for MoveGrab {
    fn motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Focus,
        event: &MotionEvent,
    ) {
        // No client has pointer focus during the grab.
        handle.motion(state, None, event);
        self.track(state, event.location);
    }

    fn button(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(state, event);
        if !handle.current_pressed().contains(&BTN_LEFT) {
            handle.unset_grab(self, state, event.serial, event.time, true);
        }
    }

    fn start_data(&self) -> &GrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, state: &mut State) {
        self.finish(state);
    }

    forward_grab_events!();
}

/// A press on a server-side title bar (task 1.11). It becomes a window move once the
/// pointer has travelled [`DRAG_THRESHOLD`]; released before that, it is a click, and two
/// clicks in a row are a double click ([`State::title_bar_clicked`]).
pub struct TitleBarGrab {
    pub moving: MoveGrab,
}

impl PointerGrab<State> for TitleBarGrab {
    fn motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Focus,
        event: &MotionEvent,
    ) {
        handle.motion(state, None, event);
        self.moving.track(state, event.location);
    }

    fn button(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(state, event);
        if !handle.current_pressed().contains(&BTN_LEFT) {
            let pos = handle.current_location();
            handle.unset_grab(self, state, event.serial, event.time, true);
            if !self.moving.started() && event.button == BTN_LEFT {
                state.title_bar_clicked(&self.moving.window, event.time.millis());
            }
            state.update_chrome_hover(pos);
        }
    }

    fn start_data(&self) -> &GrabStartData<State> {
        &self.moving.start_data
    }

    fn unset(&mut self, state: &mut State) {
        self.moving.finish(state);
    }

    forward_grab_events!();
}

/// A press on a traffic light (task 1.11). The button shows as pressed while the pointer
/// is over it; releasing it there runs its action.
pub struct TrafficLightGrab {
    pub start_data: GrabStartData<State>,
    pub window: Window,
    pub button: TrafficLight,
}

impl TrafficLightGrab {
    /// Update what the buttons look like for a pointer at `pos`; whether it is over our
    /// button.
    fn track(&self, state: &mut State, pos: Point<f64, Logical>) -> bool {
        let over = match state.chrome_hit_at(pos) {
            Some((window, ChromeHit::Button(button))) if window == self.window => Some(button),
            _ => None,
        };
        state.chrome.hover = over.map(|_| Hover {
            window: self.window.clone(),
            pressed: (over == Some(self.button)).then_some(self.button),
        });
        over == Some(self.button)
    }
}

impl PointerGrab<State> for TrafficLightGrab {
    fn motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Focus,
        event: &MotionEvent,
    ) {
        handle.motion(state, None, event);
        self.track(state, event.location);
    }

    fn button(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(state, event);
        if !handle.current_pressed().contains(&BTN_LEFT) {
            let pos = handle.current_location();
            let activate = self.track(state, pos) && event.button == BTN_LEFT;
            handle.unset_grab(self, state, event.serial, event.time, true);
            if activate {
                state.activate_traffic_light(&self.window, self.button);
            }
            state.update_chrome_hover(pos);
        }
    }

    fn start_data(&self) -> &GrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, state: &mut State) {
        state.chrome.hover = None;
    }

    forward_grab_events!();
}

/// Dragging a window edge or corner.
pub struct ResizeGrab {
    pub start_data: GrabStartData<State>,
    pub window: Window,
    pub edges: Edges,
    pub initial: Rectangle<i32, Logical>,
    pub last_size: Size<i32, Logical>,
}

impl ResizeGrab {
    /// Start resizing `window`, which currently has geometry `initial`.
    pub fn start(
        start_data: GrabStartData<State>,
        window: Window,
        edges: Edges,
        initial: Rectangle<i32, Logical>,
    ) -> Self {
        State::with_wm_data(&window, |data| {
            data.resize = ResizeState::Resizing { edges, initial };
        });
        if let Some(toplevel) = window.toplevel() {
            toplevel.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });
            toplevel.send_pending_configure();
        }
        Self {
            start_data,
            window,
            edges,
            initial,
            last_size: initial.size,
        }
    }
}

impl PointerGrab<State> for ResizeGrab {
    fn motion(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Focus,
        event: &MotionEvent,
    ) {
        handle.motion(state, None, event);
        let Some(toplevel) = self.window.toplevel() else {
            return;
        };
        let delta = event.location - self.start_data.location;
        let limits = size_limits(toplevel.wl_surface());
        self.last_size = resized(self.initial.size, self.edges, delta, limits);
        toplevel.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Resizing);
            state.size = Some(self.last_size);
        });
        toplevel.send_pending_configure();
    }

    fn button(
        &mut self,
        state: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(state, event);
        if !handle.current_pressed().contains(&BTN_LEFT) {
            handle.unset_grab(self, state, event.serial, event.time, true);
        }
    }

    fn start_data(&self) -> &GrabStartData<State> {
        &self.start_data
    }

    fn unset(&mut self, state: &mut State) {
        // The client under the pointer sets its own cursor when focus returns to it.
        state.set_cursor_override(None);
        if let Some(toplevel) = self.window.toplevel() {
            toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Resizing);
                state.size = Some(self.last_size);
            });
            toplevel.send_pending_configure();
        }
        // The client still has to commit the final size; the commit handler moves the
        // window for top/left edges and then reports the change.
        let (edges, initial) = (self.edges, self.initial);
        State::with_wm_data(&self.window, |data| {
            data.resize = ResizeState::WaitingForLastCommit { edges, initial };
        });
    }

    forward_grab_events!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_limits() -> (Size<i32, Logical>, Size<i32, Logical>) {
        (Size::from((0, 0)), Size::from((0, 0)))
    }

    #[test]
    fn bottom_right_grows_with_the_pointer() {
        let edges = Edges::from(xdg_toplevel::ResizeEdge::BottomRight);
        let size = resized((300, 200).into(), edges, (40.0, -20.0).into(), no_limits());
        assert_eq!(size, (340, 180).into());
    }

    #[test]
    fn top_left_grows_against_the_pointer_and_anchors_bottom_right() {
        let edges = Edges::from(xdg_toplevel::ResizeEdge::TopLeft);
        let size = resized((300, 200).into(), edges, (-40.0, -20.0).into(), no_limits());
        assert_eq!(size, (340, 220).into());
        let initial = Rectangle::new((100, 100).into(), (300, 200).into());
        assert_eq!(edges.anchored_location(initial, size), (60, 80).into());
    }

    #[test]
    fn limits_apply() {
        let edges = Edges::from(xdg_toplevel::ResizeEdge::Right);
        let limits = (Size::from((250, 0)), Size::from((320, 0)));
        let small = resized((300, 200).into(), edges, (-200.0, 0.0).into(), limits);
        assert_eq!(small, (250, 200).into());
        let big = resized((300, 200).into(), edges, (200.0, 0.0).into(), limits);
        assert_eq!(big, (320, 200).into());
        let tiny = resized((300, 200).into(), edges, (-400.0, 0.0).into(), no_limits());
        assert_eq!(tiny, (1, 200).into());
    }
}
