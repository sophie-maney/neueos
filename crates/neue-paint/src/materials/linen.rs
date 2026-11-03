//! Procedural linen: a woven, tileable cloth texture.
//!
//! The tile is generated per device scale from integer hashing and smooth value noise, so it
//! is sharp at every scale and identical on every machine (only `+`, `*` and `floor`, no
//! transcendental functions).

use std::sync::Arc;

use vello_common::kurbo::{Affine, Shape};
use vello_common::peniko::ImageAlphaType;
use vello_common::pixmap::{PixelMetadata, Pixmap};

use crate::Appearance;
use crate::scene::{ProceduralTexture, Scene, TextureBrush};

/// Tile size in logical pixels. Every noise period below divides it, so the tile repeats
/// seamlessly.
const TILE: u16 = 128;

/// Colours of one linen variant, in sRGB 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LinenPalette {
    base: [f32; 3],
    /// How far fibres deviate from `base`.
    contrast: f32,
}

impl LinenPalette {
    // TODO(phase 2): move these into neue-theme tokens.
    fn for_appearance(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self {
                base: [0.785, 0.788, 0.780],
                contrast: 0.11,
            },
            Appearance::Dark => Self {
                base: [0.224, 0.236, 0.259],
                contrast: 0.075,
            },
        }
    }
}

/// The linen texture for one appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Linen {
    /// Light or dark variant.
    pub appearance: Appearance,
    /// Varies the weave pattern.
    pub seed: u32,
}

impl Linen {
    /// The default linen for `appearance`.
    pub fn new(appearance: Appearance) -> Self {
        Self {
            appearance,
            seed: 0x6e65_7565, // "neue"
        }
    }

    /// A brush that tiles this linen from the scene origin.
    pub fn brush(self) -> TextureBrush {
        TextureBrush {
            texture: Arc::new(self),
            transform: Affine::IDENTITY,
        }
    }

    /// Fibre value at logical position (u, v), roughly in -1..1.
    fn fibre(&self, u: f32, v: f32) -> f32 {
        let s = self.seed;
        let row = floor_u32(v) % u32::from(TILE);
        let col = floor_u32(u) % u32::from(TILE);
        let cells = |spacing: u16| u32::from(TILE / spacing);

        // Weft (horizontal) thread for this row: a per-thread tone plus slow and fast
        // variation along its length.
        let weft = 0.55 * hash1(row, s)
            + 0.30 * noise1(row, u / 16.0, cells(16), s ^ 0xa1)
            + 0.15 * noise1(row, u / 4.0, cells(4), s ^ 0xb2);
        // Warp (vertical) thread for this column.
        let warp = 0.55 * hash1(col, s ^ 0xc3)
            + 0.30 * noise1(col, v / 16.0, cells(16), s ^ 0xd4)
            + 0.15 * noise1(col, v / 4.0, cells(4), s ^ 0xe5);

        // Plain weave: at each crossing one thread lies on top.
        let over = if (row + col) & 1 == 0 { 0.7 } else { 0.3 };
        let woven = over * weft + (1.0 - over) * warp;

        // Thread profile: lighter at the centre of a thread, darker at its edges. At scale 1
        // samples sit on thread centres, so this only adds detail at higher scales.
        let profile = |t: f32| {
            let d = t - t.floor() - 0.5;
            1.0 - 4.0 * d * d
        };
        let shade = 0.5 * (profile(u) + profile(v));

        let grain = hash2(col, row, s ^ 0xf6);
        2.0 * (0.75 * woven + 0.25 * grain - 0.5) - 0.35 * (1.0 - shade)
    }
}

impl ProceduralTexture for Linen {
    fn cache_key(&self) -> u64 {
        let appearance = match self.appearance {
            Appearance::Light => 0,
            Appearance::Dark => 1,
        };
        (0x4c49_4e45_u64 << 32) ^ (u64::from(self.seed) << 1) ^ appearance
    }

    fn tile_size(&self) -> (u16, u16) {
        (TILE, TILE)
    }

    fn render(&self, scale: f64) -> Pixmap {
        let palette = LinenPalette::for_appearance(self.appearance);
        // Scales are small positive numbers; the tile is at most a few thousand pixels.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "see above"
        )]
        let size = (f64::from(TILE) * scale).round().max(1.0) as u16;
        #[allow(clippy::cast_possible_truncation, reason = "f64 → f32 for pixel maths")]
        let scale = scale as f32;
        let mut data = Vec::with_capacity(usize::from(size) * usize::from(size) * 4);
        for y in 0..size {
            for x in 0..size {
                let u = (f32::from(x) + 0.5) / scale;
                let v = (f32::from(y) + 0.5) / scale;
                let delta = palette.contrast * self.fibre(u, v);
                for channel in palette.base {
                    data.push(to_u8(channel + delta));
                }
                data.push(u8::MAX);
            }
        }
        Pixmap::from_parts(
            data,
            size,
            size,
            PixelMetadata {
                may_have_transparency: false,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
            },
        )
    }
}

/// Fill `shape` with linen anchored at the scene origin.
pub fn fill_linen(scene: &mut Scene, shape: &impl Shape, appearance: Appearance) {
    scene.fill(shape, Linen::new(appearance).brush());
}

fn to_u8(v: f32) -> u8 {
    // Clamped to 0..=255 first, so the cast cannot truncate.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped"
    )]
    let byte = (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
    byte
}

fn floor_u32(v: f32) -> u32 {
    // Only called with non-negative tile coordinates below 2^16.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "see above"
    )]
    let n = v.floor().max(0.0) as u32;
    n
}

/// Integer hash (lowbias32) of `x` and `seed` to 0..1.
fn hash1(x: u32, seed: u32) -> f32 {
    let mut h = x ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    // Top 24 bits are exactly representable in f32.
    #[allow(clippy::cast_precision_loss, reason = "24-bit value, exact in f32")]
    let unit = (h >> 8) as f32 / 16_777_216.0;
    unit
}

fn hash2(x: u32, y: u32, seed: u32) -> f32 {
    hash1(x ^ y.wrapping_mul(0x85eb_ca6b), seed)
}

/// Periodic 1D value noise along thread `line` at position `t` (in cells), repeating every
/// `period` cells.
fn noise1(line: u32, t: f32, period: u32, seed: u32) -> f32 {
    let cell = floor_u32(t);
    let f = t - t.floor();
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash2(cell % period, line, seed);
    let b = hash2((cell + 1) % period, line, seed);
    a + (b - a) * s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_is_seamless_at_scale_1_and_2() {
        let linen = Linen::new(Appearance::Light);
        for scale in [1.0_f32, 2.0] {
            let period = f32::from(TILE);
            for i in 0..8 {
                let x = (i as f32 + 0.5) / scale;
                let y = (i as f32 * 7.0 + 0.5) / scale;
                assert_eq!(linen.fibre(x, y), linen.fibre(x + period, y));
                assert_eq!(linen.fibre(x, y), linen.fibre(x, y + period));
            }
        }
    }

    #[test]
    fn tile_size_follows_scale_and_is_opaque() {
        let pixmap = Linen::new(Appearance::Dark).render(2.0);
        assert_eq!((pixmap.width(), pixmap.height()), (256, 256));
        assert!(!pixmap.may_have_transparency());
    }

    #[test]
    fn variants_have_distinct_cache_keys() {
        assert_ne!(
            Linen::new(Appearance::Light).cache_key(),
            Linen::new(Appearance::Dark).cache_key()
        );
    }
}
