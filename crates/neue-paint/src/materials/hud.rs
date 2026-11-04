//! The dark HUD panel: a rounded, mostly opaque black panel with a gloss over its upper part,
//! a thin light rim and a soft drop shadow. Translucent over a blurred backdrop, or an
//! opaque tint without one.
//!
//! Colours and metrics come from [`neue_theme::materials::hud`].

use neue_theme::materials::hud::{GLOSS_HEIGHT, Palette, RADIUS, RIM, SHADOW_BLUR, SHADOW_OFFSET};
use vello_common::kurbo::{Point, Rect, RoundedRect, Shape, Stroke, Vec2};
use vello_common::peniko::{Color, Gradient};

use crate::scene::Scene;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::hud::SHADOW_REACH;

fn vertical(rect: Rect, top: Color, bottom: Color) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
        .with_stops([(0.0, top), (1.0, bottom)])
}

/// The outline of a panel filling `rect`.
pub fn hud_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.width().min(rect.height()) / 2.0))
}

/// Draw a HUD panel filling `rect` (logical pixels). The shadow reaches
/// [`SHADOW_REACH`] beyond the sides and top and a little more below, so leave that room.
/// Content (text, glyphs) is the caller's, in [`Palette::label`] colours.
pub fn hud(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>, backdrop: Backdrop) {
    let p = Palette::new(variant.into(), backdrop);
    let outline = hud_outline(rect);

    // The shadow falls outside the panel only: it must not darken the translucent body.
    // The clip is a frame, the shadow's extent with the panel cut out.
    let mut frame = rect
        .inflate(SHADOW_REACH, SHADOW_REACH + SHADOW_OFFSET)
        .to_path(0.1);
    let hole = outline.to_path(0.1).reverse_subpaths();
    frame.extend(hole);
    scene.push_clip(&frame);
    scene.blurred_rounded_rect(
        rect + Vec2::new(0.0, SHADOW_OFFSET),
        outline.radii().top_left,
        SHADOW_BLUR,
        p.shadow,
    );
    scene.pop_layer();

    scene.fill(&outline, vertical(rect, p.fill[0], p.fill[1]));

    scene.push_clip(&outline);
    let gloss = Rect::new(
        rect.x0,
        rect.y0,
        rect.x1,
        rect.y0 + rect.height() * GLOSS_HEIGHT,
    );
    scene.fill(&gloss, vertical(gloss, p.gloss[0], p.gloss[1]));
    scene.pop_layer();

    let rim = RoundedRect::from_rect(
        rect.inset(-RIM / 2.0),
        (outline.radii().top_left - RIM / 2.0).max(0.0),
    );
    scene.stroke(&rim, &Stroke::new(RIM), p.rim);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn layers_are_balanced() {
        let mut scene = Scene::new(200.0, 200.0);
        hud(
            &mut scene,
            Rect::new(20.0, 20.0, 180.0, 180.0),
            Appearance::Dark,
            Backdrop::Translucent,
        );
        assert_eq!(scene.open_layers(), 0);
    }

    #[test]
    fn small_panels_clamp_the_radius() {
        let outline = hud_outline(Rect::new(0.0, 0.0, 10.0, 6.0));
        assert_eq!(outline.radii().top_left, 3.0);
    }
}
