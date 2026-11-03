//! Interaction with server-side window chrome (task 1.11, ADR 0014): hit-testing the
//! title bar, its traffic lights and the invisible resize edges, hover and press feedback,
//! and what a click does.
//!
//! The chrome is not part of the client's surface, so the space cannot find it. A window's
//! *frame* is its client geometry plus the title bar ([`frame_geometry`]); [`hit_test`]
//! says what a point means for one frame and [`State::chrome_hit_at`] applies it to the
//! stack of windows. A pointer over chrome has no client focus, so clients never see it.

use neue_config::TitleBarDoubleClick;
use neue_paint::materials::title_bar::TrafficLight;
use smithay::desktop::Window;
use smithay::input::pointer::{Focus, GrabStartData};
use smithay::utils::{Logical, Point, Rectangle, Serial};

use crate::decorations::{Hover, TITLE_BAR_HEIGHT, frame_geometry, is_server_decorated};
use crate::grabs::{Edges, ResizeGrab, TitleBarGrab, TrafficLightGrab};
use crate::state::State;

/// How far outside a window's frame its resize edges reach, in logical pixels. They are
/// invisible and never cover client pixels.
pub const RESIZE_MARGIN: i32 = 5;
/// Along an edge, how far from a corner the edge counts as the corner too.
pub const RESIZE_CORNER: i32 = 12;
/// Longest gap (ms) between two clicks of a double click.
pub const DOUBLE_CLICK_MS: u32 = 400;

/// What a point over a window's chrome means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeHit {
    /// A traffic light.
    Button(TrafficLight),
    /// The rest of the title bar: drag to move, double click to zoom.
    TitleBar,
    /// A resize edge or corner just outside the frame.
    Edge(Edges),
}

/// What `pos` means for a window with `frame` (client plus title bar). `resizable` is
/// false for a zoomed window, which has no resize edges. `None` means the point is not on
/// the chrome: inside the client area, or too far away.
pub fn hit_test(
    frame: Rectangle<i32, Logical>,
    pos: Point<f64, Logical>,
    resizable: bool,
) -> Option<ChromeHit> {
    if frame.to_f64().contains(pos) {
        let bar = Rectangle::new(
            frame.loc,
            (frame.size.w, TITLE_BAR_HEIGHT.min(frame.size.h)).into(),
        );
        if !bar.to_f64().contains(pos) {
            return None;
        }
        let local = pos - frame.loc.to_f64();
        for button in TrafficLight::ALL {
            let r = button.rect();
            if (r.x0..r.x1).contains(&local.x) && (r.y0..r.y1).contains(&local.y) {
                return Some(ChromeHit::Button(button));
            }
        }
        return Some(ChromeHit::TitleBar);
    }
    if !resizable {
        return None;
    }
    let m = RESIZE_MARGIN;
    let outer = Rectangle::new(
        (frame.loc.x - m, frame.loc.y - m).into(),
        (frame.size.w + 2 * m, frame.size.h + 2 * m).into(),
    );
    if !outer.to_f64().contains(pos) {
        return None;
    }
    let (left_x, top_y) = (f64::from(frame.loc.x), f64::from(frame.loc.y));
    let (right_x, bottom_y) = (
        left_x + f64::from(frame.size.w),
        top_y + f64::from(frame.size.h),
    );
    let corner = f64::from(RESIZE_CORNER);
    let mut edges = Edges {
        left: pos.x < left_x,
        right: pos.x >= right_x,
        top: pos.y < top_y,
        bottom: pos.y >= bottom_y,
    };
    // Near a corner, an edge strip is the corner.
    if !edges.left && !edges.right {
        edges.left = pos.x < left_x + corner;
        edges.right = !edges.left && pos.x >= right_x - corner;
    } else if !edges.top && !edges.bottom {
        edges.top = pos.y < top_y + corner;
        edges.bottom = !edges.top && pos.y >= bottom_y - corner;
    }
    Some(ChromeHit::Edge(edges))
}

impl State {
    /// The window chrome under `pos` (global logical coordinates) and what part of it.
    /// A client surface above the chrome in the stacking order hides it.
    pub fn chrome_hit_at(&self, pos: Point<f64, Logical>) -> Option<(Window, ChromeHit)> {
        let client = self.space.element_under(pos).map(|(w, _)| w.clone());
        for window in self.space.elements().rev() {
            if client.as_ref() == Some(window) {
                return None;
            }
            if !is_server_decorated(window) {
                continue;
            }
            let Some(geometry) = self.space.element_geometry(window) else {
                continue;
            };
            let resizable = !Self::with_wm_data(window, |d| d.zoomed);
            if let Some(hit) = hit_test(frame_geometry(window, geometry), pos, resizable) {
                return Some((window.clone(), hit));
            }
        }
        None
    }

    /// The pointer moved to `pos` without a grab: light the traffic lights it is over and
    /// pick the cursor (a resize cursor on the edges, the arrow on the rest of the chrome).
    pub fn update_chrome_hover(&mut self, pos: Point<f64, Logical>) {
        let hit = self.chrome_hit_at(pos);
        let hover = match &hit {
            Some((window, ChromeHit::Button(_))) => Some(Hover {
                window: window.clone(),
                pressed: None,
            }),
            _ => None,
        };
        if self.chrome.hover != hover {
            self.chrome.hover = hover;
        }
        let over = hit.is_some();
        let icon = match hit {
            Some((_, ChromeHit::Edge(edges))) => Some(edges.cursor_icon()),
            _ => None,
        };
        if over != self.chrome.pointer_over || (over && icon != self.cursor_override) {
            self.set_cursor_override(icon);
        }
        self.chrome.pointer_over = over;
    }

    /// A button went down on `window`'s chrome at `hit`. Everything but the traffic lights
    /// raises and focuses the window; a left press starts the matching grab.
    pub(crate) fn press_chrome(
        &mut self,
        window: Window,
        hit: ChromeHit,
        button: u32,
        serial: Serial,
    ) {
        if !matches!(hit, ChromeHit::Button(_)) {
            self.focus_window(&window);
        }
        if button != crate::grabs::BTN_LEFT {
            return;
        }
        let pointer = self.pointer.clone();
        let start_data = GrabStartData {
            focus: None,
            button,
            location: pointer.current_location(),
        };
        match hit {
            ChromeHit::Button(which) => {
                self.chrome.hover = Some(Hover {
                    window: window.clone(),
                    pressed: Some(which),
                });
                let grab = TrafficLightGrab {
                    start_data,
                    window,
                    button: which,
                };
                pointer.set_grab(self, grab, serial, Focus::Clear);
            }
            ChromeHit::TitleBar => {
                let grab = TitleBarGrab {
                    start_data,
                    window,
                    moving: None,
                };
                pointer.set_grab(self, grab, serial, Focus::Clear);
            }
            ChromeHit::Edge(edges) => {
                let Some(geometry) = self.space.element_geometry(&window) else {
                    return;
                };
                Self::with_wm_data(&window, |d| d.grabbed = true);
                let grab = ResizeGrab::start(start_data, window, edges, geometry);
                self.set_cursor_override(Some(edges.cursor_icon()));
                pointer.set_grab(self, grab, serial, Focus::Clear);
            }
        }
    }

    /// A traffic light was clicked (pressed and released on it).
    pub(crate) fn activate_traffic_light(&mut self, window: &Window, button: TrafficLight) {
        match button {
            TrafficLight::Close => {
                if let Some(toplevel) = window.toplevel() {
                    toplevel.send_close();
                }
            }
            TrafficLight::Minimize => self.minimize_window(window),
            TrafficLight::Zoom => self.toggle_zoom(window),
        }
    }

    /// Zoom `window`, or restore it if it is zoomed.
    fn toggle_zoom(&mut self, window: &Window) {
        let zoomed = Self::with_wm_data(window, |d| d.zoomed);
        self.set_zoomed(window, !zoomed, None);
    }

    /// The title bar of `window` was clicked, without dragging, at `time` (ms). A second
    /// click within [`DOUBLE_CLICK_MS`] runs `[windows] title-bar-double-click`.
    pub(crate) fn title_bar_clicked(&mut self, window: &Window, time: u32) {
        let previous = self.chrome.last_click.take();
        let double =
            previous.is_some_and(|(w, t)| w == *window && time.wrapping_sub(t) <= DOUBLE_CLICK_MS);
        if !double {
            self.chrome.last_click = Some((window.clone(), time));
            return;
        }
        match self.config.active.windows.title_bar_double_click {
            TitleBarDoubleClick::Zoom => self.toggle_zoom(window),
            TitleBarDoubleClick::Minimize => self.minimize_window(window),
            TitleBarDoubleClick::None => {}
        }
    }

    /// `window` is gone: drop the chrome state that refers to it.
    pub(crate) fn chrome_window_gone(&mut self, window: &Window) {
        if self
            .chrome
            .hover
            .as_ref()
            .is_some_and(|h| h.window == *window)
        {
            self.chrome.hover = None;
        }
        if self
            .chrome
            .last_click
            .as_ref()
            .is_some_and(|(w, _)| w == window)
        {
            self.chrome.last_click = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 300×200 client at (100, 100) with its 22 px bar above it.
    fn frame() -> Rectangle<i32, Logical> {
        Rectangle::new(
            (100, 100 - TITLE_BAR_HEIGHT).into(),
            (300, 200 + TITLE_BAR_HEIGHT).into(),
        )
    }

    fn at(x: f64, y: f64) -> Point<f64, Logical> {
        Point::from((x, y))
    }

    fn edge(hit: Option<ChromeHit>) -> Edges {
        match hit {
            Some(ChromeHit::Edge(e)) => e,
            other => panic!("expected an edge, got {other:?}"),
        }
    }

    #[test]
    fn the_buttons_are_found_where_they_are_drawn() {
        let top = f64::from(frame().loc.y);
        for button in TrafficLight::ALL {
            let r = button.rect();
            let hit = hit_test(frame(), at(100.0 + r.center().x, top + r.center().y), true);
            assert_eq!(hit, Some(ChromeHit::Button(button)));
        }
        // Between two buttons it is the bar.
        let gap = TrafficLight::Close.rect().x1 + 1.0;
        assert_eq!(
            hit_test(frame(), at(100.0 + gap, top + 11.0), true),
            Some(ChromeHit::TitleBar)
        );
    }

    #[test]
    fn the_bar_is_the_title_bar_and_the_client_is_not_chrome() {
        assert_eq!(
            hit_test(frame(), at(250.0, 90.0), true),
            Some(ChromeHit::TitleBar)
        );
        // Last row of the bar, first row of the client.
        assert_eq!(
            hit_test(frame(), at(250.0, 99.5), true),
            Some(ChromeHit::TitleBar)
        );
        assert_eq!(hit_test(frame(), at(250.0, 100.0), true), None);
        assert_eq!(hit_test(frame(), at(100.0, 299.5), true), None);
    }

    #[test]
    fn edges_sit_outside_the_frame() {
        let f = frame();
        let left = edge(hit_test(f, at(97.0, 200.0), true));
        assert_eq!(
            left,
            Edges {
                left: true,
                ..Edges::default()
            }
        );
        let bottom = edge(hit_test(f, at(250.0, 303.0), true));
        assert_eq!(
            bottom,
            Edges {
                bottom: true,
                ..Edges::default()
            }
        );
        let top = edge(hit_test(f, at(250.0, 75.0), true));
        assert_eq!(
            top,
            Edges {
                top: true,
                ..Edges::default()
            }
        );
        // Just past the margin, and the pixel inside the frame, are not edges.
        assert_eq!(hit_test(f, at(94.5, 200.0), true), None);
        assert_eq!(hit_test(f, at(250.0, 307.0), true), None);
    }

    #[test]
    fn corners_cover_the_ends_of_the_edges() {
        let f = frame();
        let (x1, y1) = (400.0, 300.0);
        let corner = |x, y| edge(hit_test(f, at(x, y), true));
        let all = |left, right, top, bottom| Edges {
            left,
            right,
            top,
            bottom,
        };
        // Diagonal outside the corner.
        assert_eq!(corner(96.0, 74.0), all(true, false, true, false));
        assert_eq!(corner(x1 + 2.0, y1 + 2.0), all(false, true, false, true));
        // Along the left edge near the bottom, and along the top edge near the right.
        assert_eq!(corner(97.0, y1 - 3.0), all(true, false, false, true));
        assert_eq!(corner(x1 - 3.0, 75.0), all(false, true, true, false));
        // Mid-edge stays a plain edge.
        assert_eq!(corner(97.0, 200.0), all(true, false, false, false));
    }

    #[test]
    fn a_zoomed_window_has_no_edges_but_keeps_its_bar() {
        assert_eq!(hit_test(frame(), at(97.0, 200.0), false), None);
        assert_eq!(
            hit_test(frame(), at(250.0, 90.0), false),
            Some(ChromeHit::TitleBar)
        );
    }
}
