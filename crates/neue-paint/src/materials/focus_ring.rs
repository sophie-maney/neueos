//! The keyboard focus ring: a band of the accent's focus colour hugging a control's outline
//! from outside. Width from [`neue_theme::metrics::control::FOCUS_RING`], colour from
//! [`neue_theme::SemanticColors::focus_ring`].

use neue_theme::SemanticColors;
use neue_theme::metrics::control::FOCUS_RING;
use vello_common::kurbo::{RoundedRect, RoundedRectRadii, Stroke};

use crate::Variant;
use crate::scene::Scene;

/// Draw a focus ring around `outline` (the control's own shape, logical pixels). It
/// reaches [`FOCUS_RING`] outside it.
pub fn focus_ring(scene: &mut Scene, outline: RoundedRect, variant: impl Into<Variant>) {
    let color = SemanticColors::new(variant.into()).focus_ring;
    let half = FOCUS_RING / 2.0;
    let r = outline.radii();
    let grow = |v: f64| if v > 0.0 { v + half } else { 0.0 };
    let ring = RoundedRect::from_rect(
        outline.rect().inflate(half, half),
        RoundedRectRadii::new(
            grow(r.top_left),
            grow(r.top_right),
            grow(r.bottom_right),
            grow(r.bottom_left),
        ),
    );
    scene.stroke(&ring, &Stroke::new(FOCUS_RING), color);
}
