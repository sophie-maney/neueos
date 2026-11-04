//! The groove of sliders and the trough of progress bars: a recessed channel.
//!
//! Colours come from [`neue_theme::materials::groove`].

use neue_theme::materials::groove::{ETCH_OFFSET, Palette, RIM};
use vello_common::kurbo::{Rect, RoundedRect, Vec2};

use super::aqua_button::vertical;
use crate::Variant;
use crate::scene::Scene;

/// Draws the groove filling `rect` (fully rounded ends) and returns the inside, where a
/// fill goes, and its corner radius. The etched highlight sits [`ETCH_OFFSET`] below.
pub(super) fn draw(scene: &mut Scene, rect: Rect, variant: Variant) -> (Rect, f64) {
    let p = Palette::new(variant);
    let radius = rect.height() / 2.0;
    scene.fill(
        &RoundedRect::from_rect(rect + Vec2::new(0.0, ETCH_OFFSET), radius),
        p.etch,
    );
    scene.fill(
        &RoundedRect::from_rect(rect, radius),
        vertical(rect, &[(0.0, p.rim[0]), (1.0, p.rim[1])]),
    );
    let inner = rect.inset(-RIM);
    let inner_radius = (radius - RIM).max(0.0);
    scene.fill(
        &RoundedRect::from_rect(inner, inner_radius),
        vertical(inner, &[(0.0, p.fill[0]), (1.0, p.fill[1])]),
    );
    (inner, inner_radius)
}
