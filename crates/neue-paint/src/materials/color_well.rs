//! The colour well and its panel's hue and saturation area, and the colour maths they
//! share: HSB (hue, saturation, brightness) to sRGB and back, and hex notation.
//!
//! The well is a bevel button holding a swatch; the swatch shows the colour's opacity over
//! a checkerboard. Colours and metrics come from [`neue_theme::materials::color_well`].

use neue_theme::materials::color_well::{
    AREA_RADIUS, CHECKER, DISABLED_OPACITY, KNOB_RADIUS, SWATCH_INSET, SWATCH_RADIUS, border,
    checker, knob_outline, knob_ring,
};
use vello_common::kurbo::{Circle, Point, Rect, RoundedRect, Stroke};
use vello_common::peniko::{Color, Gradient};

use super::bevel_button::{BevelButton, bevel_button};
use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// A colour as hue, saturation, brightness and opacity. Hue is in degrees, `0.0..360.0`;
/// the others run from 0 to 1.
///
/// HSB loses information in greys and black (a grey has no hue, black has no saturation
/// either), so [`Hsb::from_color`] takes the previous value to keep what the colour can't
/// say.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hsb {
    /// Hue in degrees, `0.0..360.0`.
    pub hue: f32,
    /// Saturation, 0 (grey) to 1 (full colour).
    pub saturation: f32,
    /// Brightness, 0 (black) to 1.
    pub brightness: f32,
    /// Opacity, 0 (clear) to 1.
    pub alpha: f32,
}

impl Hsb {
    /// The sRGB colour with these values. Out-of-range values are clamped (hue wraps).
    pub fn to_color(self) -> Color {
        let s = self.saturation.clamp(0.0, 1.0);
        let v = self.brightness.clamp(0.0, 1.0);
        let h = self.hue.rem_euclid(360.0) / 60.0;
        let channel = |n: f32| {
            let k = (n + h) % 6.0;
            v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
        };
        Color::new([
            channel(5.0),
            channel(3.0),
            channel(1.0),
            self.alpha.clamp(0.0, 1.0),
        ])
    }

    /// The HSB values of `color`. Where `color` leaves one open (hue of a grey, saturation
    /// of black), `previous`'s stays.
    pub fn from_color(color: Color, previous: Self) -> Self {
        let [r, g, b, a] = color.components;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let grey = delta <= f32::EPSILON;
        let hue = if grey {
            previous.hue
        } else if max == r {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let saturation = if max <= 0.0 {
            previous.saturation
        } else if grey {
            0.0
        } else {
            delta / max
        };
        Self {
            hue: hue.rem_euclid(360.0),
            saturation,
            brightness: max,
            alpha: a,
        }
    }
}

/// `color` in hex: `#RRGGBB`, or `#RRGGBBAA` if it is not fully opaque. Upper case.
pub fn color_hex(color: Color) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel clamped to 0..255 and rounded"
    )]
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b, a] = color.components.map(byte);
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

/// The colour `text` spells in hex: `RGB`, `RRGGBB` or `RRGGBBAA`, with or without a
/// leading `#`, in either case. Without an alpha part, the colour gets `alpha`.
pub fn parse_hex(text: &str, alpha: f32) -> Option<Color> {
    let digits = text.trim().trim_start_matches('#');
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // Only ASCII digits are left, so byte offsets are character offsets.
    let pair = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    let nibble = |i: usize| u8::from_str_radix(&digits[i..=i], 16).ok().map(|v| v * 17);
    let (r, g, b, a) = match digits.len() {
        3 => (nibble(0)?, nibble(1)?, nibble(2)?, None),
        6 => (pair(0)?, pair(2)?, pair(4)?, None),
        8 => (pair(0)?, pair(2)?, pair(4)?, Some(pair(6)?)),
        _ => return None,
    };
    let unit = |v: u8| f32::from(v) / 255.0;
    Some(Color::new([
        unit(r),
        unit(g),
        unit(b),
        a.map_or(alpha.clamp(0.0, 1.0), unit),
    ]))
}

/// Everything that changes how a colour well looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorWell {
    /// The colour in the swatch (opacity shows over a checkerboard).
    pub color: Color,
    /// How far the bevel is pressed in, from 0 to 1: held down, or its panel is open.
    pub depth: f32,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the well.
    pub focused: bool,
}

/// Fills `rect` with the checkerboard that shows through transparent colours.
pub fn checkerboard(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let [light, dark] = checker(variant.into());
    scene.push_clip(&rect);
    scene.fill(&rect, light);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive count of squares"
    )]
    let count = |length: f64| (length / CHECKER).ceil().max(0.0) as usize;
    for row in 0..count(rect.height()) {
        for column in 0..count(rect.width()) {
            if (row + column) % 2 == 1 {
                #[expect(clippy::cast_precision_loss, reason = "a small square count")]
                let (x, y) = (
                    rect.x0 + column as f64 * CHECKER,
                    rect.y0 + row as f64 * CHECKER,
                );
                scene.fill(&Rect::new(x, y, x + CHECKER, y + CHECKER), dark);
            }
        }
    }
    scene.pop_layer();
}

/// Draw a colour well filling `rect` (logical pixels; the regular height is
/// [`neue_theme::metrics::control::REGULAR_HEIGHT`]). The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn color_well(scene: &mut Scene, rect: Rect, well: ColorWell, variant: impl Into<Variant>) {
    let variant = variant.into();
    bevel_button(
        scene,
        rect,
        BevelButton {
            depth: well.depth,
            disabled: well.disabled,
            focused: well.focused,
        },
        variant,
    );
    if well.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    let swatch = rect.inset(-SWATCH_INSET);
    let shape = RoundedRect::from_rect(swatch, SWATCH_RADIUS);
    scene.push_clip(&shape);
    checkerboard(scene, swatch, variant);
    scene.fill(&shape, well.color);
    scene.pop_layer();
    scene.stroke(&shape, &Stroke::new(1.0), border(variant));
    if well.disabled {
        scene.pop_layer();
    }
}

/// Everything that changes how the hue and saturation area looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorArea {
    /// The colour chosen: hue runs left to right, saturation top (full) to bottom (grey),
    /// and the brightness darkens the whole area.
    pub hsb: Hsb,
    /// Greyed out: reduced opacity, no knob focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the knob.
    pub focused: bool,
}

/// Where the knob for `hsb` sits in an area filling `rect`.
pub fn color_area_knob(rect: Rect, hsb: Hsb) -> Point {
    let x = rect.x0 + f64::from(hsb.hue.rem_euclid(360.0) / 360.0) * rect.width();
    let y = rect.y0 + f64::from(1.0 - hsb.saturation.clamp(0.0, 1.0)) * rect.height();
    Point::new(x, y)
}

/// The hue (degrees) and saturation the point `p` selects in an area filling `rect`;
/// points outside it select the nearest edge.
pub fn color_area_value_at(rect: Rect, p: Point) -> (f32, f32) {
    let fx = ((p.x - rect.x0) / rect.width().max(1.0)).clamp(0.0, 1.0);
    let fy = ((p.y - rect.y0) / rect.height().max(1.0)).clamp(0.0, 1.0);
    // Hue wraps, so the right edge is the left edge's hue again: stop just short of it.
    #[expect(clippy::cast_possible_truncation, reason = "a 0..1 position")]
    let (fx, fy) = (fx as f32, fy as f32);
    ((fx * 360.0).min(359.99), 1.0 - fy)
}

/// Draw the hue and saturation area filling `rect` (logical pixels). The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside the knob.
pub fn color_area(scene: &mut Scene, rect: Rect, area: ColorArea, variant: impl Into<Variant>) {
    let variant = variant.into();
    let v = area.hsb.brightness.clamp(0.0, 1.0);
    if area.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    let shape = RoundedRect::from_rect(rect, AREA_RADIUS);
    scene.push_clip(&shape);
    // Hue runs through the six corners of the colour cube, so six straight stretches of
    // gradient are exact.
    let hues: Vec<(f32, Color)> = (0..=6u8)
        .map(|i| {
            let full = Hsb {
                hue: f32::from(i) * 60.0,
                saturation: 1.0,
                brightness: v,
                alpha: 1.0,
            };
            (f32::from(i) / 6.0, full.to_color())
        })
        .collect();
    scene.fill(
        &rect,
        Gradient::new_linear(Point::new(rect.x0, 0.0), Point::new(rect.x1, 0.0)).with_stops(&*hues),
    );
    // Saturation fades each column from its hue to the grey of the same brightness.
    let grey = Color::new([v, v, v, 1.0]);
    scene.fill(
        &rect,
        Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
            .with_stops([(0.0, grey.with_alpha(0.0)), (1.0, grey)]),
    );
    scene.pop_layer();
    scene.stroke(&shape, &Stroke::new(1.0), border(variant));

    let knob = Circle::new(color_area_knob(rect, area.hsb), KNOB_RADIUS);
    let chosen = Hsb {
        alpha: 1.0,
        ..area.hsb
    };
    scene.fill(&knob, chosen.to_color());
    scene.stroke(&knob, &Stroke::new(2.0), knob_ring(variant));
    let reach = KNOB_RADIUS + 1.5;
    scene.stroke(
        &Circle::new(knob.center, reach),
        &Stroke::new(1.0),
        knob_outline(variant),
    );
    if area.disabled {
        scene.pop_layer();
    } else if area.focused {
        let bounds = Rect::from_center_size(knob.center, (2.0 * reach, 2.0 * reach));
        focus_ring(scene, RoundedRect::from_rect(bounds, reach), variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn primaries_and_secondaries_have_the_expected_hues() {
        let hue = |r, g, b| Hsb::from_color(Color::new([r, g, b, 1.0]), Hsb::default()).hue;
        assert_eq!(hue(1.0, 0.0, 0.0), 0.0);
        assert_eq!(hue(1.0, 1.0, 0.0), 60.0);
        assert_eq!(hue(0.0, 1.0, 0.0), 120.0);
        assert_eq!(hue(0.0, 1.0, 1.0), 180.0);
        assert_eq!(hue(0.0, 0.0, 1.0), 240.0);
        assert_eq!(hue(1.0, 0.0, 1.0), 300.0);
    }

    #[test]
    fn every_8_bit_colour_round_trips_through_hsb() {
        // A coarse sweep of the cube, edges and greys included.
        let steps = [0u8, 1, 17, 51, 85, 128, 170, 203, 254, 255];
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    let color = Color::new([
                        f32::from(r) / 255.0,
                        f32::from(g) / 255.0,
                        f32::from(b) / 255.0,
                        1.0,
                    ]);
                    let hsb = Hsb::from_color(color, Hsb::default());
                    assert!(
                        close(hsb.to_color().components, color.components),
                        "{r},{g},{b} -> {hsb:?} -> {:?}",
                        hsb.to_color().components
                    );
                    assert_eq!(color_hex(hsb.to_color()), color_hex(color));
                }
            }
        }
    }

    #[test]
    fn hsb_round_trips_through_a_colour() {
        for hue in [0.0, 30.0, 59.0, 60.0, 119.5, 200.0, 299.0, 359.0] {
            for (saturation, brightness) in [(1.0, 1.0), (0.5, 0.75), (0.2, 0.3), (0.9, 0.05)] {
                let hsb = Hsb {
                    hue,
                    saturation,
                    brightness,
                    alpha: 0.4,
                };
                let back = Hsb::from_color(hsb.to_color(), Hsb::default());
                assert!((back.hue - hue).abs() < 1e-2, "{hsb:?} -> {back:?}");
                assert!(
                    (back.saturation - saturation).abs() < 1e-4,
                    "{hsb:?} -> {back:?}"
                );
                assert!(
                    (back.brightness - brightness).abs() < 1e-4,
                    "{hsb:?} -> {back:?}"
                );
                assert_eq!(back.alpha, 0.4);
            }
        }
    }

    #[test]
    fn greys_and_black_keep_what_the_colour_cannot_say() {
        let previous = Hsb {
            hue: 200.0,
            saturation: 0.6,
            brightness: 0.5,
            alpha: 1.0,
        };
        let grey = Hsb::from_color(Color::new([0.5, 0.5, 0.5, 1.0]), previous);
        assert_eq!(
            (grey.hue, grey.saturation, grey.brightness),
            (200.0, 0.0, 0.5)
        );
        let black = Hsb::from_color(Color::new([0.0, 0.0, 0.0, 1.0]), previous);
        assert_eq!(
            (black.hue, black.saturation, black.brightness),
            (200.0, 0.6, 0.0)
        );
    }

    #[test]
    fn out_of_range_values_are_clamped_and_hue_wraps() {
        let wild = Hsb {
            hue: 480.0,
            saturation: 3.0,
            brightness: -1.0,
            alpha: 2.0,
        };
        assert_eq!(wild.to_color().components, [0.0, 0.0, 0.0, 1.0]);
        let a = Hsb {
            hue: 480.0,
            saturation: 1.0,
            brightness: 1.0,
            alpha: 1.0,
        };
        let b = Hsb { hue: 120.0, ..a };
        assert!(close(a.to_color().components, b.to_color().components));
    }

    #[test]
    fn hex_is_six_digits_when_opaque_and_eight_otherwise() {
        assert_eq!(color_hex(Color::new([1.0, 0.5, 0.0, 1.0])), "#FF8000");
        assert_eq!(color_hex(Color::new([0.0, 0.0, 1.0, 0.5])), "#0000FF80");
    }

    #[test]
    fn hex_parses_the_forms_people_type() {
        let hex = |t: &str| parse_hex(t, 0.25).map(color_hex);
        assert_eq!(hex("#ff8000").as_deref(), Some("#FF800040"));
        assert_eq!(hex("FF8000").as_deref(), Some("#FF800040"));
        assert_eq!(hex("  #f80 ").as_deref(), Some("#FF880040"));
        assert_eq!(hex("#11223344").as_deref(), Some("#11223344"));
        for bad in [
            "", "#", "#12", "#1234", "#12345", "#1234567", "#gg0000", "ééé", "#12 456",
        ] {
            assert_eq!(hex(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_area_maps_points_to_values_and_back() {
        let rect = Rect::new(10.0, 20.0, 210.0, 140.0);
        let (hue, sat) = color_area_value_at(rect, Point::new(110.0, 20.0));
        assert_eq!((hue, sat), (180.0, 1.0));
        let (_, sat) = color_area_value_at(rect, Point::new(0.0, 500.0));
        assert_eq!(sat, 0.0);
        let (hue, _) = color_area_value_at(rect, Point::new(999.0, 0.0));
        assert!(hue < 360.0);
        let hsb = Hsb {
            hue: 90.0,
            saturation: 0.25,
            brightness: 1.0,
            alpha: 1.0,
        };
        let at = color_area_knob(rect, hsb);
        assert_eq!(color_area_value_at(rect, at), (90.0, 0.25));
    }

    #[test]
    fn layers_are_balanced() {
        let hsb = Hsb {
            hue: 10.0,
            saturation: 0.5,
            brightness: 0.5,
            alpha: 0.5,
        };
        for disabled in [false, true] {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let mut scene = Scene::new(60.0, 40.0);
                let well = ColorWell {
                    color: hsb.to_color(),
                    depth: 0.0,
                    disabled,
                    focused: !disabled,
                };
                color_well(
                    &mut scene,
                    Rect::new(4.0, 4.0, 50.0, 26.0),
                    well,
                    appearance,
                );
                assert_eq!(scene.open_layers(), 0);
                let mut scene = Scene::new(240.0, 160.0);
                let area = ColorArea {
                    hsb,
                    disabled,
                    focused: !disabled,
                };
                let rect = Rect::new(4.0, 4.0, 224.0, 124.0);
                color_area(&mut scene, rect, area, appearance);
                assert_eq!(scene.open_layers(), 0);
            }
        }
    }
}
