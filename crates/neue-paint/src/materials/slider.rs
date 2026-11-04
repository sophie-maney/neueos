//! The slider: a recessed groove with a round gel knob that travels along it.
//!
//! Colours and metrics come from [`neue_theme::materials::slider`] and
//! [`neue_theme::materials::groove`]; the knob is the push button's neutral gel.

use neue_theme::materials::aqua_button::Palette;
use neue_theme::materials::slider::{DISABLED_OPACITY, KNOB, PRESSED_DIM, TRACK_HEIGHT};
use vello_common::kurbo::{Rect, RoundedRect};

use super::aqua_button::draw as draw_gel;
use super::focus_ring;
use super::groove;
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a slider looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slider {
    /// The knob's position, from 0 (left end) to 1 (right end).
    pub value: f32,
    /// The knob is held down.
    pub pressed: bool,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the knob.
    pub focused: bool,
}

/// The knob inside a slider filling `rect` (logical pixels) at `value` (0..1): also the
/// shape the focus ring follows.
pub fn slider_knob(rect: Rect, value: f64) -> RoundedRect {
    let travel = (rect.width() - KNOB).max(0.0);
    let x = rect.x0 + value.clamp(0.0, 1.0) * travel;
    let y = rect.center().y - KNOB / 2.0;
    RoundedRect::from_rect(Rect::new(x, y, x + KNOB, y + KNOB), KNOB / 2.0)
}

/// The slider value (0..1) whose knob is centred at `x`, in the same coordinates as
/// `rect`: what a pointer at `x` selects.
pub fn slider_value_at(rect: Rect, x: f64) -> f64 {
    let travel = rect.width() - KNOB;
    if travel <= 0.0 {
        return 0.0;
    }
    ((x - rect.x0 - KNOB / 2.0) / travel).clamp(0.0, 1.0)
}

/// Draw a slider filling `rect` (logical pixels; [`neue_theme::metrics::control::SLIDER_HEIGHT`]
/// high is the regular size). The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside the knob.
pub fn slider(scene: &mut Scene, rect: Rect, slider: Slider, variant: impl Into<Variant>) {
    let variant = variant.into();
    if slider.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    let y = rect.center().y - TRACK_HEIGHT / 2.0;
    groove::draw(
        scene,
        Rect::new(rect.x0, y, rect.x1, y + TRACK_HEIGHT),
        variant,
    );

    let knob = slider_knob(rect, f64::from(slider.value));
    let palette = if slider.pressed {
        Palette::plain(variant).dimmed(PRESSED_DIM)
    } else {
        Palette::plain(variant)
    };
    draw_gel(scene, knob.rect(), KNOB / 2.0, &palette);

    if slider.disabled {
        scene.pop_layer();
    } else if slider.focused {
        focus_ring(scene, knob, variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    fn rect() -> Rect {
        Rect::new(0.0, 0.0, 115.0, 19.0)
    }

    #[test]
    fn the_knob_travels_from_end_to_end() {
        assert_eq!(slider_knob(rect(), 0.0).rect().x0, 0.0);
        assert_eq!(slider_knob(rect(), 1.0).rect().x1, 115.0);
        assert_eq!(slider_knob(rect(), 0.5).rect().center().x, 57.5);
    }

    #[test]
    fn a_pointer_at_the_knob_centre_selects_that_value() {
        for v in [0.0, 0.25, 0.5, 1.0] {
            let x = slider_knob(rect(), v).rect().center().x;
            assert!((slider_value_at(rect(), x) - v).abs() < 1e-9);
        }
        assert_eq!(slider_value_at(rect(), -50.0), 0.0);
        assert_eq!(slider_value_at(rect(), 500.0), 1.0);
    }

    #[test]
    fn layers_are_balanced() {
        for disabled in [false, true] {
            let mut scene = Scene::new(120.0, 30.0);
            let s = Slider {
                value: 0.5,
                pressed: true,
                disabled,
                focused: true,
            };
            slider(&mut scene, rect(), s, Appearance::Dark);
            assert_eq!(scene.open_layers(), 0);
        }
    }
}
