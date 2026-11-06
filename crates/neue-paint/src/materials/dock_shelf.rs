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
//! Also here: the running indicator that glows on the shelf under an app's icon, the
//! tile shown for an app without an icon, the divider between the dock's sections and the
//! tile of a minimised window (its thumbnail, or a drawing of a window while there is none).

use neue_theme::materials::dock_shelf::{
    DIVIDER_INSET, DIVIDER_WIDTH, EDGE, FLAT_RADIUS, INDICATOR, INDICATOR_GLOW, PERSPECTIVE_INSET,
    PLACEHOLDER_RADIUS, Palette, REFLECTION_SCALE, SHEEN_HEIGHT, THUMBNAIL_RADIUS,
    THUMBNAIL_SHADOW_BLUR, THUMBNAIL_SHADOW_OFFSET, WINDOW_BADGE, WINDOW_RADIUS, WINDOW_TITLE,
};
use vello_common::kurbo::{
    Affine, BezPath, Ellipse, Line, Point, Rect, RoundedRect, Shape, Stroke, Vec2,
};
use vello_common::peniko::Gradient;

use crate::icon::draw_raster;
use crate::materials::desktop::Picture;
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

/// Draw the divider between the dock's sections in the slot `rect` (a tile's stretch of the
/// dock): a thin line across the middle of it, standing as tall as the icons and
/// upright whichever edge the dock is on (`vertical`: the line runs up and down).
pub fn dock_divider(scene: &mut Scene, rect: Rect, vertical: bool, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::Opaque);
    let centre = rect.center();
    let line = if vertical {
        let inset = rect.height() * DIVIDER_INSET;
        Rect::new(
            centre.x - DIVIDER_WIDTH / 2.0,
            rect.y0 + inset,
            centre.x + DIVIDER_WIDTH / 2.0,
            rect.y1 - inset,
        )
    } else {
        let inset = rect.width() * DIVIDER_INSET;
        Rect::new(
            rect.x0 + inset,
            centre.y - DIVIDER_WIDTH / 2.0,
            rect.x1 - inset,
            centre.y + DIVIDER_WIDTH / 2.0,
        )
    };
    scene.fill(&line, p.divider);
}

/// Draw the tile of a minimised window filling the square `rect`: its `thumbnail` fitted
/// in the tile over a soft shadow, or a small window (a title bar over a body) while there
/// is none; then `badge`, if any, drawn by the caller in the rect it is given, in its bottom
/// right corner.
pub fn minimised_window(
    scene: &mut Scene,
    rect: Rect,
    variant: impl Into<Variant>,
    thumbnail: Option<&Picture>,
    badge: impl FnOnce(&mut Scene, Rect),
) {
    let variant = variant.into();
    let p = Palette::new(variant, Backdrop::Opaque);
    let side = rect.width().min(rect.height());
    let card = rect.inset(-side * 0.06);
    match thumbnail {
        Some(picture) => draw_thumbnail(scene, card, side, picture, &p),
        None => draw_window(scene, card, side, &p),
    }
    let size = card.width().min(card.height()) * WINDOW_BADGE;
    let corner = side * 0.02;
    badge(
        scene,
        Rect::new(
            card.x1 - size - corner,
            card.y1 - size - corner,
            card.x1 - corner,
            card.y1 - corner,
        ),
    );
}

/// The drawing of a window filling `card`, for a tile `side` long.
fn draw_window(scene: &mut Scene, card: Rect, side: f64, p: &Palette) {
    let radius = side * WINDOW_RADIUS;
    let shape = RoundedRect::from_rect(card, radius);
    scene.fill(
        &shape,
        Gradient::new_linear(Point::new(0.0, card.y0), Point::new(0.0, card.y1))
            .with_stops([(0.0, p.window[0]), (1.0, p.window[1])]),
    );
    // The title bar: the top of the card, cut to its corners.
    scene.push_clip(&shape);
    scene.fill(
        &Rect::new(card.x0, card.y0, card.x1, card.y0 + side * WINDOW_TITLE),
        p.window[3],
    );
    scene.pop_layer();
    scene.stroke(
        &RoundedRect::from_rect(card.inset(-0.5), radius - 0.5),
        &Stroke::new(1.0),
        p.window[2],
    );
}

/// `picture` as large as fits in `area` (keeping its proportions, centred), over a
/// shadow, for a tile `side` long.
fn draw_thumbnail(scene: &mut Scene, area: Rect, side: f64, picture: &Picture, p: &Palette) {
    let (pw, ph) = (f64::from(picture.width()), f64::from(picture.height()));
    let k = (area.width() / pw).min(area.height() / ph);
    // Whole logical pixels, as the picture is placed (`draw_raster`).
    let (w, h) = ((pw * k).round().max(1.0), (ph * k).round().max(1.0));
    let x = (area.x0 + (area.width() - w) / 2.0).round();
    let y = (area.y0 + (area.height() - h) / 2.0).round();
    let at = Rect::new(x, y, x + w, y + h);
    let radius = (side * THUMBNAIL_RADIUS).min(w / 2.0).min(h / 2.0);
    scene.blurred_rounded_rect(
        at + Vec2::new(0.0, side * THUMBNAIL_SHADOW_OFFSET),
        radius,
        side * THUMBNAIL_SHADOW_BLUR,
        p.thumbnail_shadow,
    );
    let shape = RoundedRect::from_rect(at, radius);
    scene.push_clip(&shape);
    draw_raster(scene, at, picture);
    scene.pop_layer();
    scene.stroke(
        &RoundedRect::from_rect(at.inset(-0.5), (radius - 0.5).max(0.0)),
        &Stroke::new(1.0),
        p.window[2],
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

    #[test]
    fn a_minimised_window_hands_its_badge_the_bottom_right_and_balances_layers() {
        let picture = Picture::from_rgba8(4, 2, &[200; 32]).expect("a picture");
        for variant in [Appearance::Light, Appearance::Dark] {
            for thumbnail in [None, Some(&picture)] {
                let mut scene = Scene::new(48.0, 48.0);
                let mut badge = None;
                minimised_window(
                    &mut scene,
                    Rect::new(0.0, 0.0, 48.0, 48.0),
                    variant,
                    thumbnail,
                    |_, rect| badge = Some(rect),
                );
                assert_eq!(scene.open_layers(), 0);
                let badge = badge.expect("the badge is drawn by the caller");
                assert!(badge.x1 < 48.0 && badge.y1 < 48.0);
                assert!(badge.x0 > 12.0 && badge.y0 > 12.0, "{badge:?}");
                assert!((badge.width() - badge.height()).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn a_divider_is_centred_in_its_slot_and_balanced() {
        let mut scene = Scene::new(52.0, 48.0);
        dock_divider(
            &mut scene,
            Rect::new(0.0, 0.0, 52.0, 48.0),
            true,
            Appearance::Light,
        );
        dock_divider(
            &mut scene,
            Rect::new(0.0, 0.0, 52.0, 48.0),
            false,
            Appearance::Dark,
        );
        assert_eq!(scene.open_layers(), 0);
    }
}
