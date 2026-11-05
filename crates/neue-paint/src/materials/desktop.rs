//! The desktop's background (task 3.2c): a default picture drawn from vectors, and a photo
//! the user chose, scaled to the output.
//!
//! The default picture ([`default_picture`]) is a sky with a glow and three ranges of
//! hills, in a light and a dark variant. A [`Picture`] holds decoded pixels (the shell
//! decodes PNG and JPEG; this crate only draws them) and [`picture`] paints it onto a
//! rectangle under a [`Fit`]. Pictures are resampled for the device scale they are drawn at
//! (as the linen texture is generated), so they are sharp at every scale.
//!
//! Colours and metrics come from [`neue_theme::materials::desktop`].

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use neue_theme::materials::desktop::{GLOW_CENTER, GLOW_RADIUS, HILLS, Palette};
use vello_common::kurbo::{Affine, BezPath, Point, Rect};
use vello_common::peniko::{Color, Gradient, ImageAlphaType};
use vello_common::pixmap::{PixelMetadata, Pixmap};

use crate::Variant;
use crate::scene::{ProceduralTexture, Scene, TextureBrush};

/// How a [`Picture`] is placed on the output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    /// Scaled to cover the whole output, keeping its proportions; the overflow is cut off
    /// equally on both sides.
    #[default]
    Fill,
    /// Scaled to fit inside the output, keeping its proportions; the rest shows black.
    Fit,
    /// Scaled to the output's size, whatever its proportions.
    Stretch,
    /// At its own size (one picture pixel per logical pixel), in the middle.
    Center,
}

impl Fit {
    /// Where a picture of `size` (width, height) lands in `area`.
    pub fn place(self, size: (f64, f64), area: Rect) -> Rect {
        let (w, h) = size;
        if w <= 0.0 || h <= 0.0 {
            return area;
        }
        let scale = match self {
            Self::Stretch => return area,
            Self::Fill => (area.width() / w).max(area.height() / h),
            Self::Fit => (area.width() / w).min(area.height() / h),
            Self::Center => 1.0,
        };
        let (w, h) = (w * scale, h * scale);
        let c = area.center();
        Rect::new(c.x - w / 2.0, c.y - h / 2.0, c.x + w / 2.0, c.y + h / 2.0)
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Decoded pixels of a photo. Cheap to clone (the pixels are shared).
#[derive(Clone)]
pub struct Picture {
    width: u32,
    height: u32,
    /// Premultiplied RGBA8.
    pixels: Arc<[u8]>,
    opaque: bool,
    /// Tells the backends' texture caches this picture from every other.
    id: u64,
}

impl fmt::Debug for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Picture")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Picture {
    /// A picture from straight-alpha RGBA8 pixels, row by row from the top left. `None` if
    /// a side is zero, a side is over 65535 or `rgba` has the wrong length.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
        let len = usize::try_from(u64::from(width) * u64::from(height) * 4).ok()?;
        if width == 0 || height == 0 || width > 65535 || height > 65535 || rgba.len() != len {
            return None;
        }
        let mut pixels = rgba.to_vec();
        let mut opaque = true;
        for px in pixels.as_chunks_mut::<4>().0 {
            let a = u16::from(px[3]);
            if a != 255 {
                opaque = false;
                for c in &mut px[..3] {
                    // (c * a + 127) / 255 is at most 255.
                    *c = ((u16::from(*c) * a + 127) / 255) as u8;
                }
            }
        }
        Some(Self {
            width,
            height,
            pixels: pixels.into(),
            opaque,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The premultiplied pixel at (`x`, `y`) as floats, clamped to the edges.
    fn texel(&self, x: i64, y: i64) -> [f32; 4] {
        let x = x.clamp(0, i64::from(self.width) - 1);
        let y = y.clamp(0, i64::from(self.height) - 1);
        // Both are in range after clamping, so the index is too.
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        let p = &self.pixels[i..i + 4];
        [
            f32::from(p[0]),
            f32::from(p[1]),
            f32::from(p[2]),
            f32::from(p[3]),
        ]
    }

    /// The picture's colour at (`u`, `v`) pixels (bilinear, pixel centres at +0.5),
    /// averaged over a `radius`-pixel box when it is shrunk a lot.
    fn sample(&self, u: f64, v: f64, radius: i64) -> [f32; 4] {
        if radius <= 1 {
            let (x, y) = (u - 0.5, v - 0.5);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (a, b) = (self.texel(x0, y0), self.texel(x0 + 1, y0));
            let (c, d) = (self.texel(x0, y0 + 1), self.texel(x0 + 1, y0 + 1));
            let mut out = [0.0; 4];
            for i in 0..4 {
                let top = a[i] + (b[i] - a[i]) * fx;
                let bottom = c[i] + (d[i] - c[i]) * fx;
                out[i] = top + (bottom - top) * fy;
            }
            return out;
        }
        // A box of radius × radius pixels centred on the sample.
        let (x0, y0) = (u as i64 - radius / 2, v as i64 - radius / 2);
        let mut sum = [0.0_f32; 4];
        for y in y0..y0 + radius {
            for x in x0..x0 + radius {
                let t = self.texel(x, y);
                for i in 0..4 {
                    sum[i] += t[i];
                }
            }
        }
        let n = (radius * radius) as f32;
        sum.map(|s| s / n)
    }
}

/// A [`Picture`] placed on an output of a given size: what the backends cache and draw.
struct PictureTexture {
    picture: Picture,
    fit: Fit,
    /// The output's logical size.
    size: (u16, u16),
    letterbox: Color,
}

impl ProceduralTexture for PictureTexture {
    fn cache_key(&self) -> u64 {
        let fit = match self.fit {
            Fit::Fill => 0_u64,
            Fit::Fit => 1,
            Fit::Stretch => 2,
            Fit::Center => 3,
        };
        // The picture's id is unique per decoded picture; the rest places it.
        (self.picture.id << 40)
            ^ (fit << 32)
            ^ (u64::from(self.size.0) << 16)
            ^ u64::from(self.size.1)
    }

    fn tile_size(&self) -> (u16, u16) {
        self.size
    }

    fn render(&self, scale: f64) -> Pixmap {
        // Output sizes are at most 65535 logical pixels; scales are small.
        let to_px = |logical: u16| (f64::from(logical) * scale).round().clamp(1.0, 65535.0) as u16;
        let (w, h) = (to_px(self.size.0), to_px(self.size.1));
        let area = Rect::new(0.0, 0.0, f64::from(w), f64::from(h));
        let pic = &self.picture;
        let src = (f64::from(pic.width), f64::from(pic.height));
        // `Center` is one picture pixel per logical pixel, so `scale` device pixels.
        let placed = if self.fit == Fit::Center {
            let (pw, ph) = (src.0 * scale, src.1 * scale);
            let c = area.center();
            Rect::new(
                c.x - pw / 2.0,
                c.y - ph / 2.0,
                c.x + pw / 2.0,
                c.y + ph / 2.0,
            )
        } else {
            self.fit.place(src, area)
        };
        let step = src.0 / placed.width();
        let radius = step.floor().max(1.0) as i64;
        let bg = self.letterbox.premultiply().to_rgba8();
        let bg = [bg.r, bg.g, bg.b, bg.a];

        let mut data = Vec::with_capacity(usize::from(w) * usize::from(h) * 4);
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
                if px < placed.x0 || px >= placed.x1 || py < placed.y0 || py >= placed.y1 {
                    data.extend_from_slice(&bg);
                    continue;
                }
                let u = (px - placed.x0) / placed.width() * src.0;
                let v = (py - placed.y0) / placed.height() * src.1;
                let c = pic.sample(u, v, radius);
                // A picture with holes shows the letterbox colour through them.
                let behind = (255.0 - c[3]) / 255.0;
                for i in 0..4 {
                    let value = c[i] + f32::from(bg[i]) * behind;
                    data.push((value + 0.5).clamp(0.0, 255.0) as u8);
                }
            }
        }
        Pixmap::from_parts(
            data,
            w,
            h,
            PixelMetadata {
                // Whatever the picture's holes, the letterbox is opaque behind them.
                may_have_transparency: self.letterbox.to_rgba8().a != 255 && !pic.opaque,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
            },
        )
    }
}

/// Draw `picture` placed by `fit` in `rect`, which must start at whole logical pixels and
/// have a whole size (an output does). The rest of `rect` is black.
pub fn picture(
    scene: &mut Scene,
    rect: Rect,
    picture: &Picture,
    fit: Fit,
    variant: impl Into<Variant>,
) {
    let palette = Palette::new(variant.into());
    // `rect` is an output's size: positive and whole, below 65536.
    let size = (rect.width().round() as u16, rect.height().round() as u16);
    let brush = TextureBrush {
        texture: Arc::new(PictureTexture {
            picture: picture.clone(),
            fit,
            size,
            letterbox: palette.letterbox,
        }),
        transform: Affine::translate((rect.x0, rect.y0)),
    };
    scene.fill(&rect, brush);
}

/// Draw the default picture filling `rect`.
pub fn default_picture(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let (w, h) = (rect.width(), rect.height());
    let at = |fx: f64, fy: f64| Point::new(rect.x0 + fx * w, rect.y0 + fy * h);

    // Sky: from the top down to the horizon (the back hills' ridge).
    let sky = Gradient::new_linear(at(0.0, 0.0), at(0.0, 0.7)).with_stops([
        (0.0, p.sky[0]),
        (0.55, p.sky[1]),
        (1.0, p.sky[2]),
    ]);
    scene.fill(&rect, sky);

    // The glow: opaque in the middle, gone at its edge.
    let (cx, cy) = GLOW_CENTER;
    let glow = Gradient::new_radial(at(cx, cy), (GLOW_RADIUS * w) as f32).with_stops([
        (0.0, p.glow),
        (0.12, p.glow),
        (1.0, p.glow.with_alpha(0.0)),
    ]);
    scene.fill(&rect, glow);

    // Hills, back to front.
    for (ridge, color) in HILLS.iter().zip(p.hills) {
        scene.fill(&hills(rect, ridge), color);
    }
}

/// A ridge through `ridge`'s five heights, filled down to the bottom of `rect`.
fn hills(rect: Rect, ridge: &[f64; 5]) -> BezPath {
    let (w, h) = (rect.width(), rect.height());
    let point = |i: usize| {
        Point::new(
            rect.x0 + w * i as f64 / (ridge.len() - 1) as f64,
            rect.y0 + h * ridge[i],
        )
    };
    let mut path = BezPath::new();
    path.move_to(Point::new(rect.x0, rect.y1));
    path.line_to(point(0));
    // Quadratic curves through the points' midpoints, with the points as control points:
    // smooth, and never overshooting the ridge.
    for i in 1..ridge.len() - 1 {
        path.quad_to(point(i), point(i).midpoint(point(i + 1)));
    }
    path.line_to(point(ridge.len() - 1));
    path.line_to(Point::new(rect.x1, rect.y1));
    path.close_path();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_covers_the_area_and_fit_stays_inside() {
        let area = Rect::new(0.0, 0.0, 200.0, 100.0);
        let fill = Fit::Fill.place((100.0, 100.0), area);
        assert_eq!(fill, Rect::new(0.0, -50.0, 200.0, 150.0));
        let fit = Fit::Fit.place((100.0, 100.0), area);
        assert_eq!(fit, Rect::new(50.0, 0.0, 150.0, 100.0));
        assert_eq!(Fit::Stretch.place((1.0, 7.0), area), area);
        assert_eq!(
            Fit::Center.place((40.0, 20.0), area),
            Rect::new(80.0, 40.0, 120.0, 60.0)
        );
    }

    #[test]
    fn pictures_reject_bad_data() {
        assert!(Picture::from_rgba8(0, 1, &[]).is_none());
        assert!(Picture::from_rgba8(1, 1, &[0; 3]).is_none());
        assert!(Picture::from_rgba8(1, 1, &[1, 2, 3, 255]).is_some());
    }

    #[test]
    fn a_stretched_picture_renders_its_pixels() {
        // Left half red, right half blue, drawn at twice the size.
        let mut rgba = Vec::new();
        for _ in 0..2 {
            rgba.extend([255, 0, 0, 255, 0, 0, 255, 255]);
        }
        let pic = Picture::from_rgba8(2, 2, &rgba).expect("valid picture");
        let tex = PictureTexture {
            picture: pic,
            fit: Fit::Stretch,
            size: (4, 4),
            letterbox: Color::from_rgba8(0, 0, 0, 255),
        };
        let pixmap = tex.render(1.0);
        let data = pixmap.data_as_u8_slice();
        assert_eq!(&data[0..4], &[255, 0, 0, 255]);
        assert_eq!(&data[12..16], &[0, 0, 255, 255]);
    }

    #[test]
    fn a_narrow_picture_leaves_black_bars() {
        let pic = Picture::from_rgba8(1, 1, &[255, 255, 255, 255]).expect("valid picture");
        let tex = PictureTexture {
            picture: pic,
            fit: Fit::Fit,
            size: (4, 2),
            letterbox: Color::from_rgba8(0, 0, 0, 255),
        };
        let pixmap = tex.render(1.0);
        let data = pixmap.data_as_u8_slice();
        // Row 0: black, white, white, black.
        assert_eq!(&data[0..4], &[0, 0, 0, 255]);
        assert_eq!(&data[4..8], &[255, 255, 255, 255]);
        assert_eq!(&data[12..16], &[0, 0, 0, 255]);
    }
}
