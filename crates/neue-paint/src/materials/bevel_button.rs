//! Bevel and recessed buttons, the flat buttons of toolbars and inspectors.
//!
//! The bevel button is a square-ish grey button that sinks in when pressed or on. The
//! recessed button has no chrome at rest and shows a dark pill with an inner shadow when
//! on. Colours and metrics come from [`neue_theme::materials::bevel_button`].

use neue_theme::materials::bevel_button::{
    DISABLED_OPACITY, ETCH_OFFSET, INNER_SHADOW, Palette, RADIUS, RIM, SHADOW_BLUR, SHADOW_OFFSET,
    recessed,
};
use vello_common::kurbo::{Rect, RoundedRect, Vec2};

use super::aqua_button::vertical;
use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a bevel button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BevelButton {
    /// How far the button is pressed in, from 0 (rest) to 1: held down, or on.
    pub depth: f32,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the button.
    pub focused: bool,
}

/// The outline of a bevel button filling `rect`: the shape focus rings follow.
pub fn bevel_button_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.height() / 2.0))
}

/// Draw a bevel button filling `rect` (logical pixels; the label is the caller's). The
/// focus ring reaches [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn bevel_button(
    scene: &mut Scene,
    rect: Rect,
    button: BevelButton,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    let p = Palette::new(variant, button.depth);
    let outline = bevel_button_outline(rect);
    let radius = outline.radii().top_left;
    if button.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }

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
    scene.fill(
        &outline,
        vertical(rect, &[(0.0, p.rim[0]), (1.0, p.rim[1])]),
    );
    let body_rect = rect.inset(-RIM);
    let body = RoundedRect::from_rect(body_rect, (radius - RIM).max(0.0));
    scene.fill(
        &body,
        vertical(body_rect, &[(0.0, p.body[0]), (1.0, p.body[1])]),
    );
    inner_shadow(scene, &body, body_rect, p.inner_shadow, INNER_SHADOW);

    if button.disabled {
        scene.pop_layer();
    } else if button.focused {
        focus_ring(scene, outline, variant);
    }
}

/// A shadow cast down from the top edge of `body`, fading out over `height`.
fn inner_shadow(
    scene: &mut Scene,
    body: &RoundedRect,
    body_rect: Rect,
    color: neue_theme::Color,
    height: f64,
) {
    if color.components[3] <= 0.0 {
        return;
    }
    scene.push_clip(body);
    let band = Rect::new(
        body_rect.x0,
        body_rect.y0,
        body_rect.x1,
        body_rect.y0 + height,
    );
    scene.fill(
        &band,
        vertical(band, &[(0.0, color), (1.0, color.with_alpha(0.0))]),
    );
    scene.pop_layer();
}

/// Everything that changes how a recessed button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecessedButton {
    /// How far on the button is, from 0 (plain text) to 1 (the dark pill): held down, or
    /// on. The caller draws the label in
    /// [`neue_theme::materials::bevel_button::recessed::Palette::label`].
    pub amount: f32,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the button.
    pub focused: bool,
}

/// The outline of a recessed button filling `rect`: a pill.
pub fn recessed_button_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, recessed::RADIUS.min(rect.height() / 2.0))
}

/// Draw a recessed button filling `rect` (logical pixels; the label is the caller's).
pub fn recessed_button(
    scene: &mut Scene,
    rect: Rect,
    button: RecessedButton,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    let p = recessed::Palette::new(variant, button.amount);
    let outline = recessed_button_outline(rect);
    let radius = outline.radii().top_left;
    if button.disabled {
        scene.push_opacity(recessed::DISABLED_OPACITY);
    }
    if button.amount > 0.0 {
        scene.fill(
            &RoundedRect::from_rect(rect + Vec2::new(0.0, ETCH_OFFSET), radius),
            p.etch,
        );
        scene.fill(
            &outline,
            vertical(rect, &[(0.0, p.rim[0]), (1.0, p.rim[1])]),
        );
        let body_rect = rect.inset(-recessed::RIM);
        let body = RoundedRect::from_rect(body_rect, (radius - recessed::RIM).max(0.0));
        scene.fill(
            &body,
            vertical(body_rect, &[(0.0, p.body[0]), (1.0, p.body[1])]),
        );
        inner_shadow(
            scene,
            &body,
            body_rect,
            p.inner_shadow,
            recessed::INNER_SHADOW,
        );
    }
    if button.disabled {
        scene.pop_layer();
    } else if button.focused {
        focus_ring(scene, outline, variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn layers_are_balanced() {
        let rect = Rect::new(4.0, 4.0, 60.0, 26.0);
        for disabled in [false, true] {
            let mut scene = Scene::new(70.0, 30.0);
            let bevel = BevelButton {
                depth: 0.5,
                disabled,
                focused: true,
            };
            bevel_button(&mut scene, rect, bevel, Appearance::Dark);
            let recessed = RecessedButton {
                amount: 1.0,
                disabled,
                focused: true,
            };
            recessed_button(&mut scene, rect, recessed, Appearance::Light);
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn a_recessed_button_at_rest_draws_nothing() {
        let mut scene = Scene::new(70.0, 30.0);
        recessed_button(
            &mut scene,
            Rect::new(4.0, 4.0, 60.0, 26.0),
            RecessedButton::default(),
            Appearance::Light,
        );
        assert!(scene.commands().is_empty());
    }
}
