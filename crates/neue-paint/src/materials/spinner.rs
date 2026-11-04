//! The spinner: twelve spokes in a ring, the newest dark and the rest fading behind it,
//! turning clockwise in twelve steps.
//!
//! Colours and metrics come from [`neue_theme::materials::spinner`].

use std::f64::consts::TAU;

use neue_theme::materials::spinner::{
    DISABLED_OPACITY, MIN_OPACITY, SPOKE_LENGTH, SPOKE_WIDTH, SPOKES, spoke,
};
use vello_common::kurbo::{Cap, Line, Point, Rect, Stroke};

use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a spinner looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spinner {
    /// How far round it has turned, in full turns (only the fraction part counts). The
    /// dark spoke is the one `phase` falls in.
    pub phase: f32,
    /// Greyed out: reduced opacity.
    pub disabled: bool,
}

/// Draw a spinner in the largest circle that fits `rect` (logical pixels;
/// [`neue_theme::metrics::control::SPINNER_SIZE`] square is the regular size).
pub fn spinner(scene: &mut Scene, rect: Rect, spinner: Spinner, variant: impl Into<Variant>) {
    let variant = variant.into();
    let diameter = rect.width().min(rect.height());
    let radius = diameter / 2.0;
    let centre = rect.center();
    let colour = spoke(variant);
    let stroke = Stroke::new(SPOKE_WIDTH * diameter).with_caps(Cap::Round);
    // The spoke the phase has reached; the ones behind it (counter-clockwise) fade.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a 0..SPOKES index"
    )]
    let head = (f64::from(spinner.phase.rem_euclid(1.0)) * SPOKES as f64) as usize % SPOKES;

    if spinner.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    for i in 0..SPOKES {
        let behind = (head + SPOKES - i) % SPOKES;
        #[expect(clippy::cast_precision_loss, reason = "tiny counts")]
        let fade = behind as f32 / (SPOKES - 1) as f32;
        let opacity = 1.0 - fade * (1.0 - MIN_OPACITY);
        #[expect(clippy::cast_precision_loss, reason = "tiny counts")]
        let angle = TAU * i as f64 / SPOKES as f64;
        // 0 is straight up; angles grow clockwise.
        let dir = (angle.sin(), -angle.cos());
        let at = |r: f64| Point::new(centre.x + dir.0 * r, centre.y + dir.1 * r);
        let inner = radius * (1.0 - SPOKE_LENGTH);
        scene.stroke(
            &Line::new(at(inner), at(radius - SPOKE_WIDTH * diameter / 2.0)),
            &stroke,
            colour.multiply_alpha(opacity),
        );
    }
    if spinner.disabled {
        scene.pop_layer();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    fn commands(phase: f32, disabled: bool) -> usize {
        let mut scene = Scene::new(16.0, 16.0);
        let s = Spinner { phase, disabled };
        spinner(
            &mut scene,
            Rect::new(0.0, 0.0, 16.0, 16.0),
            s,
            Appearance::Light,
        );
        assert_eq!(scene.open_layers(), 0);
        scene.commands().len()
    }

    #[test]
    fn draws_every_spoke_and_balances_layers() {
        assert_eq!(commands(0.0, false), SPOKES);
        assert_eq!(commands(0.37, true), SPOKES + 2);
    }

    #[test]
    fn it_turns_in_whole_steps() {
        let step = 1.0 / SPOKES as f32;
        // Anywhere inside one step draws the same spokes; the next step differs.
        let at = |phase: f32| {
            let mut scene = Scene::new(16.0, 16.0);
            let s = Spinner {
                phase,
                ..Spinner::default()
            };
            spinner(
                &mut scene,
                Rect::new(0.0, 0.0, 16.0, 16.0),
                s,
                Appearance::Light,
            );
            format!("{:?}", scene.commands())
        };
        assert_eq!(at(0.1 * step), at(0.9 * step));
        assert_ne!(at(0.1 * step), at(1.1 * step));
    }
}
