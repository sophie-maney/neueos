//! The Aqua gel push button: neutral at rest, tinted by the accent colour when it is the
//! default button (pulsing) or held down.
//!
//! Built from vector layers only: a soft shadow and etched highlight below, a rim, a
//! four-stop gel body with a hard midline, and a gloss over the upper half. Colours and
//! metrics come from [`neue_theme::materials::aqua_button`].

use neue_theme::materials::aqua_button::{
    DISABLED_OPACITY, ETCH_OFFSET, GLOSS_INSET, GLOSS_TOP, Palette, RADIUS, RIM, SHADOW_BLUR,
    SHADOW_OFFSET,
};
use vello_common::kurbo::{Point, Rect, RoundedRect, RoundedRectRadii, Vec2};
use vello_common::peniko::{Color, Gradient};

use super::focus_ring;

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

pub(super) fn vertical(rect: Rect, stops: &[(f32, Color)]) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1)).with_stops(stops)
}

/// Everything that changes how a push button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PushButton {
    /// At rest or held down.
    pub state: ButtonState,
    /// The default button (Return presses it): accent gel at rest. Pass `false` while its
    /// window is inactive.
    pub default: bool,
    /// The default button's pulse, from 0 (rest) to 1 (brightest). Ignored for other
    /// buttons.
    pub pulse: f32,
    /// Greyed out: neutral gel at reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the button.
    pub focused: bool,
}

/// The outline of a push button filling `rect`: the shape focus rings follow.
pub fn push_button_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.height() / 2.0))
}

/// Draw a push button filling `rect` (logical pixels; the label is the caller's). The
/// focus ring reaches [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn push_button(scene: &mut Scene, rect: Rect, button: PushButton, variant: impl Into<Variant>) {
    let variant = variant.into();
    let palette = if button.disabled {
        Palette::plain(variant)
    } else if button.state == ButtonState::Pressed {
        Palette::new(variant, true)
    } else if button.default {
        Palette::new(variant, false).pulsed(button.pulse)
    } else {
        Palette::plain(variant)
    };
    if button.disabled {
        scene.push_opacity(DISABLED_OPACITY);
        draw(scene, rect, RADIUS.min(rect.height() / 2.0), &palette);
        scene.pop_layer();
    } else {
        draw(scene, rect, RADIUS.min(rect.height() / 2.0), &palette);
        if button.focused {
            focus_ring(scene, push_button_outline(rect), variant);
        }
    }
}

/// Draw an accent-coloured Aqua button filling `rect` (logical pixels;
/// [`neue_theme::metrics::control::REGULAR_HEIGHT`] tall is the regular size): the default
/// button at rest, or any button held down. `variant` accepts an [`crate::Appearance`] for
/// the default accent and contrast. [`push_button`] covers every other look.
pub fn aqua_button(scene: &mut Scene, rect: Rect, state: ButtonState, variant: impl Into<Variant>) {
    let radius = RADIUS.min(rect.height() / 2.0);
    draw(
        scene,
        rect,
        radius,
        &Palette::new(variant.into(), state == ButtonState::Pressed),
    );
}

/// The whole gel button in `rect` with corner `radius`: etch, shadow, rim, body and gloss.
/// Checkboxes, radio buttons and segmented controls reuse it.
pub(super) fn draw(scene: &mut Scene, rect: Rect, radius: f64, p: &Palette) {
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

    gel_body(scene, rect.inset(-RIM), (radius - RIM).max(0.0), p);
}

/// The gel inside the rim: body with a hard step at the midline, and the gloss over the
/// upper half, clipped to the body. `body_rect` is the rim's inner edge.
pub(super) fn gel_body(scene: &mut Scene, body_rect: Rect, radius: f64, p: &Palette) {
    let body = RoundedRect::from_rect(body_rect, radius);
    let [top, hi, lo, bottom] = p.gel;
    scene.fill(
        &body,
        vertical(
            body_rect,
            &[(0.0, top), (0.5, hi), (0.5, lo), (1.0, bottom)],
        ),
    );

    scene.push_clip(&body);
    let gloss_rect = Rect::new(
        body_rect.x0 + GLOSS_INSET,
        body_rect.y0 + GLOSS_TOP,
        body_rect.x1 - GLOSS_INSET,
        body_rect.center().y,
    );
    let gloss_radius = (radius - GLOSS_INSET).max(0.0);
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

    #[test]
    fn disabled_buttons_balance_layers_and_skip_the_focus_ring() {
        let rect = Rect::new(4.0, 4.0, 96.0, 26.0);
        let mut focused = Scene::new(100.0, 30.0);
        let button = PushButton {
            focused: true,
            ..PushButton::default()
        };
        push_button(&mut focused, rect, button, Appearance::Light);
        let mut disabled = Scene::new(100.0, 30.0);
        let button = PushButton {
            disabled: true,
            ..button
        };
        push_button(&mut disabled, rect, button, Appearance::Light);
        assert_eq!(disabled.open_layers(), 0);
        // Opacity layer push and pop replace the focus ring's single stroke.
        assert_eq!(
            disabled.commands().len(),
            focused.commands().len() + 1,
            "disabled: no ring, one opacity layer"
        );
    }
}
