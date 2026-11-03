//! Window title bar: the brushed-grey gradient strip with rounded top corners, the three
//! traffic-light buttons and the centred, embossed title.
//!
//! The compositor draws server-side title bars from three slices so a resize never
//! re-renders anything: [`TitleBarSlice::Left`] (corner and buttons), a stretchable
//! [`TitleBarSlice::Middle`] column and [`TitleBarSlice::Right`] (corner). The title is
//! rendered on its own with [`title_text`] and placed at [`title_x`]. [`title_bar`] draws
//! the whole bar in one scene from the same pieces; golden tests use it.
//!
//! Everything is vector-drawn, with a light and a dark variant and a focused and an
//! unfocused state.

use vello_common::kurbo::{Cap, Circle, Line, Point, Rect, RoundedRect, RoundedRectRadii, Stroke};
use vello_common::peniko::{Color, Gradient};

use crate::Appearance;
use crate::scene::Scene;
use crate::text::TextLine;

// TODO(phase 2): metrics and colours move to neue-theme tokens.
/// Height of the title bar, in logical pixels.
pub const TITLE_BAR_HEIGHT: f64 = 22.0;
/// Width of the left slice: the rounded corner and the traffic lights.
pub const LEFT_SLICE_WIDTH: f64 = 68.0;
/// Width of the right slice: the rounded corner.
pub const RIGHT_SLICE_WIDTH: f64 = 8.0;
/// Width of the middle slice, stretched to fill the rest of the bar.
pub const MIDDLE_SLICE_WIDTH: f64 = 1.0;
/// Narrowest bar that shows every slice in full.
pub const MIN_WIDTH: f64 = LEFT_SLICE_WIDTH + MIDDLE_SLICE_WIDTH + RIGHT_SLICE_WIDTH;
/// Font size of the title.
pub const TITLE_FONT_SIZE: f32 = 13.0;
/// Baseline of the title, from the top of the bar.
const TITLE_BASELINE: f64 = 15.0;
/// Space between the title and the buttons or the right end of the bar.
const TITLE_MARGIN: f64 = 8.0;
/// Radius of the top corners.
const CORNER_RADIUS: f64 = 5.0;
/// Diameter of a traffic-light button.
const BUTTON_DIAMETER: f64 = 12.0;
/// Left edge of the first button.
const BUTTON_X: f64 = 8.0;
/// Distance between the left edges of neighbouring buttons.
const BUTTON_PITCH: f64 = 20.0;

/// Which window a title bar belongs to, as far as drawing is concerned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TitleBarStyle {
    /// Light or dark.
    pub appearance: Appearance,
    /// Whether the window is the focused (key) window.
    pub focused: bool,
    /// The pointer is over the traffic lights: all three show their glyphs and, in an
    /// unfocused window, their colour.
    pub hover: bool,
    /// The traffic light being pressed (the pointer went down on it and is still over it).
    pub pressed: Option<TrafficLight>,
}

impl TitleBarStyle {
    /// The same style with the pointer interaction removed: what the middle and right
    /// slices and the title look like, whatever the buttons do.
    pub fn without_interaction(self) -> Self {
        Self {
            hover: false,
            pressed: None,
            ..self
        }
    }
}

/// One of the three slices of a title bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TitleBarSlice {
    /// Left corner and traffic lights, [`LEFT_SLICE_WIDTH`] wide.
    Left,
    /// A [`MIDDLE_SLICE_WIDTH`] column of plain bar, stretched horizontally.
    Middle,
    /// Right corner, [`RIGHT_SLICE_WIDTH`] wide.
    Right,
}

impl TitleBarSlice {
    /// All slices, left to right.
    pub const ALL: [Self; 3] = [Self::Left, Self::Middle, Self::Right];

    /// Logical width of the slice before stretching.
    pub fn width(self) -> f64 {
        match self {
            Self::Left => LEFT_SLICE_WIDTH,
            Self::Middle => MIDDLE_SLICE_WIDTH,
            Self::Right => RIGHT_SLICE_WIDTH,
        }
    }

    /// Where the slice starts in a bar of the minimum width.
    fn offset(self) -> f64 {
        match self {
            Self::Left => 0.0,
            Self::Middle => LEFT_SLICE_WIDTH,
            Self::Right => LEFT_SLICE_WIDTH + MIDDLE_SLICE_WIDTH,
        }
    }
}

/// The three traffic-light buttons, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrafficLight {
    /// Close (red).
    Close,
    /// Minimise (yellow).
    Minimize,
    /// Zoom (green).
    Zoom,
}

impl TrafficLight {
    /// All buttons, left to right.
    pub const ALL: [Self; 3] = [Self::Close, Self::Minimize, Self::Zoom];

    /// The button's bounds, relative to the title bar's top-left corner.
    pub fn rect(self) -> Rect {
        let index = match self {
            Self::Close => 0.0,
            Self::Minimize => 1.0,
            Self::Zoom => 2.0,
        };
        let x = BUTTON_X + index * BUTTON_PITCH;
        let y = ((TITLE_BAR_HEIGHT - BUTTON_DIAMETER) / 2.0).floor();
        Rect::new(x, y, x + BUTTON_DIAMETER, y + BUTTON_DIAMETER)
    }
}

/// Colours of the bar for one style.
#[derive(Clone, Copy, Debug)]
struct BarPalette {
    /// Gradient top and bottom.
    fill: [Color; 2],
    /// One-pixel highlight along the top edge.
    highlight: Color,
    /// One-pixel line along the bottom edge, between the bar and the window content.
    separator: Color,
    text: Color,
    /// Embossed shadow one pixel below the text.
    text_shadow: Color,
}

const fn grey(v: f32) -> Color {
    Color::new([v, v, v, 1.0])
}

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::new([r, g, b, 1.0])
}

const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new([r, g, b, a])
}

impl BarPalette {
    fn new(style: TitleBarStyle) -> Self {
        match (style.appearance, style.focused) {
            (Appearance::Light, true) => Self {
                fill: [grey(0.910), grey(0.780)],
                highlight: rgba(1.0, 1.0, 1.0, 0.75),
                separator: grey(0.400),
                text: grey(0.165),
                text_shadow: rgba(1.0, 1.0, 1.0, 0.55),
            },
            (Appearance::Light, false) => Self {
                fill: [grey(0.960), grey(0.905)],
                highlight: rgba(1.0, 1.0, 1.0, 0.85),
                separator: grey(0.640),
                text: grey(0.520),
                text_shadow: rgba(1.0, 1.0, 1.0, 0.50),
            },
            (Appearance::Dark, true) => Self {
                fill: [grey(0.300), grey(0.205)],
                highlight: rgba(1.0, 1.0, 1.0, 0.16),
                separator: grey(0.060),
                text: grey(0.880),
                text_shadow: rgba(0.0, 0.0, 0.0, 0.55),
            },
            (Appearance::Dark, false) => Self {
                fill: [grey(0.235), grey(0.195)],
                highlight: rgba(1.0, 1.0, 1.0, 0.09),
                separator: grey(0.090),
                text: grey(0.520),
                text_shadow: rgba(0.0, 0.0, 0.0, 0.40),
            },
        }
    }
}

/// Colours of one traffic-light button.
#[derive(Clone, Copy, Debug)]
struct ButtonPalette {
    /// Rim gradient, top and bottom.
    rim: [Color; 2],
    /// Body glow: centre (low in the button) and edge.
    body: [Color; 2],
    /// Gloss alpha at its top and bottom.
    gloss: [f32; 2],
    /// Etched highlight below the button.
    etch: Color,
}

impl ButtonPalette {
    fn new(button: TrafficLight, style: TitleBarStyle) -> Self {
        let dark = style.appearance == Appearance::Dark;
        let etch = if dark {
            rgba(1.0, 1.0, 1.0, 0.06)
        } else {
            rgba(1.0, 1.0, 1.0, 0.60)
        };
        // Hovering the buttons lights them even in an unfocused window.
        if !(style.focused || style.hover) {
            return if dark {
                Self {
                    rim: [grey(0.16), grey(0.12)],
                    body: [grey(0.42), grey(0.30)],
                    gloss: [0.22, 0.02],
                    etch,
                }
            } else {
                Self {
                    rim: [grey(0.66), grey(0.58)],
                    body: [grey(0.90), grey(0.80)],
                    gloss: [0.70, 0.10],
                    etch,
                }
            };
        }
        let (rim, body) = match button {
            TrafficLight::Close => (
                [rgb(0.62, 0.13, 0.10), rgb(0.48, 0.07, 0.05)],
                [rgb(1.00, 0.52, 0.45), rgb(0.88, 0.22, 0.17)],
            ),
            TrafficLight::Minimize => (
                [rgb(0.66, 0.44, 0.04), rgb(0.52, 0.32, 0.02)],
                [rgb(1.00, 0.88, 0.45), rgb(0.93, 0.64, 0.10)],
            ),
            TrafficLight::Zoom => (
                [rgb(0.10, 0.45, 0.10), rgb(0.05, 0.34, 0.06)],
                [rgb(0.62, 0.92, 0.45), rgb(0.27, 0.70, 0.18)],
            ),
        };
        let (rim, body) = if dark {
            (rim.map(|c| darken(c, 0.75)), body.map(|c| darken(c, 0.85)))
        } else {
            (rim, body)
        };
        let palette = Self {
            rim,
            body,
            gloss: if dark { [0.45, 0.04] } else { [0.70, 0.06] },
            etch,
        };
        if style.pressed == Some(button) {
            palette.pressed()
        } else {
            palette
        }
    }

    /// The darker, flatter look of a button held down.
    fn pressed(self) -> Self {
        Self {
            rim: self.rim.map(|c| darken(c, 0.85)),
            body: self.body.map(|c| darken(c, 0.72)),
            gloss: self.gloss.map(|a| a * 0.4),
            etch: self.etch,
        }
    }
}

fn darken(color: Color, factor: f32) -> Color {
    let [r, g, b, a] = color.components;
    Color::new([r * factor, g * factor, b * factor, a])
}

fn vertical(y0: f64, y1: f64, stops: &[(f32, Color)]) -> Gradient {
    Gradient::new_linear(Point::new(0.0, y0), Point::new(0.0, y1)).with_stops(stops)
}

/// Draw the bar background (no buttons, no title) `width` wide at the origin.
fn background(scene: &mut Scene, width: f64, style: TitleBarStyle) {
    let p = BarPalette::new(style);
    let h = TITLE_BAR_HEIGHT;
    let shape = RoundedRect::from_rect(
        Rect::new(0.0, 0.0, width, h),
        RoundedRectRadii::new(CORNER_RADIUS, CORNER_RADIUS, 0.0, 0.0),
    );
    scene.push_clip(&shape);
    scene.fill(
        &Rect::new(0.0, 0.0, width, h),
        vertical(0.0, h, &[(0.0, p.fill[0]), (1.0, p.fill[1])]),
    );
    scene.fill(&Rect::new(0.0, 0.0, width, 1.0), p.highlight);
    scene.fill(&Rect::new(0.0, h - 1.0, width, h), p.separator);
    scene.pop_layer();
}

/// Draw one traffic-light button.
fn button(scene: &mut Scene, which: TrafficLight, style: TitleBarStyle) {
    let p = ButtonPalette::new(which, style);
    let rect = which.rect();
    let center = rect.center();
    let r = BUTTON_DIAMETER / 2.0;

    // Etched highlight peeking out below the rim.
    scene.fill(&Circle::new(center + (0.0, 0.75), r), p.etch);
    // Rim.
    scene.fill(
        &Circle::new(center, r),
        vertical(rect.y0, rect.y1, &[(0.0, p.rim[0]), (1.0, p.rim[1])]),
    );
    // Body: a glow rising from the bottom.
    let inner = r - 1.0;
    #[allow(clippy::cast_possible_truncation, reason = "small logical radius")]
    let glow = Gradient::new_radial(center + (0.0, inner * 0.55), (inner * 1.35) as f32)
        .with_stops([(0.0, p.body[0]), (1.0, p.body[1])]);
    scene.fill(&Circle::new(center, inner), glow);
    // Gloss over the upper half.
    let white = |a: f32| Color::WHITE.with_alpha(a);
    let gloss = vello_common::kurbo::Ellipse::new(
        center - (0.0, inner * 0.45),
        (inner * 0.70, inner * 0.45),
        0.0,
    );
    scene.fill(
        &gloss,
        vertical(
            center.y - inner * 0.9,
            center.y,
            &[(0.0, white(p.gloss[0])), (1.0, white(p.gloss[1]))],
        ),
    );
    if style.hover {
        glyph(scene, which, center, p.rim[1]);
    }
}

/// Half the length of a glyph stroke, from its centre.
const GLYPH_REACH: f64 = 2.6;
/// Line width of the glyphs.
const GLYPH_WIDTH: f64 = 1.5;

/// The symbol a button shows while the pointer is over the traffic lights: a cross, a
/// bar and a plus.
fn glyph(scene: &mut Scene, which: TrafficLight, center: Point, color: Color) {
    let stroke = Stroke::new(GLYPH_WIDTH).with_caps(Cap::Round);
    let r = GLYPH_REACH;
    let mut line = |dx0: f64, dy0: f64, dx1: f64, dy1: f64| {
        scene.stroke(
            &Line::new(center + (dx0, dy0), center + (dx1, dy1)),
            &stroke,
            color,
        );
    };
    match which {
        TrafficLight::Close => {
            let d = r * 0.85;
            line(-d, -d, d, d);
            line(-d, d, d, -d);
        }
        TrafficLight::Minimize => line(-r, 0.0, r, 0.0),
        TrafficLight::Zoom => {
            line(-r, 0.0, r, 0.0);
            line(0.0, -r, 0.0, r);
        }
    }
}

/// Draw `slice` with its top-left corner at the origin. Every slice is cut out of the same
/// [`MIN_WIDTH`] bar, so slices placed side by side (the middle one stretched) look exactly
/// like [`title_bar`] without the title.
pub fn title_bar_slice(scene: &mut Scene, slice: TitleBarSlice, style: TitleBarStyle) {
    let width = slice.width();
    scene.push_clip(&Rect::new(0.0, 0.0, width, TITLE_BAR_HEIGHT));
    scene.with_transform(
        vello_common::kurbo::Affine::translate((-slice.offset(), 0.0)),
        |scene| {
            background(scene, MIN_WIDTH, style);
            if slice == TitleBarSlice::Left {
                for which in TrafficLight::ALL {
                    button(scene, which, style);
                }
            }
        },
    );
    scene.pop_layer();
}

/// A scene holding just `slice`.
pub fn title_bar_slice_scene(slice: TitleBarSlice, style: TitleBarStyle) -> Scene {
    let mut scene = Scene::new(slice.width(), TITLE_BAR_HEIGHT);
    title_bar_slice(&mut scene, slice, style);
    scene
}

/// Widest the title may be in a bar `bar_width` wide.
pub fn title_max_width(bar_width: f64) -> f64 {
    (bar_width - LEFT_SLICE_WIDTH - RIGHT_SLICE_WIDTH - TITLE_MARGIN).max(0.0)
}

/// Where a title `text_width` wide starts in a bar `bar_width` wide: centred on the bar,
/// pushed right if it would cover the buttons. Whole logical pixels, so text stays crisp.
pub fn title_x(bar_width: f64, text_width: f64) -> f64 {
    let centred = ((bar_width - text_width) / 2.0).round();
    let min = LEFT_SLICE_WIDTH;
    let max = (bar_width - RIGHT_SLICE_WIDTH - TITLE_MARGIN - text_width).round();
    centred.min(max).max(min)
}

/// Width of the scene [`title_text`] draws into for `line`: the text plus its shadow.
pub fn title_text_width(line: &TextLine) -> f64 {
    line.width().ceil() + 1.0
}

/// Draw the embossed title `line` with the left end of the bar's text area at the origin;
/// the scene should be [`title_text_width`] × [`TITLE_BAR_HEIGHT`].
pub fn title_text(scene: &mut Scene, line: &TextLine, style: TitleBarStyle) {
    let p = BarPalette::new(style);
    scene.draw_text(line, Point::new(0.0, TITLE_BASELINE + 1.0), p.text_shadow);
    scene.draw_text(line, Point::new(0.0, TITLE_BASELINE), p.text);
}

/// A scene holding just the title.
pub fn title_text_scene(line: &TextLine, style: TitleBarStyle) -> Scene {
    let mut scene = Scene::new(title_text_width(line).max(1.0), TITLE_BAR_HEIGHT);
    title_text(&mut scene, line, style);
    scene
}

/// Draw a whole title bar `width` wide (at least [`MIN_WIDTH`]) at the origin, with
/// `title` already laid out (truncated to [`title_max_width`] by the caller).
pub fn title_bar(scene: &mut Scene, width: f64, title: &TextLine, style: TitleBarStyle) {
    let width = width.max(MIN_WIDTH);
    title_bar_slice(scene, TitleBarSlice::Left, style);
    let middle = width - LEFT_SLICE_WIDTH - RIGHT_SLICE_WIDTH;
    scene.with_transform(
        vello_common::kurbo::Affine::translate((LEFT_SLICE_WIDTH, 0.0))
            * vello_common::kurbo::Affine::scale_non_uniform(middle / MIDDLE_SLICE_WIDTH, 1.0),
        |scene| title_bar_slice(scene, TitleBarSlice::Middle, style),
    );
    scene.with_transform(
        vello_common::kurbo::Affine::translate((width - RIGHT_SLICE_WIDTH, 0.0)),
        |scene| title_bar_slice(scene, TitleBarSlice::Right, style),
    );
    let x = title_x(width, title.width());
    scene.with_transform(vello_common::kurbo::Affine::translate((x, 0.0)), |scene| {
        title_text(scene, title, style);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{CpuRenderer, Quality};

    #[test]
    fn title_stays_clear_of_the_buttons() {
        assert_eq!(title_x(400.0, 100.0), 150.0);
        assert_eq!(title_x(200.0, 100.0), LEFT_SLICE_WIDTH);
        // Wider than the room left after the buttons: it starts right after them.
        assert_eq!(title_x(150.0, 200.0), LEFT_SLICE_WIDTH);
        // Long-ish title in a mid-sized bar: pushed left of the right margin, not centred.
        let x = title_x(260.0, 160.0);
        assert!(x + 160.0 <= 260.0 - RIGHT_SLICE_WIDTH - TITLE_MARGIN, "{x}");
        assert!(x >= LEFT_SLICE_WIDTH);
    }

    #[test]
    fn buttons_are_inside_the_left_slice() {
        for b in TrafficLight::ALL {
            let r = b.rect();
            assert!(r.x0 >= 0.0 && r.x1 <= LEFT_SLICE_WIDTH && r.y1 <= TITLE_BAR_HEIGHT);
        }
    }

    /// Slices rendered separately match the corresponding columns of the full bar.
    #[test]
    fn slices_match_the_full_bar() {
        let style = TitleBarStyle {
            appearance: Appearance::Light,
            focused: true,
            ..TitleBarStyle::default()
        };
        let mut renderer = CpuRenderer::new(Quality::Reference);
        let mut full = Scene::new(MIN_WIDTH, TITLE_BAR_HEIGHT);
        title_bar(&mut full, MIN_WIDTH, &TextLine::default(), style);
        let full = renderer.render(&full, 2.0).unwrap();
        for slice in TitleBarSlice::ALL {
            let part = renderer
                .render(&title_bar_slice_scene(slice, style), 2.0)
                .unwrap();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let x0 = (slice.offset() * 2.0) as usize;
            for y in 0..usize::from(part.height()) {
                for x in 0..usize::from(part.width()) {
                    let a = part.data()[y * usize::from(part.width()) + x];
                    let b = full.data()[y * usize::from(full.width()) + x0 + x];
                    let diff = [
                        a.r.abs_diff(b.r),
                        a.g.abs_diff(b.g),
                        a.b.abs_diff(b.b),
                        a.a.abs_diff(b.a),
                    ];
                    assert!(
                        diff.iter().all(|d| *d <= 2),
                        "{slice:?} differs at ({x}, {y}): {a:?} vs {b:?}"
                    );
                }
            }
        }
    }
}
