//! The progress bar: a groove filled with accent gel. A bar without a known fraction is
//! filled all the way and shows the barber pole: diagonal stripes that move to the right.
//!
//! Colours and metrics come from [`neue_theme::materials::progress`],
//! [`neue_theme::materials::groove`] and the push button's accent gel.

use neue_theme::materials::aqua_button::{Palette, RIM};
use neue_theme::materials::progress::{DISABLED_OPACITY, STRIPE, STRIPE_PERIOD};
use vello_common::kurbo::{BezPath, Point, Rect, RoundedRect};

use super::aqua_button::{gel_body, vertical};
use super::groove;
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a progress bar looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    /// How much is done, from 0 to 1; `None` for the indeterminate barber pole.
    pub fraction: Option<f32>,
    /// How far the barber pole's stripes have moved, in stripe periods (only the fraction
    /// part counts).
    pub phase: f32,
    /// Greyed out: reduced opacity.
    pub disabled: bool,
}

/// Draw a progress bar filling `rect` (logical pixels; [`neue_theme::materials::progress::HEIGHT`]
/// high is the regular size).
pub fn progress_bar(scene: &mut Scene, rect: Rect, bar: Progress, variant: impl Into<Variant>) {
    let variant = variant.into();
    if bar.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    let (inner, inner_radius) = groove::draw(scene, rect, variant);

    // The filled part: all of it when indeterminate; at least a full pill otherwise, so a
    // little progress still shows.
    let fill_width = match bar.fraction {
        None => inner.width(),
        Some(f) if f <= 0.0 => 0.0,
        Some(f) => (inner.width() * f64::from(f.min(1.0))).max(inner.height()),
    };
    if fill_width > 0.0 {
        let fill = Rect::new(inner.x0, inner.y0, inner.x0 + fill_width, inner.y1);
        let accent = Palette::new(variant, false);
        let outline = RoundedRect::from_rect(fill, inner_radius);
        scene.fill(
            &outline,
            vertical(fill, &[(0.0, accent.rim[0]), (1.0, accent.rim[1])]),
        );
        gel_body(
            scene,
            fill.inset(-RIM),
            (inner_radius - RIM).max(0.0),
            &accent,
        );
        if bar.fraction.is_none() {
            scene.push_clip(&outline);
            stripes(scene, fill, bar.phase);
            scene.pop_layer();
        }
    }

    if bar.disabled {
        scene.pop_layer();
    }
}

/// The barber pole's light stripes across `fill`: parallelograms at 45°, half of every
/// period wide, moved right by `phase` periods.
fn stripes(scene: &mut Scene, fill: Rect, phase: f32) {
    let offset = f64::from(phase.rem_euclid(1.0)) * STRIPE_PERIOD;
    let slant = fill.height();
    let mut x = fill.x0 - STRIPE_PERIOD + offset - slant;
    while x < fill.x1 {
        let mut path = BezPath::new();
        path.move_to(Point::new(x, fill.y1));
        path.line_to(Point::new(x + STRIPE_PERIOD / 2.0, fill.y1));
        path.line_to(Point::new(x + STRIPE_PERIOD / 2.0 + slant, fill.y0));
        path.line_to(Point::new(x + slant, fill.y0));
        path.close_path();
        scene.fill(&path, STRIPE);
        x += STRIPE_PERIOD;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    fn rect() -> Rect {
        Rect::new(0.0, 0.0, 100.0, 12.0)
    }

    fn commands(bar: Progress) -> usize {
        let mut scene = Scene::new(110.0, 20.0);
        progress_bar(&mut scene, rect(), bar, Appearance::Light);
        assert_eq!(scene.open_layers(), 0);
        scene.commands().len()
    }

    #[test]
    fn only_indeterminate_bars_have_stripes() {
        let determinate = commands(Progress {
            fraction: Some(1.0),
            ..Progress::default()
        });
        let indeterminate = commands(Progress::default());
        assert!(indeterminate > determinate);
    }

    #[test]
    fn an_empty_bar_draws_no_fill() {
        let empty = commands(Progress {
            fraction: Some(0.0),
            ..Progress::default()
        });
        let some = commands(Progress {
            fraction: Some(0.01),
            ..Progress::default()
        });
        assert!(some > empty);
    }

    #[test]
    fn layers_are_balanced_when_disabled() {
        commands(Progress {
            disabled: true,
            ..Progress::default()
        });
    }
}
