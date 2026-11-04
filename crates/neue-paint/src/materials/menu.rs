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
    BezPath, Cap, Circle, Join, Point, Rect, RoundedRect, Shape, Stroke, Vec2,
};
use vello_common::peniko::{Color, Gradient};

use crate::Variant;
use crate::scene::Scene;

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
/// reaches [`SHADOW_REACH`] beyond the sides and top and a little more below.
pub fn menu_panel(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
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
    let p = Palette::new(variant.into());
    scene.fill(&rect, vertical(rect, p.highlight[0], p.highlight[1]));
    scene.fill(
        &Rect::new(rect.x0, rect.y0, rect.x1, rect.y0 + 1.0),
        p.highlight_edge,
    );
}

/// Draw a separator's line, centred across `rect` (a whole separator row).
pub fn menu_separator(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
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
