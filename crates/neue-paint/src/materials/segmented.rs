//! The segmented control: segments in one rounded gel body. The selected segment is accent
//! gel, and moves between segments as the (fractional) selection does.
//!
//! Colours and metrics come from [`neue_theme::materials::segmented`] and the push
//! button's gel ([`neue_theme::materials::aqua_button`]).

use neue_theme::materials::aqua_button::Palette;
use neue_theme::materials::segmented::{DISABLED_OPACITY, DIVIDER, PRESSED_DIM, RADIUS, divider};
use neue_theme::metrics::control::RIM;
use vello_common::kurbo::{Rect, RoundedRect};
use vello_common::peniko::Color;

use super::aqua_button::{draw, gel_body, vertical};
use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a segmented control looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segmented<'a> {
    /// Width of every segment, in order. They should add up to the rect's width.
    pub widths: &'a [f64],
    /// The selected segment's index, fractional while the highlight moves between two.
    pub selection: f64,
    /// The segment held down by the pointer.
    pub pressed: Option<usize>,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the whole control.
    pub focused: bool,
}

/// The outline of a segmented control filling `rect`: the shape focus rings follow.
pub fn segmented_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.height() / 2.0))
}

/// The x positions of the segment boundaries inside `rect`: `widths.len() + 1` of them,
/// from the left edge to the right edge.
pub fn segment_edges(rect: Rect, widths: &[f64]) -> Vec<f64> {
    let mut x = rect.x0;
    let mut edges = vec![x];
    for w in widths {
        x += w;
        edges.push(x);
    }
    edges
}

/// Draw a segmented control filling `rect` (logical pixels; the labels are the caller's).
/// The focus ring reaches [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn segmented(
    scene: &mut Scene,
    rect: Rect,
    control: Segmented<'_>,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    let n = control.widths.len();
    if n == 0 {
        return;
    }
    let outline = segmented_outline(rect);
    let radius = outline.radii().top_left;
    let edges = segment_edges(rect, control.widths);

    if control.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    draw(scene, rect, radius, &Palette::plain(variant));

    // Everything else stays inside the control's outline.
    scene.push_clip(&outline);
    let inner = rect.inset(-RIM);

    // The segment held down, if it isn't the selected one: darkened.
    let sel = control.selection.clamp(0.0, (n - 1) as f64);
    if let Some(i) = control.pressed.filter(|&i| i < n) {
        #[expect(clippy::cast_precision_loss, reason = "segment counts are tiny")]
        let selected = (sel - i as f64).abs() < 0.5;
        if !selected {
            let seg = Rect::new(edges[i], inner.y0, edges[i + 1], inner.y1);
            scene.fill(&seg, Color::new([0.0, 0.0, 0.0, 1.0 - PRESSED_DIM]));
        }
    }

    // Dividers between segments.
    let divider_color = divider(variant);
    for &x in &edges[1..n] {
        let line = Rect::new(x - DIVIDER / 2.0, inner.y0, x + DIVIDER / 2.0, inner.y1);
        scene.fill(&line, divider_color);
    }

    // The highlight: interpolated between the two segments the selection sits between.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sel is clamped to 0..n-1"
    )]
    let (lo, hi) = (sel.floor() as usize, sel.ceil() as usize);
    let frac = sel - sel.floor();
    let lerp = |a: f64, b: f64| a + (b - a) * frac;
    let highlight = Rect::new(
        lerp(edges[lo], edges[hi]),
        rect.y0,
        lerp(edges[lo + 1], edges[hi + 1]),
        rect.y1,
    );
    let held = control.pressed.is_some_and(|i| {
        #[expect(clippy::cast_precision_loss, reason = "segment counts are tiny")]
        let near = (sel - i as f64).abs() < 0.5;
        near
    });
    let accent = Palette::new(variant, held);
    scene.fill(
        &highlight,
        vertical(highlight, &[(0.0, accent.rim[0]), (1.0, accent.rim[1])]),
    );
    gel_body(scene, highlight.inset(-RIM), 0.0, &accent);
    scene.pop_layer();

    if control.disabled {
        scene.pop_layer();
    } else if control.focused {
        focus_ring(scene, outline, variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn edges_accumulate_from_the_left() {
        let rect = Rect::new(4.0, 0.0, 124.0, 22.0);
        assert_eq!(
            segment_edges(rect, &[40.0, 50.0, 30.0]),
            [4.0, 44.0, 94.0, 124.0]
        );
    }

    #[test]
    fn layers_are_balanced_with_a_moving_highlight() {
        let rect = Rect::new(4.0, 4.0, 124.0, 26.0);
        for disabled in [false, true] {
            let mut scene = Scene::new(130.0, 30.0);
            let control = Segmented {
                widths: &[40.0, 50.0, 30.0],
                selection: 1.4,
                pressed: Some(2),
                disabled,
                focused: true,
            };
            segmented(&mut scene, rect, control, Appearance::Light);
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn no_segments_draws_nothing() {
        let mut scene = Scene::new(10.0, 10.0);
        let control = Segmented {
            widths: &[],
            selection: 0.0,
            pressed: None,
            disabled: false,
            focused: false,
        };
        segmented(
            &mut scene,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            control,
            Appearance::Dark,
        );
        assert!(scene.commands().is_empty());
    }
}
