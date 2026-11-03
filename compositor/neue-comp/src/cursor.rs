//! Pointer images for compositor-drawn cursors (the tty backend): XCursor themes through
//! the pure-Rust `xcursor` crate, and a built-in arrow as the fallback (ADR 0012).
//!
//! [`Themes`] resolves a CSS cursor name ([`CursorIcon`], from `cursor-shape-v1` or a
//! compositor action such as a resize) to an image at the physical size an output needs,
//! caches it, and steps through animated cursors. Sizes are `XCURSOR_SIZE` (default 24)
//! times the output scale, so a scale-2 output gets the 48 px images and stays sharp; the
//! element is then drawn 1:1 in physical pixels.
//!
//! The built-in arrow is a polygon rasterised with an analytic signed distance, so edges
//! are antialiased at any size: a dark fill with a light outline, readable on any
//! background. It is original artwork.

use std::collections::HashMap;
use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::input::pointer::CursorIcon;
use smithay::utils::{Logical, Physical, Point, Size, Transform};
use tracing::{debug, warn};

/// Width and height of the built-in arrow at scale 1, in pixels.
pub const SIZE: i32 = 24;
/// Smallest and largest cursor size accepted from `XCURSOR_SIZE`, in pixels.
const SIZE_RANGE: (u32, u32) = (8, 256);

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

/// The arrow as an `Argb8888` buffer of `size`×`size` pixels, premultiplied, little-endian
/// (`B, G, R, A` bytes). The artwork is drawn on a [`SIZE`] grid and scaled to fit.
pub fn pixels(size: i32) -> Vec<u8> {
    let k = size as f32 / SIZE as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let d = signed_distance((x as f32 + 0.5) / k, (y as f32 + 0.5) / k) * k;
            let fill = (0.5 - d).clamp(0.0, 1.0);
            let alpha = (0.5 - (d - OUTLINE * k)).clamp(0.0, 1.0);
            // Premultiplied: fill over edge, both opaque, under a common coverage `alpha`.
            let edge = alpha - fill;
            let channel = |i: usize| to_byte(FILL[i] * fill + EDGE[i] * edge);
            out.extend_from_slice(&[channel(2), channel(1), channel(0), to_byte(alpha)]);
        }
    }
    out
}

/// One cursor image, ready to draw 1:1 in physical pixels.
#[derive(Debug)]
pub struct Frame {
    pub buffer: MemoryRenderBuffer,
    /// Where the pointer's tip is inside the image, in pixels.
    pub hotspot: Point<i32, Physical>,
    /// Image size in pixels.
    pub size: Size<i32, Physical>,
}

impl Frame {
    /// Image size in logical pixels at `scale`: what to hand the render element so it
    /// covers exactly `size` physical pixels.
    pub fn logical_size(&self, scale: f64) -> Size<i32, Logical> {
        Size::from((
            (f64::from(self.size.w) / scale).round() as i32,
            (f64::from(self.size.h) / scale).round() as i32,
        ))
    }
}

/// An image or an animation: frames with the time (ms) each stays up.
#[derive(Debug)]
struct Animation {
    frames: Vec<(Frame, u32)>,
    total_ms: u32,
}

impl Animation {
    fn still(frame: Frame) -> Self {
        Self {
            frames: vec![(frame, 0)],
            total_ms: 0,
        }
    }

    fn at(&self, elapsed: Duration) -> &Frame {
        if self.total_ms == 0 {
            return &self.frames[0].0;
        }
        let mut t = (elapsed.as_millis() % u128::from(self.total_ms)) as u32;
        for (frame, delay) in &self.frames {
            if t < *delay {
                return frame;
            }
            t -= delay;
        }
        &self.frames[0].0
    }
}

/// An XCursor theme with a cache of decoded cursors, per icon and pixel size.
#[derive(Debug)]
pub struct Themes {
    theme: xcursor::CursorTheme,
    /// Nominal size at scale 1 (`XCURSOR_SIZE`).
    base_size: u32,
    cache: HashMap<(CursorIcon, u32), Animation>,
}

impl Themes {
    /// The theme named by `XCURSOR_THEME` (default `default`) at `XCURSOR_SIZE` (default 24).
    pub fn from_env() -> Self {
        let name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".into());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(SIZE as u32);
        Self::new(&name, size)
    }

    pub fn new(theme: &str, base_size: u32) -> Self {
        Self {
            theme: xcursor::CursorTheme::load(theme),
            base_size: base_size.clamp(SIZE_RANGE.0, SIZE_RANGE.1),
            cache: HashMap::new(),
        }
    }

    /// Pixel size for an output at `scale`.
    fn pixel_size(&self, scale: f64) -> u32 {
        ((f64::from(self.base_size) * scale).round() as u32).clamp(SIZE_RANGE.0, SIZE_RANGE.1)
    }

    /// The image for `icon` on an output at `scale`, `elapsed` into the session (only
    /// animated cursors depend on it). Falls back to the built-in arrow when the theme
    /// has no such cursor.
    pub fn frame(&mut self, icon: CursorIcon, scale: f64, elapsed: Duration) -> &Frame {
        let size = self.pixel_size(scale);
        if !self.cache.contains_key(&(icon, size)) {
            let animation = self.load(icon, size).unwrap_or_else(|| {
                debug!(
                    icon = icon.name(),
                    "no theme cursor, using the built-in arrow"
                );
                Animation::still(builtin(size as i32))
            });
            self.cache.insert((icon, size), animation);
        }
        self.cache[&(icon, size)].at(elapsed)
    }

    /// Load `icon` from the theme (trying the standard name, then the legacy aliases).
    fn load(&self, icon: CursorIcon, size: u32) -> Option<Animation> {
        let path = std::iter::once(icon.name())
            .chain(icon.alt_names().iter().copied())
            .find_map(|name| self.theme.load_icon(name))?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                warn!(?path, %err, "reading a cursor file failed");
                return None;
            }
        };
        let images = xcursor::parser::parse_xcursor(&bytes)?;
        animation_from(&images, size)
    }
}

/// The frames of `images` whose nominal size is closest to `size`.
fn animation_from(images: &[xcursor::parser::Image], size: u32) -> Option<Animation> {
    let nearest = images
        .iter()
        .min_by_key(|image| image.size.abs_diff(size))?
        .size;
    let frames: Vec<(Frame, u32)> = images
        .iter()
        .filter(|image| image.size == nearest)
        .map(|image| {
            let frame = Frame {
                // `pixels_rgba` is R, G, B, A bytes in memory: `Abgr8888` in DRM terms.
                buffer: MemoryRenderBuffer::from_slice(
                    &image.pixels_rgba,
                    Fourcc::Abgr8888,
                    (image.width as i32, image.height as i32),
                    1,
                    Transform::Normal,
                    None,
                ),
                hotspot: Point::from((image.xhot as i32, image.yhot as i32)),
                size: Size::from((image.width as i32, image.height as i32)),
            };
            (frame, image.delay)
        })
        .collect();
    // A single frame is never animated, whatever its delay says.
    let total_ms = if frames.len() > 1 {
        frames.iter().map(|(_, delay)| delay).sum()
    } else {
        0
    };
    Some(Animation { frames, total_ms })
}

/// The built-in arrow at `size`×`size` pixels.
fn builtin(size: i32) -> Frame {
    let k = size as f32 / SIZE as f32;
    Frame {
        buffer: MemoryRenderBuffer::from_slice(
            &pixels(size),
            Fourcc::Argb8888,
            (size, size),
            1,
            Transform::Normal,
            None,
        ),
        hotspot: Point::from((
            (ARROW[0].0 * k).round() as i32,
            (ARROW[0].1 * k).round() as i32,
        )),
        size: Size::from((size, size)),
    }
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

    fn pixel(pixels: &[u8], size: i32, x: i32, y: i32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    #[test]
    fn buffer_has_the_right_size() {
        assert_eq!(pixels(SIZE).len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(pixels(48).len(), 48 * 48 * 4);
    }

    #[test]
    fn inside_is_opaque_dark() {
        let [b, g, r, a] = pixel(&pixels(SIZE), SIZE, 4, 10);
        assert_eq!(a, 255);
        assert!(r < 40 && g < 40 && b < 40, "got {r} {g} {b}");
    }

    #[test]
    fn outline_is_light() {
        // Just left of the arrow's left edge (x = 2).
        let [b, g, r, a] = pixel(&pixels(SIZE), SIZE, 1, 10);
        assert!(a > 200, "alpha {a}");
        assert!(r > 200 && g > 200 && b > 200, "got {r} {g} {b}");
    }

    #[test]
    fn far_corner_is_transparent() {
        assert_eq!(pixel(&pixels(SIZE), SIZE, SIZE - 1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn colour_never_exceeds_alpha() {
        // Premultiplied alpha invariant, at scale 1 and scale 1.5.
        for size in [SIZE, 36] {
            for chunk in pixels(size).as_chunks::<4>().0 {
                assert!(chunk[..3].iter().all(|&c| c <= chunk[3]), "{chunk:?}");
            }
        }
    }

    #[test]
    fn hotspot_scales_with_the_image() {
        assert_eq!(builtin(SIZE).hotspot, Point::from((2, 2)));
        assert_eq!(builtin(48).hotspot, Point::from((4, 4)));
    }

    fn image(size: u32, delay: u32) -> xcursor::parser::Image {
        xcursor::parser::Image {
            size,
            width: size,
            height: size,
            xhot: 1,
            yhot: 2,
            delay,
            pixels_rgba: vec![0; (size * size * 4) as usize],
            pixels_argb: vec![0; (size * size * 4) as usize],
        }
    }

    #[test]
    fn picks_the_nearest_nominal_size() {
        let images = [image(24, 0), image(32, 0), image(48, 0)];
        let animation = animation_from(&images, 36).unwrap();
        assert_eq!(animation.frames[0].0.size, Size::from((32, 32)));
        let animation = animation_from(&images, 100).unwrap();
        assert_eq!(animation.frames[0].0.size, Size::from((48, 48)));
    }

    #[test]
    fn animation_steps_through_frames_and_loops() {
        let images = [image(24, 100), image(24, 300)];
        let animation = animation_from(&images, 24).unwrap();
        assert_eq!(animation.total_ms, 400);
        let first = animation.at(Duration::from_millis(50)) as *const Frame;
        let second = animation.at(Duration::from_millis(150)) as *const Frame;
        let looped = animation.at(Duration::from_millis(450)) as *const Frame;
        assert!(!std::ptr::eq(first, second));
        assert!(std::ptr::eq(first, looped));
    }

    #[test]
    fn a_single_frame_is_still() {
        let animation = animation_from(&[image(24, 50)], 24).unwrap();
        assert_eq!(animation.total_ms, 0);
    }

    #[test]
    fn missing_theme_falls_back_to_the_arrow() {
        let mut themes = Themes::new("neue-no-such-theme", 24);
        let frame = themes.frame(CursorIcon::Text, 2.0, Duration::ZERO);
        assert_eq!(frame.size, Size::from((48, 48)));
        assert_eq!(frame.hotspot, Point::from((4, 4)));
    }

    #[test]
    fn size_comes_from_base_and_scale() {
        let themes = Themes::new("neue-no-such-theme", 24);
        assert_eq!(themes.pixel_size(1.0), 24);
        assert_eq!(themes.pixel_size(1.5), 36);
        assert_eq!(themes.pixel_size(2.0), 48);
        assert_eq!(Themes::new("x", 1000).pixel_size(1.0), SIZE_RANGE.1);
    }

    #[test]
    fn logical_size_covers_the_physical_pixels() {
        assert_eq!(builtin(36).logical_size(1.5), Size::from((24, 24)));
        assert_eq!(builtin(48).logical_size(2.0), Size::from((24, 24)));
    }
}
