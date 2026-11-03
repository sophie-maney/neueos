//! Window placement: new windows cascade down and to the right, like a stack of papers.
//!
//! Pure geometry so it can be tested without a Wayland server. Floating window management
//! proper (centring, remembering positions, avoiding the menu bar) comes in task 1.5.

/// Offset between consecutive windows in the cascade, in logical pixels.
pub const CASCADE_STEP: i32 = 24;
/// Distance of the first window from the output's top-left corner, in logical pixels.
pub const CASCADE_ORIGIN: i32 = 32;

/// A rectangle in logical coordinates: `(x, y, width, height)`.
pub type Rect = (i32, i32, i32, i32);

/// Position of the `index`-th window (0-based) in the cascade on `output`.
///
/// The cascade restarts at the origin, shifted right by one step per restart, once a
/// window of `size` would extend past the bottom or right edge of the output. Windows
/// larger than the output are placed at the origin.
pub fn cascade(output: Rect, size: (i32, i32), index: usize) -> (i32, i32) {
    let (ox, oy, ow, oh) = output;
    let (w, h) = size;
    let step = CASCADE_STEP;

    // How many windows fit before one would cross an edge.
    let room_x = ow - CASCADE_ORIGIN - w;
    let room_y = oh - CASCADE_ORIGIN - h;
    if room_x < 0 || room_y < 0 {
        return (ox + CASCADE_ORIGIN.min(ow.saturating_sub(w).max(0)), oy);
    }
    let per_column = usize::try_from(room_y / step).unwrap_or(0) + 1;
    let column = index / per_column;
    let row = index % per_column;

    // `row` and `column` are bounded by the output size divided by the step, so they fit
    // in an i32. Fall back to 0 rather than panic if that ever changes.
    let row = i32::try_from(row).unwrap_or(0);
    let column = i32::try_from(column).unwrap_or(0);
    let x = (CASCADE_ORIGIN + row * step + column * step * 2).min(CASCADE_ORIGIN + room_x);
    let y = CASCADE_ORIGIN + row * step;
    (ox + x, oy + y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: Rect = (0, 0, 1280, 800);

    #[test]
    fn first_window_at_origin() {
        assert_eq!(
            cascade(OUTPUT, (640, 480), 0),
            (CASCADE_ORIGIN, CASCADE_ORIGIN)
        );
    }

    #[test]
    fn windows_cascade_down_and_right() {
        let a = cascade(OUTPUT, (640, 480), 0);
        let b = cascade(OUTPUT, (640, 480), 1);
        assert_eq!((b.0 - a.0, b.1 - a.1), (CASCADE_STEP, CASCADE_STEP));
    }

    #[test]
    fn cascade_wraps_and_stays_on_screen() {
        let size = (640, 480);
        for i in 0..200 {
            let (x, y) = cascade(OUTPUT, size, i);
            assert!(x >= 0 && x + size.0 <= OUTPUT.2, "x out of bounds at {i}");
            assert!(y >= 0 && y + size.1 <= OUTPUT.3, "y out of bounds at {i}");
        }
        // After wrapping, the cascade restarts at the top.
        let per_column = (800 - CASCADE_ORIGIN - 480) / CASCADE_STEP + 1;
        let wrapped = cascade(OUTPUT, size, per_column as usize);
        assert_eq!(wrapped.1, CASCADE_ORIGIN);
    }

    #[test]
    fn respects_output_offset() {
        let (x, y) = cascade((1280, 100, 1280, 800), (640, 480), 0);
        assert_eq!((x, y), (1280 + CASCADE_ORIGIN, 100 + CASCADE_ORIGIN));
    }

    #[test]
    fn oversized_window_goes_to_top_left() {
        assert_eq!(cascade(OUTPUT, (2000, 2000), 3), (0, 0));
    }
}
