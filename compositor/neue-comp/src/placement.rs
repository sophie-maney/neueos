//! Window placement and position constraints (task 1.5, ADR 0008).
//!
//! Pure geometry so it can be tested without a Wayland server. All rectangles are window
//! geometry (without client-side shadows) in global logical coordinates. The *work area*
//! is the part of an output not reserved by panels such as the menu bar (layer-shell
//! exclusive zones, task 1.12); until then it is the whole output.

/// Offset between a window and the next window of the same app, in logical pixels.
pub const CASCADE_STEP: i32 = 24;

/// A rectangle in logical coordinates: `(x, y, width, height)`.
pub type Rect = (i32, i32, i32, i32);

/// What placement knows about a new window.
#[derive(Debug, Clone, Copy, Default)]
pub struct NewWindow {
    /// Size of the new window.
    pub size: (i32, i32),
    /// Geometry of its parent (`xdg_toplevel.set_parent`), if mapped.
    pub parent: Option<Rect>,
    /// Geometry of the topmost mapped window of the same app.
    pub sibling: Option<Rect>,
    /// Where the last closed window of the same app was, this session.
    pub remembered: Option<(i32, i32)>,
}

/// Where a new window goes, in order of preference:
///
/// 1. centred on its parent (dialogs);
/// 2. one step down and right of the topmost window of the same app (cascade), restarting
///    at the work area's edge when that would run off it;
/// 3. where the app's last window was closed;
/// 4. centred horizontally, with a third of the free height above it and two thirds below
///    (a little above the centre, which reads as centred).
///
/// The result is then [`constrain`]ed to the work area.
pub fn place(work: Rect, new: &NewWindow) -> (i32, i32) {
    let (w, h) = new.size;
    let (wx, wy, ww, wh) = work;
    let pos = if let Some((px, py, pw, ph)) = new.parent {
        (px + (pw - w) / 2, py + (ph - h) / 2)
    } else if let Some((sx, sy, _, _)) = new.sibling {
        let mut x = sx + CASCADE_STEP;
        let mut y = sy + CASCADE_STEP;
        if x + w > wx + ww {
            x = wx;
        }
        if y + h > wy + wh {
            y = wy;
        }
        (x, y)
    } else if let Some(pos) = new.remembered {
        pos
    } else {
        (wx + (ww - w) / 2, wy + (wh - h) / 3)
    };
    constrain(work, new.size, pos)
}

/// `pos` moved so a window of `size` lies inside `work` where it fits. A window wider or
/// taller than the work area is aligned to its left or top edge. The top edge is never
/// above the work area, so the title bar can't slide under the menu bar.
pub fn constrain(work: Rect, size: (i32, i32), pos: (i32, i32)) -> (i32, i32) {
    let (wx, wy, ww, wh) = work;
    let clamp = |p: i32, len: i32, start: i32, room: i32| {
        if len >= room {
            start
        } else {
            p.clamp(start, start + room - len)
        }
    };
    (clamp(pos.0, size.0, wx, ww), clamp(pos.1, size.1, wy, wh))
}

/// Where a sheet's frame (its title bar, if it has one, and client area) goes: centred
/// horizontally on its parent's client area `parent`, its top `chrome` below the top of
/// it. That is just under a server-side title bar (`chrome` 0), or under the title bar and
/// toolbar a window draws itself, as far down as it announced (`neue_window_v1`, ADR 0064;
/// clamped to the parent's height). Not constrained to the work area: a sheet stays
/// attached to its parent even when the parent hangs off the screen.
pub fn sheet(parent: Rect, chrome: i32, size: (i32, i32)) -> (i32, i32) {
    let (px, py, pw, ph) = parent;
    (px + (pw - size.0) / 2, py + chrome.clamp(0, ph.max(0)))
}

/// The position for an interactive or scripted move to `pos`: free, except that the top
/// edge stays at or below the top of the work area (OS X keeps title bars below the menu
/// bar) and above its bottom, so the window can always be grabbed again.
pub fn constrain_move(work: Rect, pos: (i32, i32)) -> (i32, i32) {
    let (_, wy, _, wh) = work;
    (pos.0, pos.1.clamp(wy, (wy + wh - 1).max(wy)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = (0, 0, 1280, 800);

    fn new(size: (i32, i32)) -> NewWindow {
        NewWindow {
            size,
            ..NewWindow::default()
        }
    }

    #[test]
    fn first_window_is_centred_a_little_high() {
        assert_eq!(place(WORK, &new((640, 500))), (320, 100));
    }

    #[test]
    fn dialogs_centre_on_their_parent() {
        let dialog = NewWindow {
            parent: Some((100, 100, 600, 400)),
            ..new((200, 100))
        };
        assert_eq!(place(WORK, &dialog), (300, 250));
    }

    #[test]
    fn same_app_cascades_from_its_topmost_window() {
        let second = NewWindow {
            sibling: Some((300, 200, 640, 480)),
            ..new((640, 480))
        };
        assert_eq!(
            place(WORK, &second),
            (300 + CASCADE_STEP, 200 + CASCADE_STEP)
        );
    }

    #[test]
    fn cascade_restarts_at_the_edges() {
        let next = NewWindow {
            sibling: Some((630, 310, 640, 480)),
            ..new((640, 480))
        };
        assert_eq!(place(WORK, &next), (0, 0));
    }

    #[test]
    fn remembered_position_is_reused_and_constrained() {
        let reopened = NewWindow {
            remembered: Some((50, 60)),
            ..new((300, 200))
        };
        assert_eq!(place(WORK, &reopened), (50, 60));
        let off_screen = NewWindow {
            remembered: Some((5000, -40)),
            ..new((300, 200))
        };
        assert_eq!(place(WORK, &off_screen), (980, 0));
    }

    #[test]
    fn work_area_offset_keeps_windows_below_the_menu_bar() {
        let work = (0, 22, 1280, 778);
        let (_, y) = place(work, &new((640, 900)));
        assert_eq!(y, 22);
        let dialog = NewWindow {
            parent: Some((0, 22, 200, 100)),
            ..new((400, 300))
        };
        assert_eq!(place(work, &dialog), (0, 22));
    }

    #[test]
    fn sheets_hang_from_the_top_of_their_parent() {
        assert_eq!(sheet((100, 122, 600, 400), 0, (200, 150)), (300, 122));
        // Wider than its parent, or with the parent off screen: still attached.
        assert_eq!(sheet((-500, 300, 400, 300), 0, (600, 200)), (-600, 300));
    }

    #[test]
    fn sheets_hang_under_announced_chrome() {
        assert_eq!(sheet((100, 122, 600, 400), 52, (200, 150)), (300, 174));
        // Never below the parent's bottom edge, never above its top.
        assert_eq!(sheet((100, 122, 600, 400), 900, (200, 150)), (300, 522));
        assert_eq!(sheet((100, 122, 600, 400), -5, (200, 150)), (300, 122));
    }

    #[test]
    fn moves_stay_below_the_top_of_the_work_area() {
        let work = (0, 22, 1280, 778);
        assert_eq!(constrain_move(work, (-300, 0)), (-300, 22));
        assert_eq!(constrain_move(work, (2000, 5000)), (2000, 799));
        assert_eq!(constrain_move(work, (10, 400)), (10, 400));
    }
}
