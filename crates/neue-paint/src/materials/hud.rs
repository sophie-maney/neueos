//! The dark HUD panel: a rounded, mostly opaque black panel with a gloss over its upper part,
//! a thin light rim and a soft drop shadow. Translucent over a blurred backdrop, or an
//! opaque tint without one.
//!
//! Colours and metrics come from [`neue_theme::materials::hud`].

use neue_theme::materials::hud::{
    CLOSE_INSET, CLOSE_SIZE, GLOSS_HEIGHT, Palette, RADIUS, RIM, SHADOW_BLUR, SHADOW_OFFSET,
    TITLE_HEIGHT,
};
use vello_common::kurbo::{Cap, Circle, Line, Point, Rect, RoundedRect, Shape, Stroke, Vec2};
use vello_common::peniko::{Color, Gradient};

use crate::scene::Scene;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::hud::{SHADOW_REACH, TITLE_HEIGHT as HUD_TITLE_HEIGHT};

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

/// Where a HUD window's close button is, in the window's coordinates: a disc in the title
/// bar's left corner, centred on the bar.
pub fn close_rect() -> Rect {
    let top = (TITLE_HEIGHT - CLOSE_SIZE) / 2.0;
    Rect::new(CLOSE_INSET, top, CLOSE_INSET + CLOSE_SIZE, top + CLOSE_SIZE)
}

/// What the pointer does to a HUD window's close button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CloseState {
    /// The pointer is over the button: the cross shows.
    pub hover: bool,
    /// The pointer went down on the button and is still over it.
    pub pressed: bool,
}

/// Draw a HUD window's close button at [`close_rect`]: a disc, with a cross in it while the
/// pointer is over it.
pub fn hud_close_button(scene: &mut Scene, variant: impl Into<Variant>, state: CloseState) {
    // Only the label colour and the disc depend on the backdrop's tint, and not by much.
    let p = Palette::new(variant.into(), Backdrop::Opaque);
    let rect = close_rect();
    let disc = Circle::new(rect.center(), CLOSE_SIZE / 2.0);
    scene.fill(&disc, p.close[usize::from(state.pressed)]);
    scene.stroke(&disc, &Stroke::new(RIM), p.rim);
    if state.hover || state.pressed {
        let arm = CLOSE_SIZE * 0.2;
        let c = rect.center();
        let stroke = Stroke::new(1.5).with_caps(Cap::Round);
        scene.stroke(
            &Line::new((c.x - arm, c.y - arm), (c.x + arm, c.y + arm)),
            &stroke,
            p.label,
        );
        scene.stroke(
            &Line::new((c.x - arm, c.y + arm), (c.x + arm, c.y - arm)),
            &stroke,
            p.label,
        );
    }
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
    fn close_button_sits_inside_the_title_bar() {
        let rect = close_rect();
        assert!(rect.y0 >= 0.0 && rect.y1 <= TITLE_HEIGHT);
        assert_eq!(rect.y0, TITLE_HEIGHT - rect.y1);
    }

    #[test]
    fn close_button_layers_are_balanced() {
        let mut scene = Scene::new(60.0, 20.0);
        hud_close_button(
            &mut scene,
            Appearance::Light,
            CloseState {
                hover: true,
                pressed: true,
            },
        );
        assert_eq!(scene.open_layers(), 0);
    }

    #[test]
    fn small_panels_clamp_the_radius() {
        let outline = hud_outline(Rect::new(0.0, 0.0, 10.0, 6.0));
        assert_eq!(outline.radii().top_left, 3.0);
    }
}
