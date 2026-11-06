//! The menu bar: a light (or dark) gradient strip with a highlight along its top edge and a
//! line along its bottom edge. Translucent over a blurred backdrop, or an opaque tint
//! without one. The caller lays out the menu titles and draws them with [`label`].
//!
//! Colours and metrics come from [`neue_theme::materials::menu_bar`].

use neue_theme::materials::menu_bar::{
    BLUETOOTH_STROKE, EDGE, LABEL_BASELINE, LABEL_EMBOSS_OFFSET, LOGO_STROKE, OFF_OPACITY, Palette,
};
use vello_common::kurbo::{BezPath, Cap, Circle, Join, Point, Rect, RoundedRect, Stroke, Vec2};
use vello_common::peniko::{Color, Gradient};

use crate::scene::Scene;
use crate::text::TextLine;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::menu_bar::{
    BATTERY_HEIGHT, BATTERY_WIDTH, BLUETOOTH_HEIGHT, BLUETOOTH_WIDTH, FONT_SIZE, HEIGHT, LOGO_SIZE,
};

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
        rect + Vec2::new(0.0, LABEL_EMBOSS_OFFSET),
        p.label_shadow,
    );
    logo(scene, rect, p.label);
}

/// Width of a battery's terminal, at the right end of its icon.
const TERMINAL: f64 = 2.0;

/// Draw the battery icon in `rect` (a [`BATTERY_WIDTH`] by [`BATTERY_HEIGHT`] box) in
/// `color`: an outline, a terminal and a fill of `level` (0 to 1) of the inside. `charging`
/// puts a bolt over it, with a halo of `knockout`.
pub fn battery(
    scene: &mut Scene,
    rect: Rect,
    level: f64,
    charging: bool,
    color: Color,
    knockout: Color,
) {
    let body = Rect::new(rect.x0, rect.y0, rect.x1 - TERMINAL, rect.y1);
    let outline = body.inset(-0.5);
    scene.stroke(
        &RoundedRect::from_rect(outline, 2.5),
        &Stroke::new(1.0),
        color,
    );
    let terminal = Rect::new(
        body.x1 + 0.5,
        rect.center().y - 2.0,
        rect.x1,
        rect.center().y + 2.0,
    );
    scene.fill(&RoundedRect::from_rect(terminal, 0.75), color);
    let inside = body.inset(-2.0);
    let level = level.clamp(0.0, 1.0);
    if level > 0.0 {
        let width = (inside.width() * level).max(1.0);
        let fill = Rect::new(inside.x0, inside.y0, inside.x0 + width, inside.y1);
        scene.fill(&RoundedRect::from_rect(fill, 1.0), color);
    }
    if charging {
        let c = body.center();
        let mut bolt = BezPath::new();
        bolt.move_to((c.x + 1.0, c.y - 4.5));
        bolt.line_to((c.x - 2.8, c.y + 0.7));
        bolt.line_to((c.x - 0.2, c.y + 0.7));
        bolt.line_to((c.x - 1.0, c.y + 4.5));
        bolt.line_to((c.x + 2.8, c.y - 0.7));
        bolt.line_to((c.x + 0.2, c.y - 0.7));
        bolt.close_path();
        let halo = Stroke::new(1.6).with_join(Join::Round);
        scene.stroke(&bolt, &halo, knockout);
        scene.fill(&bolt, color);
    }
}

/// Draw the battery icon in `rect` as a menu-bar label: its embossed shadow, then the icon.
pub fn battery_label(
    scene: &mut Scene,
    rect: Rect,
    level: f64,
    charging: bool,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into(), Backdrop::default());
    let shadow = rect + Vec2::new(0.0, LABEL_EMBOSS_OFFSET);
    battery(
        scene,
        shadow,
        level,
        false,
        p.label_shadow,
        Color::TRANSPARENT,
    );
    battery(scene, rect, level, charging, p.label, p.knockout);
}

/// Draw the Bluetooth icon in `rect` (a [`BLUETOOTH_WIDTH`] by [`BLUETOOTH_HEIGHT`] box)
/// in `color`: the rune, and a dot on each side when `connected`.
pub fn bluetooth(scene: &mut Scene, rect: Rect, connected: bool, color: Color) {
    let c = rect.center();
    let (w, h) = (2.8, rect.height() / 2.0 - BLUETOOTH_STROKE / 2.0);
    // Down the middle, then the two arrowheads' strokes crossing over it.
    let mut rune = BezPath::new();
    rune.move_to((c.x - w, c.y - h * 0.5));
    rune.line_to((c.x + w, c.y + h * 0.5));
    rune.line_to((c.x, c.y + h));
    rune.line_to((c.x, c.y - h));
    rune.line_to((c.x + w, c.y - h * 0.5));
    rune.line_to((c.x - w, c.y + h * 0.5));
    let stroke = Stroke::new(BLUETOOTH_STROKE)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    scene.stroke(&rune, &stroke, color);
    if connected {
        for x in [rect.x0 + 1.0, rect.x1 - 1.0] {
            scene.fill(&Circle::new((x, c.y), 1.0), color);
        }
    }
}

/// Draw the Bluetooth icon in `rect` as a menu-bar label: its embossed shadow, then the
/// icon, faded while the adapter is off (`powered` false).
pub fn bluetooth_label(
    scene: &mut Scene,
    rect: Rect,
    powered: bool,
    connected: bool,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into(), Backdrop::default());
    if !powered {
        scene.push_opacity(OFF_OPACITY);
    }
    let shadow = rect + Vec2::new(0.0, LABEL_EMBOSS_OFFSET);
    bluetooth(scene, shadow, connected, p.label_shadow);
    bluetooth(scene, rect, connected, p.label);
    if !powered {
        scene.pop_layer();
    }
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

    #[test]
    fn icons_balance_their_layers() {
        let rect = Rect::new(0.0, 0.0, BATTERY_WIDTH, HEIGHT);
        let mut scene = Scene::new(BATTERY_WIDTH, HEIGHT);
        battery_label(&mut scene, rect, 0.5, true, Appearance::Dark);
        bluetooth_label(&mut scene, rect, false, true, Appearance::Light);
        assert_eq!(scene.open_layers(), 0);
        assert!(!scene.commands().is_empty());
    }

    #[test]
    fn an_empty_battery_has_no_fill_and_a_charging_one_a_bolt() {
        let count = |level, charging| {
            let mut scene = Scene::new(BATTERY_WIDTH, BATTERY_HEIGHT);
            let rect = Rect::new(0.0, 0.0, BATTERY_WIDTH, BATTERY_HEIGHT);
            battery(
                &mut scene,
                rect,
                level,
                charging,
                Color::BLACK,
                Color::WHITE,
            );
            scene.commands().len()
        };
        assert_eq!(count(0.0, false) + 1, count(0.5, false));
        assert_eq!(count(0.5, false) + 2, count(0.5, true));
    }
}
