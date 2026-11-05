//! The menu bar: a light (or dark) gradient strip with a highlight along its top edge and a
//! line along its bottom edge. Translucent over a blurred backdrop, or an opaque tint
//! without one. The caller lays out the menu titles and draws them with [`label`].
//!
//! Colours and metrics come from [`neue_theme::materials::menu_bar`].

use neue_theme::materials::menu_bar::{
    EDGE, LABEL_BASELINE, LABEL_EMBOSS_OFFSET, LOGO_STROKE, Palette,
};
use vello_common::kurbo::{BezPath, Cap, Circle, Point, Rect, Stroke};
use vello_common::peniko::{Color, Gradient};

use crate::scene::Scene;
use crate::text::TextLine;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::menu_bar::{FONT_SIZE, HEIGHT, LOGO_SIZE};

/// Draw the bar filling `rect` ([`HEIGHT`] tall for the real bar).
pub fn menu_bar(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>, backdrop: Backdrop) {
    let p = Palette::new(variant.into(), backdrop);
    let fill = Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
        .with_stops([(0.0, p.fill[0]), (1.0, p.fill[1])]);
    scene.fill(&rect, fill);
    scene.fill(
        &Rect::new(rect.x0, rect.y0, rect.x1, rect.y0 + EDGE),
        p.highlight,
    );
    scene.fill(
        &Rect::new(rect.x0, rect.y1 - EDGE, rect.x1, rect.y1),
        p.separator,
    );
}

/// Draw a menu title or menu extra whose text starts at `x`, vertically placed for a bar
/// whose top is `bar_top`: the embossed shadow, then the text.
pub fn label(
    scene: &mut Scene,
    line: &TextLine,
    x: f64,
    bar_top: f64,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into(), Backdrop::default());
    let origin = Point::new(x, bar_top + LABEL_BASELINE);
    scene.draw_text(
        line,
        Point::new(origin.x, origin.y + LABEL_EMBOSS_OFFSET),
        p.label_shadow,
    );
    scene.draw_text(line, origin, p.label);
}

/// Draw the neueOS logo mark (original artwork: a ring around a lowercase "n" arch) in
/// `rect`, a [`LOGO_SIZE`] square, in `color`.
pub fn logo(scene: &mut Scene, rect: Rect, color: Color) {
    let stroke = Stroke::new(LOGO_STROKE).with_caps(Cap::Round);
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0 - LOGO_STROKE / 2.0;
    scene.stroke(&Circle::new(c, r), &stroke, color);
    // The "n": a stem on the left, an arch over to a stem on the right.
    let w = r * 0.42;
    let (top, bottom) = (c.y - r * 0.30, c.y + r * 0.45);
    let mut n = BezPath::new();
    n.move_to((c.x - w, c.y - r * 0.40));
    n.line_to((c.x - w, bottom));
    n.move_to((c.x - w, top + w));
    n.quad_to((c.x - w, top), (c.x, top));
    n.quad_to((c.x + w, top), (c.x + w, top + w));
    n.line_to((c.x + w, bottom));
    scene.stroke(&n, &stroke, color);
}

/// Draw the logo mark in `rect` as a menu-bar label: its embossed shadow, then the mark.
pub fn logo_label(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::default());
    logo(
        scene,
        rect + vello_common::kurbo::Vec2::new(0.0, LABEL_EMBOSS_OFFSET),
        p.label_shadow,
    );
    logo(scene, rect, p.label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn draws_gradient_and_two_edges_without_layers() {
        let mut scene = Scene::new(100.0, HEIGHT);
        menu_bar(
            &mut scene,
            Rect::new(0.0, 0.0, 100.0, HEIGHT),
            Appearance::Light,
            Backdrop::Translucent,
        );
        assert_eq!(scene.commands().len(), 3);
        assert_eq!(scene.open_layers(), 0);
    }
}
