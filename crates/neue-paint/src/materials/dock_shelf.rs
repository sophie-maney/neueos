//! The dock shelf: a pane of glass. In perspective it is a trapezoid whose far edge is
//! narrower than its near edge, as when the Dock lies on the bottom edge of the screen;
//! flat it is a rounded rectangle for docks on the left and right edges (the caller
//! rotates the scene with [`Scene::with_transform`] for those).
//!
//! Layers: the glass gradient, the icons' reflections in it ([`dock_shelf_with`]), a sheen
//! over its far part, highlights along the far edge and the sides, and a dark line along the
//! near edge. Translucent over a blurred backdrop, or an opaque tint without one. Colours
//! and metrics come from [`neue_theme::materials::dock_shelf`].
//!
//! Also here: the running indicator that glows on the shelf under an app's icon, and the
//! tile shown for an app without an icon.

use neue_theme::materials::dock_shelf::{
    EDGE, FLAT_RADIUS, INDICATOR, INDICATOR_GLOW, PERSPECTIVE_INSET, PLACEHOLDER_RADIUS, Palette,
    REFLECTION_SCALE, SHEEN_HEIGHT,
};
use vello_common::kurbo::{
    Affine, BezPath, Ellipse, Line, Point, Rect, RoundedRect, Shape, Stroke,
};
use vello_common::peniko::Gradient;

use crate::scene::Scene;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::dock_shelf::DEPTH;

/// How the shelf is seen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShelfStyle {
    /// A trapezoid, narrower at its far (top) edge: the Dock on the bottom edge.
    #[default]
    Perspective,
    /// A rounded rectangle: the Dock on the left or right edge.
    Flat,
}

/// The outline of the shelf filling `rect`.
fn outline(rect: Rect, style: ShelfStyle) -> BezPath {
    match style {
        ShelfStyle::Perspective => {
            // A short shelf keeps a flat far edge.
            let inset = PERSPECTIVE_INSET.min(rect.width() / 4.0);
            let mut path = BezPath::new();
            path.move_to((rect.x0 + inset, rect.y0));
            path.line_to((rect.x1 - inset, rect.y0));
            path.line_to((rect.x1, rect.y1));
            path.line_to((rect.x0, rect.y1));
            path.close_path();
            path
        }
        ShelfStyle::Flat => {
            RoundedRect::from_rect(rect, FLAT_RADIUS.min(rect.height() / 2.0)).to_path(0.1)
        }
    }
}

/// Draw the shelf filling `rect` ([`DEPTH`] tall in perspective).
pub fn dock_shelf(
    scene: &mut Scene,
    rect: Rect,
    style: ShelfStyle,
    variant: impl Into<Variant>,
    backdrop: Backdrop,
) {
    dock_shelf_with(scene, rect, style, variant, backdrop, |_| {});
}

/// Draw the shelf filling `rect`, with what `reflections` draws seen in the glass: cut to
/// the shelf, faint, and under its sheen and edges. Draw each icon there through
/// [`reflected`]. Nothing is reflected under increased contrast.
pub fn dock_shelf_with(
    scene: &mut Scene,
    rect: Rect,
    style: ShelfStyle,
    variant: impl Into<Variant>,
    backdrop: Backdrop,
    reflections: impl FnOnce(&mut Scene),
) {
    let p = Palette::new(variant.into(), backdrop);
    let body = outline(rect, style);
    let vertical = |y0: f64, y1: f64, stops: [vello_common::peniko::Color; 2]| {
        Gradient::new_linear(Point::new(0.0, y0), Point::new(0.0, y1))
            .with_stops([(0.0, stops[0]), (1.0, stops[1])])
    };
    scene.fill(&body, vertical(rect.y0, rect.y1, p.fill));

    // Everything else stays inside the body, so edge lines are drawn twice as wide and only
    // their inner half shows.
    scene.push_clip(&body);
    if p.reflection > 0.0 {
        scene.push_opacity(p.reflection);
        reflections(scene);
        scene.pop_layer();
    }
    let sheen_end = rect.y0 + rect.height() * SHEEN_HEIGHT;
    scene.fill(
        &Rect::new(rect.x0, rect.y0, rect.x1, sheen_end),
        vertical(rect.y0, sheen_end, p.sheen),
    );
    let edge = Stroke::new(EDGE * 2.0);
    match style {
        ShelfStyle::Perspective => {
            let inset = PERSPECTIVE_INSET.min(rect.width() / 4.0);
            scene.stroke(
                &Line::new((rect.x0 + inset, rect.y0), (rect.x0, rect.y1)),
                &edge,
                p.side_edge,
            );
            scene.stroke(
                &Line::new((rect.x1 - inset, rect.y0), (rect.x1, rect.y1)),
                &edge,
                p.side_edge,
            );
            scene.stroke(
                &Line::new((rect.x0 + inset, rect.y0), (rect.x1 - inset, rect.y0)),
                &edge,
                p.top_edge,
            );
        }
        ShelfStyle::Flat => {
            scene.stroke(&body, &edge, p.side_edge);
            let r = FLAT_RADIUS.min(rect.height() / 2.0);
            scene.stroke(
                &Line::new((rect.x0 + r, rect.y0), (rect.x1 - r, rect.y0)),
                &edge,
                p.top_edge,
            );
        }
    }
    let bottom = match style {
        ShelfStyle::Perspective => (rect.x0, rect.x1),
        ShelfStyle::Flat => (
            rect.x0 + FLAT_RADIUS.min(rect.height() / 2.0),
            rect.x1 - FLAT_RADIUS.min(rect.height() / 2.0),
        ),
    };
    scene.stroke(
        &Line::new((bottom.0, rect.y1), (bottom.1, rect.y1)),
        &edge,
        p.bottom_edge,
    );
    scene.pop_layer();
}

/// Draws what `draw` draws mirrored in the glass about the line `y = base` (the bottom of
/// the icons standing on the shelf) and squashed to [`REFLECTION_SCALE`] of its height, for
/// [`dock_shelf_with`]'s `reflections`. An icon `lift` above its base is reflected as far
/// below it, squashed alike.
pub fn reflected(scene: &mut Scene, base: f64, draw: impl FnOnce(&mut Scene)) {
    let mirror = Affine::translate((0.0, base))
        * Affine::scale_non_uniform(1.0, -REFLECTION_SCALE)
        * Affine::translate((0.0, -base));
    scene.with_transform(mirror, draw);
}

/// Draw the running indicator: a small light with a glow, centred on `center` (on the
/// shelf, under a running app's icon).
pub fn running_indicator(scene: &mut Scene, center: Point, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::Translucent);
    let (w, h) = INDICATOR;
    let core = Rect::from_center_size(center, (w, h));
    scene.blurred_rounded_rect(core, h / 2.0, INDICATOR_GLOW / 2.0, p.indicator[1]);
    scene.fill(&Ellipse::from_rect(core), p.indicator[0]);
}

/// Draw the tile shown for an app without an icon, filling the square `rect`: a plain
/// rounded tile with a soft gradient and a rim.
pub fn placeholder_icon(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::Opaque);
    let side = rect.width().min(rect.height());
    let inset = side * 0.08;
    let tile = rect.inset(-inset);
    let shape = RoundedRect::from_rect(tile, side * PLACEHOLDER_RADIUS);
    scene.fill(
        &shape,
        Gradient::new_linear(Point::new(0.0, tile.y0), Point::new(0.0, tile.y1))
            .with_stops([(0.0, p.placeholder[0]), (1.0, p.placeholder[1])]),
    );
    let rim = Stroke::new(1.0);
    scene.stroke(
        &RoundedRect::from_rect(tile.inset(-0.5), side * PLACEHOLDER_RADIUS - 0.5),
        &rim,
        p.placeholder[2],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn layers_are_balanced_for_both_styles() {
        for style in [ShelfStyle::Perspective, ShelfStyle::Flat] {
            let mut scene = Scene::new(300.0, 40.0);
            dock_shelf(
                &mut scene,
                Rect::new(0.0, 0.0, 300.0, DEPTH),
                style,
                Appearance::Dark,
                Backdrop::Translucent,
            );
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn reflections_are_mirrored_squashed_and_balanced() {
        let mut scene = Scene::new(300.0, 40.0);
        let mut seen = None;
        dock_shelf_with(
            &mut scene,
            Rect::new(0.0, 10.0, 300.0, 10.0 + DEPTH),
            ShelfStyle::Perspective,
            Appearance::Light,
            Backdrop::Opaque,
            |scene| {
                reflected(scene, 16.0, |scene| {
                    seen = Some(scene.transform() * Point::new(0.0, 6.0));
                });
            },
        );
        assert_eq!(scene.open_layers(), 0);
        // 10 px above the base is reflected 5 px below it.
        assert_eq!(seen, Some(Point::new(0.0, 21.0)));
    }

    #[test]
    fn nothing_is_reflected_under_increased_contrast() {
        let variant = Variant::default().with_contrast(neue_theme::Contrast::Increased);
        let mut called = false;
        dock_shelf_with(
            &mut Scene::new(300.0, 40.0),
            Rect::new(0.0, 0.0, 300.0, DEPTH),
            ShelfStyle::Perspective,
            variant,
            Backdrop::Opaque,
            |_| called = true,
        );
        assert!(!called);
    }

    #[test]
    fn far_edge_is_narrower_in_perspective() {
        let rect = Rect::new(0.0, 0.0, 300.0, DEPTH);
        let bounds = outline(rect, ShelfStyle::Perspective).bounding_box();
        assert_eq!(bounds, rect);
        let top = outline(rect, ShelfStyle::Perspective).elements()[1];
        assert_eq!(
            top,
            vello_common::kurbo::PathEl::LineTo(Point::new(300.0 - PERSPECTIVE_INSET, 0.0))
        );
    }

    #[test]
    fn short_shelves_keep_a_flat_far_edge() {
        let rect = Rect::new(0.0, 0.0, 20.0, DEPTH);
        let path = outline(rect, ShelfStyle::Perspective);
        assert_eq!(
            path.elements()[0],
            vello_common::kurbo::PathEl::MoveTo(Point::new(5.0, 0.0))
        );
    }
}
