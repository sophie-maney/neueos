//! Menus: a light (or dark) rounded panel with a soft drop shadow, rows of items, thin
//! separator lines, and an accent band behind the highlighted item. The caller lays out
//! the items and draws their text; this draws the panel, the band, separators, check marks
//! and submenu arrows.
//!
//! Colours and metrics come from [`neue_theme::materials::menu`].

use neue_theme::materials::menu::{
    ARROW_SIZE, MARK_STROKE, Palette, RADIUS, RIM, SEPARATOR_LINE, SHADOW_BLUR, SHADOW_OFFSET,
};
use vello_common::kurbo::{
    Arc, BezPath, Cap, Circle, Join, Point, Rect, RoundedRect, Shape, Stroke, Vec2,
};
use vello_common::peniko::{Color, Gradient};

use crate::scene::Scene;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::menu::SHADOW_REACH;

/// The mark before an item's label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MenuMark {
    /// No mark.
    #[default]
    None,
    /// A check mark: the item is on.
    Check,
    /// A dash: the item is on for some of the selection (mixed).
    Dash,
    /// A dot: the chosen item of a radio group.
    Dot,
}

fn vertical(rect: Rect, top: Color, bottom: Color) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
        .with_stops([(0.0, top), (1.0, bottom)])
}

/// The outline of a menu panel filling `rect`.
pub fn menu_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.width().min(rect.height()) / 2.0))
}

/// Draw a menu panel filling `rect` (logical pixels): shadow, body and rim. The shadow
/// reaches [`SHADOW_REACH`] beyond the sides and top and a little more below. The body is
/// translucent on [`Backdrop::Translucent`], for the compositor to blur behind it.
pub fn menu_panel(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>, backdrop: Backdrop) {
    let p = Palette::new(variant.into(), backdrop);
    let outline = menu_outline(rect);

    // The shadow falls outside the panel only, so a translucent body (Phase 3) stays
    // clean: clip to the shadow's extent with the panel cut out.
    let mut frame = rect
        .inflate(SHADOW_REACH, SHADOW_REACH + SHADOW_OFFSET)
        .to_path(0.1);
    frame.extend(outline.to_path(0.1).reverse_subpaths());
    scene.push_clip(&frame);
    scene.blurred_rounded_rect(
        rect + Vec2::new(0.0, SHADOW_OFFSET / 2.0),
        outline.radii().top_left,
        SHADOW_BLUR,
        p.shadow,
    );
    scene.pop_layer();

    scene.fill(&outline, vertical(rect, p.fill[0], p.fill[1]));
    let rim = RoundedRect::from_rect(
        rect.inset(-RIM / 2.0),
        (outline.radii().top_left - RIM / 2.0).max(0.0),
    );
    scene.stroke(&rim, &Stroke::new(RIM), p.rim);
}

/// Draw the highlighted item's band filling `rect`.
pub fn menu_highlight(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::Opaque);
    scene.fill(&rect, vertical(rect, p.highlight[0], p.highlight[1]));
    scene.fill(
        &Rect::new(rect.x0, rect.y0, rect.x1, rect.y0 + 1.0),
        p.highlight_edge,
    );
}

/// Draw a separator's line, centred across `rect` (a whole separator row).
pub fn menu_separator(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::Opaque);
    let y = (rect.center().y - SEPARATOR_LINE / 2.0).round();
    scene.fill(
        &Rect::new(rect.x0, y, rect.x1, y + SEPARATOR_LINE),
        p.separator,
    );
}

/// Draw `mark` in `rect` (a [`neue_theme::materials::menu::MARK_SIZE`] square) in `color`.
pub fn menu_mark(scene: &mut Scene, rect: Rect, mark: MenuMark, color: Color) {
    let at =
        |fx: f64, fy: f64| Point::new(rect.x0 + rect.width() * fx, rect.y0 + rect.height() * fy);
    let stroke = Stroke::new(MARK_STROKE)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    match mark {
        MenuMark::None => {}
        MenuMark::Check => {
            let mut path = BezPath::new();
            path.move_to(at(0.05, 0.55));
            path.line_to(at(0.38, 0.88));
            path.line_to(at(0.95, 0.1));
            scene.stroke(&path, &stroke, color);
        }
        MenuMark::Dash => {
            let mut path = BezPath::new();
            path.move_to(at(0.1, 0.5));
            path.line_to(at(0.9, 0.5));
            scene.stroke(&path, &stroke, color);
        }
        MenuMark::Dot => {
            let r = rect.width().min(rect.height()) * 0.33;
            scene.fill(&Circle::new(rect.center(), r), color);
        }
    }
}

/// Draw the submenu arrow, a triangle pointing right, centred in `rect`, in `color`.
pub fn submenu_arrow(scene: &mut Scene, rect: Rect, color: Color) {
    let c = rect.center();
    let h = ARROW_SIZE / 2.0;
    let w = ARROW_SIZE * 0.8;
    let mut path = BezPath::new();
    path.move_to(Point::new(c.x - w / 2.0, c.y - h));
    path.line_to(Point::new(c.x + w / 2.0, c.y));
    path.line_to(Point::new(c.x - w / 2.0, c.y + h));
    path.close_path();
    scene.fill(&path, color);
}

/// Draw the signal glyph at an item's right in `rect` (a
/// [`neue_theme::materials::menu::SIGNAL_WIDTH`] by `SIGNAL_HEIGHT` box) in `color`: the
/// Wi-Fi fan with `bars` (0 to 3) of its arcs lit.
pub fn menu_signal(scene: &mut Scene, rect: Rect, bars: u8, color: Color) {
    super::menu_bar::wifi(scene, rect, bars, color);
}

/// Draw a padlock in `rect` (a [`neue_theme::materials::menu::LOCK_WIDTH`] by
/// `LOCK_HEIGHT` box) in `color`: the item needs a password.
pub fn menu_lock(scene: &mut Scene, rect: Rect, color: Color) {
    let body = Rect::new(rect.x0, rect.y0 + rect.height() * 0.45, rect.x1, rect.y1);
    scene.fill(&RoundedRect::from_rect(body, 1.5), color);
    // The shackle: a half circle over the body and a short leg down each side.
    let radius = rect.width() / 2.0 - 1.4;
    let top = Point::new(rect.center().x, body.y0 - radius + 0.5);
    let stroke = Stroke::new(1.3).with_caps(Cap::Butt);
    scene.stroke(
        &Arc::new(
            top,
            Vec2::new(radius, radius),
            std::f64::consts::PI,
            std::f64::consts::PI,
            0.0,
        ),
        &stroke,
        color,
    );
    for x in [top.x - radius, top.x + radius] {
        let mut leg = BezPath::new();
        leg.move_to((x, top.y));
        leg.line_to((x, body.y0 + 0.5));
        scene.stroke(&leg, &stroke, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn panel_layers_are_balanced() {
        let mut scene = Scene::new(200.0, 200.0);
        menu_panel(
            &mut scene,
            Rect::new(20.0, 20.0, 180.0, 180.0),
            Appearance::Light,
            Backdrop::Translucent,
        );
        assert_eq!(scene.open_layers(), 0);
    }

    #[test]
    fn every_mark_but_none_draws() {
        for (mark, commands) in [
            (MenuMark::None, 0),
            (MenuMark::Check, 1),
            (MenuMark::Dash, 1),
            (MenuMark::Dot, 1),
        ] {
            let mut scene = Scene::new(10.0, 10.0);
            menu_mark(
                &mut scene,
                Rect::new(0.0, 0.0, 9.0, 9.0),
                mark,
                Color::BLACK,
            );
            assert_eq!(scene.commands().len(), commands, "{mark:?}");
        }
    }

    #[test]
    fn separator_line_is_pixel_aligned() {
        let mut scene = Scene::new(100.0, 10.0);
        menu_separator(
            &mut scene,
            Rect::new(0.0, 0.0, 100.0, 10.0),
            Appearance::Dark,
        );
        assert_eq!(scene.commands().len(), 1);
    }
}
