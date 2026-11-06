//! The on-screen display (task 3.14a): a rounded square of HUD material with a glyph (a
//! speaker or a sun) and a bar of [`STEPS`] steps showing a level.
//!
//! Colours and metrics come from [`neue_theme::materials::osd`].

use std::f64::consts::{FRAC_PI_4, PI};

use neue_theme::materials::osd::{
    BAR_BOTTOM, GLYPH_CENTER_Y, GLYPH_SIZE, Palette, RADIUS, STEP_GAP, STEP_HEIGHT, STEP_WIDTH,
    STEPS,
};
use vello_common::kurbo::{
    Arc, BezPath, Cap, Circle, Line, Point, Rect, RoundedRect, Stroke, Vec2,
};

use super::hud::hud_rounded;
use crate::scene::Scene;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::osd::{BOTTOM_MARGIN, FADE, HOLD, SIZE};

/// What the display shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// A speaker with up to three waves, more the louder it is.
    Volume,
    /// A speaker without waves, and every step unlit.
    Muted,
    /// A sun.
    Brightness,
}

/// Draw the display filling `rect` (logical pixels; meant to be [`SIZE`] square) with
/// `glyph` and `level` (0 to 1; quarter steps show).
pub fn osd(
    scene: &mut Scene,
    rect: Rect,
    glyph: Glyph,
    level: f64,
    variant: impl Into<Variant>,
    backdrop: Backdrop,
) {
    let variant = variant.into();
    let p = Palette::new(variant, backdrop);
    hud_rounded(scene, rect, RADIUS, variant, backdrop);
    let level = if level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let center = Point::new(rect.center().x, rect.y0 + GLYPH_CENTER_Y);
    match glyph {
        Glyph::Volume => speaker(scene, center, waves(level), p),
        Glyph::Muted => speaker(scene, center, 0, p),
        Glyph::Brightness => sun(scene, center, p),
    }
    let lit = if glyph == Glyph::Muted { 0.0 } else { level };
    steps(scene, rect, lit, p);
}

/// How many waves the speaker has at `level`: none when silent, then one per third.
pub fn waves(level: f64) -> u32 {
    if level <= 0.0 {
        0
    } else if level <= 1.0 / 3.0 {
        1
    } else if level <= 2.0 / 3.0 {
        2
    } else {
        3
    }
}

/// How much of step `index` is lit at `level`: 0, a quarter, a half, three quarters or 1.
pub fn step_fill(level: f64, index: u32) -> f64 {
    let quarters = (level.clamp(0.0, 1.0) * f64::from(STEPS) * 4.0).round();
    ((quarters - f64::from(index) * 4.0) / 4.0).clamp(0.0, 1.0)
}

fn steps(scene: &mut Scene, rect: Rect, level: f64, p: Palette) {
    let n = f64::from(STEPS);
    let width = n * STEP_WIDTH + (n - 1.0) * STEP_GAP;
    let x0 = rect.x0 + ((rect.width() - width) / 2.0).round();
    let y1 = rect.y1 - BAR_BOTTOM;
    for i in 0..STEPS {
        let x = x0 + f64::from(i) * (STEP_WIDTH + STEP_GAP);
        let step = Rect::new(x, y1 - STEP_HEIGHT, x + STEP_WIDTH, y1);
        scene.fill(&step, p.step_off);
        let fill = step_fill(level, i);
        if fill > 0.0 {
            let lit = Rect::new(x, step.y0, x + STEP_WIDTH * fill, y1);
            scene.fill(&lit, p.glyph);
        }
    }
}

/// A speaker facing right in the glyph box around `c`, with `waves` arcs.
fn speaker(scene: &mut Scene, c: Point, waves: u32, p: Palette) {
    let s = GLYPH_SIZE;
    let at = |x: f64, y: f64| c + Vec2::new(x * s, y * s);
    // The magnet: a small box; the cone flares from its right side.
    let body = RoundedRect::from_rect(
        Rect::from_points(at(-0.42, -0.12), at(-0.26, 0.12)),
        0.02 * s,
    );
    scene.fill(&body, p.glyph);
    let mut cone = BezPath::new();
    cone.move_to(at(-0.27, -0.12));
    cone.line_to(at(-0.04, -0.32));
    cone.line_to(at(-0.04, 0.32));
    cone.line_to(at(-0.27, 0.12));
    cone.close_path();
    scene.fill(&cone, p.glyph);
    let stroke = Stroke::new(0.06 * s).with_caps(Cap::Round);
    let origin = at(-0.04, 0.0);
    for i in 0..waves.min(3) {
        let radius = (0.16 + 0.12 * f64::from(i)) * s;
        let sweep = 0.30 * PI;
        let arc = Arc::new(origin, (radius, radius), -sweep, 2.0 * sweep, 0.0);
        scene.stroke(&arc, &stroke, p.glyph);
    }
}

/// A sun in the glyph box around `c`: a disc and eight rays.
fn sun(scene: &mut Scene, c: Point, p: Palette) {
    let s = GLYPH_SIZE;
    scene.fill(&Circle::new(c, 0.17 * s), p.glyph);
    let stroke = Stroke::new(0.07 * s).with_caps(Cap::Round);
    for i in 0..8 {
        let angle = f64::from(i) * FRAC_PI_4;
        let dir = Vec2::new(angle.cos(), angle.sin());
        let ray = Line::new(c + dir * (0.27 * s), c + dir * (0.39 * s));
        scene.stroke(&ray, &stroke, p.glyph);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn layers_are_balanced() {
        for glyph in [Glyph::Volume, Glyph::Muted, Glyph::Brightness] {
            let mut scene = Scene::new(SIZE, SIZE);
            let rect = Rect::new(0.0, 0.0, SIZE, SIZE);
            osd(
                &mut scene,
                rect,
                glyph,
                0.5,
                Appearance::Dark,
                Backdrop::Opaque,
            );
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn steps_light_in_quarters() {
        assert!((step_fill(0.5, 7) - 1.0).abs() < f64::EPSILON);
        assert!(step_fill(0.5, 8).abs() < f64::EPSILON);
        // A quarter of a step past the middle lights a quarter of step 8.
        assert!((step_fill(0.5 + 0.25 / 16.0, 8) - 0.25).abs() < 1e-9);
        assert!((step_fill(1.0, 15) - 1.0).abs() < f64::EPSILON);
        assert!(step_fill(0.0, 0).abs() < f64::EPSILON);
    }

    #[test]
    fn waves_follow_the_level() {
        assert_eq!(waves(0.0), 0);
        assert_eq!(waves(0.1), 1);
        assert_eq!(waves(0.5), 2);
        assert_eq!(waves(1.0), 3);
    }

    #[test]
    fn the_bar_fits_inside_the_square() {
        // Above the rounded corners, with a margin on each side.
        let n = f64::from(STEPS);
        const { assert!(BAR_BOTTOM >= RADIUS) };
        assert!(n * STEP_WIDTH + (n - 1.0) * STEP_GAP <= SIZE - 2.0 * 12.0);
    }
}
