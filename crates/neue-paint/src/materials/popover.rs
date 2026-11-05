//! Popovers (task 2.23a, ADR 0056): a rounded panel with an arrow on one side pointing at
//! the control it opened from, a soft drop shadow and a thin rim.
//!
//! A popover's box is its panel plus the arrow's strip on one side. It sits on an
//! [`Edge`] of its anchor (the control's rectangle) with the arrow on the facing side,
//! its tip [`GAP`] from the anchor and lined up with the anchor's centre where the panel's
//! corners allow. The geometry here is shared by the toolkit (which lays the popover out)
//! and the Wayland driver (which asks the compositor to place it, and reports back where
//! it landed).
//!
//! Colours and metrics come from [`neue_theme::materials::popover`].

use neue_theme::materials::popover::{
    ARROW_HEIGHT, ARROW_WIDTH, GAP, Palette, RADIUS, RIM, SHADOW_BLUR, SHADOW_OFFSET,
};
use vello_common::kurbo::{BezPath, Point, Rect, RoundedRect, Shape, Size, Stroke, Vec2};
use vello_common::peniko::{Color, Gradient};

use crate::Variant;
use crate::scene::Scene;

pub use neue_theme::materials::popover::SHADOW_REACH;

/// A side of a rectangle: where a popover sits relative to its anchor, or the side of a
/// popover its arrow is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Edge {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

impl Edge {
    /// The side across from this one.
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// Top and bottom run horizontally.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }
}

/// The panel of a popover whose box is `size` with its arrow on side `arrow`.
pub fn popover_body(size: Size, arrow: Edge) -> Rect {
    let r = Rect::from_origin_size(Point::ORIGIN, size);
    match arrow {
        Edge::Top => Rect {
            y0: ARROW_HEIGHT,
            ..r
        },
        Edge::Bottom => Rect {
            y1: size.height - ARROW_HEIGHT,
            ..r
        },
        Edge::Left => Rect {
            x0: ARROW_HEIGHT,
            ..r
        },
        Edge::Right => Rect {
            x1: size.width - ARROW_HEIGHT,
            ..r
        },
    }
}

/// The size of a popover box around a panel of size `body` with its arrow on side
/// `arrow`.
pub fn popover_size(body: Size, arrow: Edge) -> Size {
    if arrow.is_horizontal() {
        Size::new(body.width, body.height + ARROW_HEIGHT)
    } else {
        Size::new(body.width + ARROW_HEIGHT, body.height)
    }
}

/// Where a popover box of `size` goes on side `edge` of `anchor`: its top-left, with the
/// arrow's tip centred on that side of the anchor, [`GAP`] away.
pub fn popover_origin(anchor: Rect, edge: Edge, size: Size) -> Point {
    let c = anchor.center();
    match edge {
        Edge::Bottom => Point::new(c.x - size.width / 2.0, anchor.y1 + GAP),
        Edge::Top => Point::new(c.x - size.width / 2.0, anchor.y0 - GAP - size.height),
        Edge::Right => Point::new(anchor.x1 + GAP, c.y - size.height / 2.0),
        Edge::Left => Point::new(anchor.x0 - GAP - size.width, c.y - size.height / 2.0),
    }
}

/// Where a popover box of `size` at `origin` landed relative to `anchor`, having asked
/// for side `preferred`: the side it is on (the preferred one, or its opposite if the
/// compositor flipped it), and where its arrow's tip goes along the facing side, in box
/// coordinates, pointing at the anchor's centre as far as the corners allow.
pub fn popover_landing(anchor: Rect, preferred: Edge, size: Size, origin: Point) -> (Edge, f64) {
    let c = anchor.center();
    let mid = origin + size.to_vec2() / 2.0;
    let edge = if preferred.is_horizontal() {
        if mid.y >= c.y {
            Edge::Bottom
        } else {
            Edge::Top
        }
    } else if mid.x >= c.x {
        Edge::Right
    } else {
        Edge::Left
    };
    let (at, span) = if edge.is_horizontal() {
        (c.x - origin.x, size.width)
    } else {
        (c.y - origin.y, size.height)
    };
    (edge, clamp_arrow(at, span))
}

/// `at` kept far enough from both ends of a side `span` long for the arrow to clear the
/// corners; the middle if the side is too short.
fn clamp_arrow(at: f64, span: f64) -> f64 {
    let margin = RADIUS + ARROW_WIDTH / 2.0;
    if span < 2.0 * margin {
        span / 2.0
    } else {
        at.clamp(margin, span - margin)
    }
}

fn vertical(rect: Rect, top: Color, bottom: Color) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
        .with_stops([(0.0, top), (1.0, bottom)])
}

/// The outline of a popover box of `size`: the panel with the arrow on side `arrow`, its
/// tip at `at` along that side (box coordinates, kept clear of the corners).
pub fn popover_outline(size: Size, arrow: Edge, at: f64) -> BezPath {
    let body = popover_body(size, arrow);
    let span = if arrow.is_horizontal() {
        body.width()
    } else {
        body.height()
    };
    let at = clamp_arrow(at, span);
    let r = RADIUS.min(body.width().min(body.height()) / 2.0);
    let (w, h) = (ARROW_WIDTH / 2.0, ARROW_HEIGHT);
    let Rect { x0, y0, x1, y1 } = body;

    let mut path = BezPath::new();
    // Clockwise from the top-left corner's end, with the arrow spliced into its side.
    path.move_to((x0 + r, y0));
    if arrow == Edge::Top {
        path.line_to((at - w, y0));
        path.line_to((at, y0 - h));
        path.line_to((at + w, y0));
    }
    path.line_to((x1 - r, y0));
    corner(&mut path, (x1 - r, y0), (x1, y0), (x1, y0 + r));
    if arrow == Edge::Right {
        path.line_to((x1, at - w));
        path.line_to((x1 + h, at));
        path.line_to((x1, at + w));
    }
    path.line_to((x1, y1 - r));
    corner(&mut path, (x1, y1 - r), (x1, y1), (x1 - r, y1));
    if arrow == Edge::Bottom {
        path.line_to((at + w, y1));
        path.line_to((at, y1 + h));
        path.line_to((at - w, y1));
    }
    path.line_to((x0 + r, y1));
    corner(&mut path, (x0 + r, y1), (x0, y1), (x0, y1 - r));
    if arrow == Edge::Left {
        path.line_to((x0, at + w));
        path.line_to((x0 - h, at));
        path.line_to((x0, at - w));
    }
    path.line_to((x0, y0 + r));
    corner(&mut path, (x0, y0 + r), (x0, y0), (x0 + r, y0));
    path.close_path();
    path
}

/// A quarter circle from `from` to `to` around the corner `at` (a cubic approximation).
fn corner(path: &mut BezPath, from: (f64, f64), at: (f64, f64), to: (f64, f64)) {
    // Control points 0.5523 of the way to the corner make a circular quarter.
    const K: f64 = 0.552_284_75;
    let (from, at, to) = (Point::from(from), Point::from(at), Point::from(to));
    path.curve_to(from + (at - from) * K, to + (at - to) * K, to);
}

/// Draw a popover box of `size` at the origin (logical pixels): shadow, body, highlight
/// and rim, with the arrow on side `arrow` and its tip at `at` along that side. The shadow
/// reaches [`SHADOW_REACH`] beyond the box on every side, so leave that room.
pub fn popover(scene: &mut Scene, size: Size, arrow: Edge, at: f64, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let outline = popover_outline(size, arrow, at);
    let body = popover_body(size, arrow);
    let r = RADIUS.min(body.width().min(body.height()) / 2.0);

    // The shadow falls outside the popover only, so a translucent body (Phase 3) stays
    // clean: clip to the shadow's extent with the popover cut out.
    let mut frame = Rect::from_origin_size(Point::ORIGIN, size)
        .inflate(SHADOW_REACH, SHADOW_REACH)
        .to_path(0.1);
    frame.extend(outline.reverse_subpaths());
    scene.push_clip(&frame);
    scene.blurred_rounded_rect(
        body + Vec2::new(0.0, SHADOW_OFFSET),
        r,
        SHADOW_BLUR,
        p.shadow,
    );
    scene.pop_layer();

    let full = Rect::from_origin_size(Point::ORIGIN, size);
    scene.fill(&outline, vertical(full, p.fill[0], p.fill[1]));

    // Rim and highlight inside the outline: clip to it and stroke the outline at double
    // width, so only the inner half shows.
    scene.push_clip(&outline);
    let highlight = RoundedRect::from_rect(body.inset(-RIM), (r - RIM).max(0.0));
    let top = Rect::new(body.x0, body.y0, body.x1, body.y0 + r + 2.0 * RIM);
    scene.push_clip(&top);
    scene.stroke(&highlight, &Stroke::new(2.0 * RIM), p.highlight);
    scene.pop_layer();
    scene.stroke(&outline, &Stroke::new(2.0 * RIM), p.rim);
    scene.pop_layer();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    const ANCHOR: Rect = Rect::new(100.0, 100.0, 160.0, 122.0);
    const SIZE: Size = Size::new(200.0, 120.0);

    #[test]
    fn layers_are_balanced() {
        for arrow in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            let mut scene = Scene::new(300.0, 200.0);
            popover(&mut scene, SIZE, arrow, 40.0, Appearance::Light);
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn the_tip_touches_the_anchor_on_every_side() {
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            let origin = popover_origin(ANCHOR, edge, SIZE);
            let (landed, at) = popover_landing(ANCHOR, edge, SIZE, origin);
            assert_eq!(landed, edge);
            let arrow = edge.opposite();
            let tip = match arrow {
                Edge::Top => Point::new(origin.x + at, origin.y),
                Edge::Bottom => Point::new(origin.x + at, origin.y + SIZE.height),
                Edge::Left => Point::new(origin.x, origin.y + at),
                Edge::Right => Point::new(origin.x + SIZE.width, origin.y + at),
            };
            let expected = match edge {
                Edge::Bottom => Point::new(ANCHOR.center().x, ANCHOR.y1 + GAP),
                Edge::Top => Point::new(ANCHOR.center().x, ANCHOR.y0 - GAP),
                Edge::Right => Point::new(ANCHOR.x1 + GAP, ANCHOR.center().y),
                Edge::Left => Point::new(ANCHOR.x0 - GAP, ANCHOR.center().y),
            };
            assert!((tip - expected).hypot() < 1e-9, "{edge:?}: {tip:?}");
            // The tip is on the outline's bounding box.
            let bounds = popover_outline(SIZE, arrow, at).bounding_box();
            assert!(
                (bounds.size() - SIZE).to_vec2().hypot() < 0.5,
                "{edge:?}: {bounds:?}"
            );
        }
    }

    #[test]
    fn a_flipped_popover_points_back_at_its_anchor() {
        // Asked for below, placed above by the compositor.
        let origin = popover_origin(ANCHOR, Edge::Top, SIZE);
        let (edge, at) = popover_landing(ANCHOR, Edge::Bottom, SIZE, origin);
        assert_eq!(edge, Edge::Top);
        assert!((at - SIZE.width / 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_slid_popover_keeps_its_arrow_on_the_anchor_until_the_corner() {
        let origin = popover_origin(ANCHOR, Edge::Bottom, SIZE) + Vec2::new(30.0, 0.0);
        let (_, at) = popover_landing(ANCHOR, Edge::Bottom, SIZE, origin);
        assert!((at - (SIZE.width / 2.0 - 30.0)).abs() < 1e-9);
        // Slid so far that the anchor is past the corner: the arrow stops at the corner.
        let origin = Point::new(ANCHOR.center().x + 10.0, ANCHOR.y1 + GAP);
        let (_, at) = popover_landing(ANCHOR, Edge::Bottom, SIZE, origin);
        assert!((at - (RADIUS + ARROW_WIDTH / 2.0)).abs() < 1e-9);
    }

    #[test]
    fn body_and_size_round_trip() {
        for arrow in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            let body = popover_body(SIZE, arrow);
            assert_eq!(popover_size(body.size(), arrow), SIZE);
        }
    }
}
