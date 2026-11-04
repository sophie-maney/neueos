//! The tab view's look: a bordered box, and a strip of tabs over its top edge. The strip
//! is gel like the segmented control's, with the selected tab in accent gel (neutral and
//! a little darker while the window is inactive).
//!
//! Colours and metrics come from [`neue_theme::materials::tabs`], [`neue_theme::materials::
//! segmented`] and the push button's gel ([`neue_theme::materials::aqua_button`]).

use neue_theme::materials::aqua_button::Palette as Gel;
use neue_theme::materials::segmented::{DIVIDER, divider};
use neue_theme::materials::tabs::{BOX_RADIUS, INACTIVE_DIM, Palette};
use neue_theme::metrics::control::RIM;
use vello_common::kurbo::{Rect, RoundedRect, Stroke, Vec2};

use super::aqua_button::{draw, gel_body, vertical};
use super::focus_ring;
use super::segmented::{segment_edges, segmented_outline};
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how the tab strip looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStrip<'a> {
    /// Width of every tab, in order. They should add up to the rect's width.
    pub widths: &'a [f64],
    /// The selected tab.
    pub selected: usize,
    /// The tab held down by the pointer.
    pub pressed: Option<usize>,
    /// Whether the window is active: the selected tab is accent gel, else neutral.
    pub active: bool,
    /// Draws the keyboard focus ring around the strip.
    pub focused: bool,
}

/// The outline of a strip filling `rect`: the shape focus rings follow.
pub fn tab_strip_outline(rect: Rect) -> RoundedRect {
    segmented_outline(rect)
}

/// Draws the box filling `rect`: a faint fill, a border and an etched highlight below.
pub fn tab_box(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let outline = RoundedRect::from_rect(rect.inset(-0.5), BOX_RADIUS);
    scene.stroke(
        &(RoundedRect::from_rect(rect, BOX_RADIUS) + Vec2::new(0.0, 1.0)),
        &Stroke::new(1.0),
        p.etch,
    );
    scene.fill(&outline, p.fill);
    scene.stroke(&outline, &Stroke::new(1.0), p.border);
}

/// Draws the strip filling `rect` (logical pixels; the labels are the caller's).
pub fn tab_strip(scene: &mut Scene, rect: Rect, strip: TabStrip<'_>, variant: impl Into<Variant>) {
    let variant = variant.into();
    let n = strip.widths.len();
    if n == 0 {
        return;
    }
    let outline = tab_strip_outline(rect);
    let radius = outline.radii().top_left;
    let edges = segment_edges(rect, strip.widths);
    draw(scene, rect, radius, &Gel::plain(variant));

    scene.push_clip(&outline);
    let inner = rect.inset(-RIM);
    let selected = strip.selected.min(n - 1);

    if let Some(i) = strip.pressed.filter(|&i| i < n && i != selected) {
        let tab = Rect::new(edges[i], inner.y0, edges[i + 1], inner.y1);
        scene.fill(
            &tab,
            vello_common::peniko::Color::new([
                0.0,
                0.0,
                0.0,
                1.0 - neue_theme::materials::segmented::PRESSED_DIM,
            ]),
        );
    }

    let line = divider(variant);
    for &x in &edges[1..n] {
        let r = Rect::new(x - DIVIDER / 2.0, inner.y0, x + DIVIDER / 2.0, inner.y1);
        scene.fill(&r, line);
    }

    let highlight = Rect::new(edges[selected], rect.y0, edges[selected + 1], rect.y1);
    let gel = if strip.active {
        Gel::new(variant, false)
    } else {
        Gel::plain(variant).dimmed(INACTIVE_DIM)
    };
    scene.fill(
        &highlight,
        vertical(highlight, &[(0.0, gel.rim[0]), (1.0, gel.rim[1])]),
    );
    gel_body(scene, highlight.inset(-RIM), 0.0, &gel);
    scene.pop_layer();

    if strip.focused {
        focus_ring(scene, outline, variant);
    }
}
