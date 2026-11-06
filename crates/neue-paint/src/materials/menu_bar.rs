//! The menu bar: a light (or dark) gradient strip with a highlight along its top edge and a
//! line along its bottom edge. Translucent over a blurred backdrop, or an opaque tint
//! without one. The caller lays out the menu titles and draws them with [`label`].
//!
//! Colours and metrics come from [`neue_theme::materials::menu_bar`].

use neue_theme::materials::menu_bar::{
    BLUETOOTH_STROKE, EDGE, LABEL_BASELINE, LABEL_EMBOSS_OFFSET, LOGO_STROKE, NETWORK_STROKE,
    OFF_OPACITY, Palette, TRAY_DARK_OPACITY, UNLIT_OPACITY, VOLUME_STROKE,
};
use vello_common::kurbo::{
    Arc, BezPath, Cap, Circle, Join, Point, Rect, RoundedRect, Stroke, Vec2,
};
use vello_common::peniko::{Color, Gradient};

use crate::icon::{self, Icon};
use crate::scene::Scene;
use crate::text::TextLine;
use crate::{Backdrop, Variant};

pub use neue_theme::materials::menu_bar::{
    BATTERY_HEIGHT, BATTERY_WIDTH, BLUETOOTH_HEIGHT, BLUETOOTH_WIDTH, FONT_SIZE, HEIGHT, LOGO_SIZE,
    NETWORK_HEIGHT, NETWORK_WIDTH, VOLUME_HEIGHT, VOLUME_WIDTH,
};

/// How many arcs a Wi-Fi fan has.
pub const WIFI_ARCS: u8 = 3;

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

/// Draw the Wi-Fi fan in `rect` in `color`: a dot at the bottom and [`WIFI_ARCS`] arcs over
/// it, the first `lit` of them in full and the rest faded.
pub fn wifi(scene: &mut Scene, rect: Rect, lit: u8, color: Color) {
    let stroke = Stroke::new(NETWORK_STROKE).with_caps(Cap::Round);
    // The fan's tip is at the bottom centre; the arcs are 90 degrees of circles around it.
    let tip = Point::new(rect.center().x, rect.y1 - 1.2);
    scene.fill(&Circle::new(tip, 1.3), color);
    let reach = (rect.height() - 2.5).min(rect.width() / 2.0 * 1.35);
    for arc in 1..=WIFI_ARCS {
        let radius = reach * f64::from(arc) / f64::from(WIFI_ARCS);
        let shape = Arc::new(
            tip,
            Vec2::new(radius, radius),
            -std::f64::consts::FRAC_PI_2 - std::f64::consts::FRAC_PI_4,
            std::f64::consts::FRAC_PI_2,
            0.0,
        );
        let brush = if arc <= lit {
            color
        } else {
            color.multiply_alpha(UNLIT_OPACITY)
        };
        scene.stroke(&shape, &stroke, brush);
    }
}

/// Draw the wired (Ethernet) icon in `rect` in `color`: two chevrons pointing outwards
/// around a dot.
pub fn wired(scene: &mut Scene, rect: Rect, color: Color) {
    let c = rect.center();
    let stroke = Stroke::new(NETWORK_STROKE)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    let reach = 3.6;
    for side in [-1.0, 1.0] {
        let outer = c.x + side * (rect.width() / 2.0 - 1.0);
        let inner = outer - side * 3.6;
        let mut chevron = BezPath::new();
        chevron.move_to((inner, c.y - reach));
        chevron.line_to((outer, c.y));
        chevron.line_to((inner, c.y + reach));
        scene.stroke(&chevron, &stroke, color);
    }
    scene.fill(&Circle::new(c, 1.4), color);
}

/// Draw the Wi-Fi fan in `rect` as a menu-bar label: its embossed shadow, then the fan.
/// `off` (the switch is off) fades it and strikes it through.
pub fn wifi_label(scene: &mut Scene, rect: Rect, lit: u8, off: bool, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::default());
    if off {
        scene.push_opacity(OFF_OPACITY);
    }
    let shadow = rect + Vec2::new(0.0, LABEL_EMBOSS_OFFSET);
    let lit = if off { WIFI_ARCS } else { lit };
    wifi(scene, shadow, lit, p.label_shadow);
    wifi(scene, rect, lit, p.label);
    if off {
        strike(scene, shadow, p.label_shadow);
        strike(scene, rect, p.label);
        scene.pop_layer();
    }
}

/// Draw the wired icon in `rect` as a menu-bar label: its embossed shadow, then the icon,
/// faded while nothing is connected.
pub fn wired_label(scene: &mut Scene, rect: Rect, connected: bool, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into(), Backdrop::default());
    if !connected {
        scene.push_opacity(OFF_OPACITY);
    }
    wired(
        scene,
        rect + Vec2::new(0.0, LABEL_EMBOSS_OFFSET),
        p.label_shadow,
    );
    wired(scene, rect, p.label);
    if !connected {
        scene.pop_layer();
    }
}

/// Draw a status icon of the tray in `rect` as a menu-bar label. An icon with colours of
/// its own (an app's picture) keeps them, a dark bar softening it by
/// [`TRAY_DARK_OPACITY`]; one that follows the colour it is drawn in (a glyph) is drawn in
/// the label's colour, `highlighted` (over the blue of an open menu's title) or not.
pub fn image_label(
    scene: &mut Scene,
    rect: Rect,
    image: &Icon,
    highlighted: Option<Color>,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    let p = Palette::new(variant, Backdrop::default());
    let dim = variant.is_dark() && highlighted.is_none() && image.has_own_colors();
    if dim {
        scene.push_opacity(TRAY_DARK_OPACITY);
    }
    if let Some(colour) = highlighted {
        icon::icon(scene, rect, image, colour, variant);
    } else {
        if !image.has_own_colors() {
            icon::icon(
                scene,
                rect + Vec2::new(0.0, LABEL_EMBOSS_OFFSET),
                image,
                p.label_shadow,
                variant,
            );
        }
        icon::icon(scene, rect, image, p.label, variant);
    }
    if dim {
        scene.pop_layer();
    }
}

/// How many waves a speaker has.
pub const SPEAKER_WAVES: u8 = 3;

/// Draw the speaker in `rect` (a [`VOLUME_WIDTH`] by [`VOLUME_HEIGHT`] box) in `color`:
/// a magnet and a cone facing right, and [`SPEAKER_WAVES`] arcs in front of it, the first
/// `lit` of them in full and the rest faded (so the icon keeps its size at every level).
pub fn speaker(scene: &mut Scene, rect: Rect, lit: u8, color: Color) {
    let c = rect.center();
    let x = rect.x0 + 0.5;
    let magnet = RoundedRect::new(x, c.y - 2.4, x + 3.0, c.y + 2.4, 0.6);
    scene.fill(&magnet, color);
    let tip = x + 7.5;
    let mut cone = BezPath::new();
    cone.move_to((x + 2.6, c.y - 2.4));
    cone.line_to((tip, c.y - 5.6));
    cone.line_to((tip, c.y + 5.6));
    cone.line_to((x + 2.6, c.y + 2.4));
    cone.close_path();
    scene.fill(&cone, color);
    let stroke = Stroke::new(VOLUME_STROKE).with_caps(Cap::Round);
    let sweep = 0.28 * std::f64::consts::PI;
    for wave in 1..=SPEAKER_WAVES {
        let radius = 0.6 + 2.6 * f64::from(wave);
        let arc = Arc::new(
            Point::new(tip, c.y),
            Vec2::new(radius, radius),
            -sweep,
            2.0 * sweep,
            0.0,
        );
        let brush = if wave <= lit {
            color
        } else {
            color.multiply_alpha(UNLIT_OPACITY)
        };
        scene.stroke(&arc, &stroke, brush);
    }
}

/// Draw the speaker in `rect` as a menu-bar label: its embossed shadow, then the speaker
/// with `lit` waves. `muted` fades it and lights no wave.
pub fn speaker_label(
    scene: &mut Scene,
    rect: Rect,
    lit: u8,
    muted: bool,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into(), Backdrop::default());
    let lit = if muted { 0 } else { lit };
    if muted {
        scene.push_opacity(OFF_OPACITY);
    }
    speaker(
        scene,
        rect + Vec2::new(0.0, LABEL_EMBOSS_OFFSET),
        lit,
        p.label_shadow,
    );
    speaker(scene, rect, lit, p.label);
    if muted {
        scene.pop_layer();
    }
}

/// A diagonal line across `rect`: what strikes out an icon that is switched off.
pub fn strike(scene: &mut Scene, rect: Rect, color: Color) {
    let mut line = BezPath::new();
    line.move_to((rect.x0 + 2.0, rect.y0 + 1.0));
    line.line_to((rect.x1 - 2.0, rect.y1));
    scene.stroke(
        &line,
        &Stroke::new(NETWORK_STROKE).with_caps(Cap::Round),
        color,
    );
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
    fn a_picture_is_softened_on_a_dark_bar_only_and_a_glyph_gets_a_shadow() {
        use crate::icon::Glyph;
        use crate::materials::desktop::Picture;
        let picture = Icon::raster(Picture::from_rgba8(1, 1, &[255, 0, 0, 255]).unwrap());
        let glyph = Icon::from(Glyph::Folder);
        let rect = Rect::new(0.0, 0.0, 16.0, HEIGHT);
        let layers = |icon: &Icon, appearance| {
            let mut scene = Scene::new(16.0, HEIGHT);
            image_label(&mut scene, rect, icon, None, appearance);
            assert_eq!(scene.open_layers(), 0, "balanced");
            scene
                .commands()
                .iter()
                .filter(|c| matches!(c, crate::scene::Command::PushLayer { .. }))
                .count()
        };
        assert_eq!(layers(&picture, Appearance::Light), 0);
        assert_eq!(layers(&picture, Appearance::Dark), 1);
        // A glyph follows the label colour, so it isn't dimmed.
        assert_eq!(layers(&glyph, Appearance::Dark), 0);
        // A picture is drawn once; a glyph twice (shadow, then the glyph).
        let fills = |icon: &Icon| {
            let mut scene = Scene::new(16.0, HEIGHT);
            image_label(&mut scene, rect, icon, None, Appearance::Light);
            scene.commands().len()
        };
        assert_eq!(fills(&glyph), 2 * fills(&picture));
    }

    #[test]
    fn a_muted_speaker_is_faded_and_its_waves_unlit() {
        let rect = Rect::new(0.0, 0.0, VOLUME_WIDTH, VOLUME_HEIGHT);
        let mut scene = Scene::new(VOLUME_WIDTH, VOLUME_HEIGHT);
        speaker_label(&mut scene, rect, 2, true, Appearance::Light);
        assert_eq!(scene.open_layers(), 0);
        let mut plain = Scene::new(VOLUME_WIDTH, VOLUME_HEIGHT);
        speaker_label(&mut plain, rect, 2, false, Appearance::Light);
        // The fade's layer: one push and one pop more.
        assert_eq!(scene.commands().len(), plain.commands().len() + 2);
        // Magnet, cone and three waves, whatever the level.
        let mut one = Scene::new(VOLUME_WIDTH, VOLUME_HEIGHT);
        speaker(&mut one, rect, 0, Color::BLACK);
        assert_eq!(one.commands().len(), 2 + usize::from(SPEAKER_WAVES));
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

    #[test]
    fn network_icons_balance_their_layers_and_light_their_arcs() {
        let rect = Rect::new(0.0, 0.0, NETWORK_WIDTH, NETWORK_HEIGHT);
        let mut scene = Scene::new(NETWORK_WIDTH, NETWORK_HEIGHT);
        wifi_label(&mut scene, rect, 2, true, Appearance::Dark);
        wifi_label(&mut scene, rect, 1, false, Appearance::Light);
        wired_label(&mut scene, rect, false, Appearance::Dark);
        wired_label(&mut scene, rect, true, Appearance::Light);
        assert_eq!(scene.open_layers(), 0);
        // Every lit level draws the same shapes (the unlit arcs only fade).
        let count = |lit| {
            let mut scene = Scene::new(NETWORK_WIDTH, NETWORK_HEIGHT);
            wifi(&mut scene, rect, lit, Color::BLACK);
            scene.commands().len()
        };
        assert_eq!(count(0), count(3));
    }
}
