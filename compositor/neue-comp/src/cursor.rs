//! The built-in pointer: a procedurally drawn arrow (original artwork), used by the tty
//! backend until XCursor themes and cursor-shape-v1 arrive (task 1.9), and as the fallback
//! when no theme is installed.
//!
//! The arrow is a polygon rasterised with an analytic signed distance, so edges are
//! antialiased: a dark fill with a light outline, readable on any background.

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::{Logical, Point, Transform};

/// Width and height of the cursor image, in pixels (scale 1).
pub const SIZE: i32 = 24;
/// Where the arrow's tip is inside the image.
pub const HOTSPOT: Point<i32, Logical> = Point::new(2, 2);

/// Outline polygon of the arrow, in image pixels; the tip is first.
const ARROW: [(f32, f32); 7] = [
    (2.0, 2.0),
    (2.0, 18.0),
    (6.0, 14.5),
    (9.0, 21.0),
    (11.5, 20.0),
    (8.5, 13.5),
    (13.5, 13.5),
];
/// Width of the light outline around the fill.
const OUTLINE: f32 = 1.4;
/// Fill and outline colours (straight RGB). The cursor is chrome, not a widget; theme
/// tokens reach the compositor with task 1.10.
const FILL: [f32; 3] = [0.08, 0.08, 0.09];
const EDGE: [f32; 3] = [1.0, 1.0, 1.0];

/// The arrow as an `Argb8888` buffer, premultiplied, little-endian (`B, G, R, A` bytes).
pub fn pixels() -> Vec<u8> {
    let mut out = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = signed_distance(x as f32 + 0.5, y as f32 + 0.5);
            let fill = (0.5 - d).clamp(0.0, 1.0);
            let alpha = (0.5 - (d - OUTLINE)).clamp(0.0, 1.0);
            // Premultiplied: fill over edge, both opaque, under a common coverage `alpha`.
            let edge = alpha - fill;
            let channel = |i: usize| to_byte(FILL[i] * fill + EDGE[i] * edge);
            out.extend_from_slice(&[channel(2), channel(1), channel(0), to_byte(alpha)]);
        }
    }
    out
}

/// The arrow ready for rendering at scale 1.
pub fn default_cursor() -> MemoryRenderBuffer {
    MemoryRenderBuffer::from_slice(
        &pixels(),
        Fourcc::Argb8888,
        (SIZE, SIZE),
        1,
        Transform::Normal,
        None,
    )
}

fn to_byte(value: f32) -> u8 {
    // Clamped to 0..=255 first, so the cast cannot truncate.
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Distance from `(px, py)` to the arrow's outline: negative inside, positive outside.
fn signed_distance(px: f32, py: f32) -> f32 {
    let mut distance = f32::MAX;
    let mut inside = false;
    for i in 0..ARROW.len() {
        let (ax, ay) = ARROW[i];
        let (bx, by) = ARROW[(i + 1) % ARROW.len()];
        // Distance to the segment a–b.
        let (ex, ey) = (bx - ax, by - ay);
        let t = (((px - ax) * ex + (py - ay) * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        let (dx, dy) = (px - (ax + ex * t), py - (ay + ey * t));
        distance = distance.min((dx * dx + dy * dy).sqrt());
        // Even-odd crossing test.
        if (ay > py) != (by > py) && px < ax + (py - ay) / (by - ay) * ex {
            inside = !inside;
        }
    }
    if inside { -distance } else { distance }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], x: i32, y: i32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    #[test]
    fn buffer_has_the_right_size() {
        assert_eq!(pixels().len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn inside_is_opaque_dark() {
        let [b, g, r, a] = pixel(&pixels(), 4, 10);
        assert_eq!(a, 255);
        assert!(r < 40 && g < 40 && b < 40, "got {r} {g} {b}");
    }

    #[test]
    fn outline_is_light() {
        // Just left of the arrow's left edge (x = 2).
        let [b, g, r, a] = pixel(&pixels(), 1, 10);
        assert!(a > 200, "alpha {a}");
        assert!(r > 200 && g > 200 && b > 200, "got {r} {g} {b}");
    }

    #[test]
    fn far_corner_is_transparent() {
        assert_eq!(pixel(&pixels(), SIZE - 1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn colour_never_exceeds_alpha() {
        // Premultiplied alpha invariant.
        for chunk in pixels().as_chunks::<4>().0 {
            assert!(chunk[..3].iter().all(|&c| c <= chunk[3]), "{chunk:?}");
        }
    }

    #[test]
    fn hotspot_is_the_tip() {
        let (x, y) = ARROW[0];
        assert_eq!(HOTSPOT, Point::new(x as i32, y as i32));
    }
}
