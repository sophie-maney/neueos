//! The Aqua gel push button (the default button, tinted by the accent colour).
//!
//! Built from vector layers only: a soft shadow and etched highlight below, a rim, a
//! four-stop gel body with a hard midline, and a gloss over the upper half. Colours and
//! metrics come from [`neue_theme::materials::aqua_button`].

use neue_theme::materials::aqua_button::{
    ETCH_OFFSET, GLOSS_INSET, GLOSS_TOP, Palette, RADIUS, RIM, SHADOW_BLUR, SHADOW_OFFSET,
};
use vello_common::kurbo::{Point, Rect, RoundedRect, RoundedRectRadii, Vec2};
use vello_common::peniko::{Color, Gradient};

use crate::Variant;
use crate::scene::Scene;

/// Interaction state of a push button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonState {
    /// At rest.
    #[default]
    Normal,
    /// Held down.
    Pressed,
}

fn vertical(rect: Rect, stops: &[(f32, Color)]) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1)).with_stops(stops)
}

/// Draw an Aqua push button filling `rect` (logical pixels;
/// [`neue_theme::metrics::control::REGULAR_HEIGHT`] tall is the regular size). `variant`
/// accepts an [`crate::Appearance`] for the default accent and contrast.
///
/// The button has no label yet: text rendering arrives with neue-kit.
pub fn aqua_button(scene: &mut Scene, rect: Rect, state: ButtonState, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), state == ButtonState::Pressed);
    let radius = RADIUS.min(rect.height() / 2.0);

    // Etched highlight below the button, then the drop shadow.
    scene.fill(
        &RoundedRect::from_rect(rect + Vec2::new(0.0, ETCH_OFFSET), radius),
        p.etch,
    );
    scene.blurred_rounded_rect(
        rect + Vec2::new(0.0, SHADOW_OFFSET),
        radius,
        SHADOW_BLUR,
        p.shadow,
    );

    // Rim.
    let rim = RoundedRect::from_rect(rect, radius);
    scene.fill(&rim, vertical(rect, &[(0.0, p.rim[0]), (1.0, p.rim[1])]));

    // Gel body with a hard step at the midline.
    let body_rect = rect.inset(-RIM);
    let body = RoundedRect::from_rect(body_rect, (radius - RIM).max(0.0));
    let [top, hi, lo, bottom] = p.gel;
    scene.fill(
        &body,
        vertical(
            body_rect,
            &[(0.0, top), (0.5, hi), (0.5, lo), (1.0, bottom)],
        ),
    );

    // Gloss over the upper half, clipped to the body.
    scene.push_clip(&body);
    let gloss_rect = Rect::new(
        body_rect.x0 + GLOSS_INSET,
        body_rect.y0 + GLOSS_TOP,
        body_rect.x1 - GLOSS_INSET,
        body_rect.center().y,
    );
    let gloss_radius = (radius - RIM - GLOSS_INSET).max(0.0);
    let gloss = RoundedRect::from_rect(
        gloss_rect,
        RoundedRectRadii::new(gloss_radius, gloss_radius, 0.0, 0.0),
    );
    scene.fill(
        &gloss,
        vertical(gloss_rect, &[(0.0, p.gloss[0]), (1.0, p.gloss[1])]),
    );
    scene.pop_layer();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;
    use crate::scene::Command;

    #[test]
    fn layers_are_balanced() {
        let mut scene = Scene::new(100.0, 30.0);
        aqua_button(
            &mut scene,
            Rect::new(4.0, 4.0, 96.0, 26.0),
            ButtonState::Pressed,
            Appearance::Dark,
        );
        assert_eq!(scene.open_layers(), 0);
        assert!(matches!(scene.commands().last(), Some(Command::PopLayer)));
    }
}
