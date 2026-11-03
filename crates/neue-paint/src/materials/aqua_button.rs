//! The Aqua gel push button (the blue default button).
//!
//! Built from vector layers only: a soft shadow and etched highlight below, a navy rim, a
//! four-stop gel body with a hard midline, and a white gloss over the upper half.

use vello_common::kurbo::{Point, Rect, RoundedRect, RoundedRectRadii, Vec2};
use vello_common::peniko::{Color, Gradient};

use crate::Appearance;
use crate::scene::Scene;

/// Corner radius of the rim, in logical pixels.
// TODO(phase 2): metrics and colours move to neue-theme tokens.
const RADIUS: f64 = 4.0;
/// Rim thickness.
const RIM: f64 = 1.0;

/// Interaction state of a push button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonState {
    /// At rest.
    #[default]
    Normal,
    /// Held down.
    Pressed,
}

/// Colours of one appearance × state combination.
#[derive(Clone, Copy, Debug)]
struct Palette {
    shadow: Color,
    etch: Color,
    rim: [Color; 2],
    /// Top, above the midline, below the midline, bottom.
    gel: [Color; 4],
    /// Gloss alpha at its top and bottom.
    gloss: [f32; 2],
}

const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::new([r, g, b, 1.0])
}

const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new([r, g, b, a])
}

impl Palette {
    fn new(appearance: Appearance, state: ButtonState) -> Self {
        use Appearance::{Dark, Light};
        use ButtonState::{Normal, Pressed};
        match (appearance, state) {
            (Light, Normal) => Self {
                shadow: rgba(0.0, 0.0, 0.0, 0.28),
                etch: rgba(1.0, 1.0, 1.0, 0.55),
                rim: [rgb(0.24, 0.38, 0.66), rgb(0.14, 0.26, 0.54)],
                gel: [
                    rgb(0.74, 0.87, 0.99),
                    rgb(0.51, 0.72, 0.96),
                    rgb(0.33, 0.58, 0.93),
                    rgb(0.63, 0.83, 1.00),
                ],
                gloss: [0.55, 0.08],
            },
            (Light, Pressed) => Self {
                shadow: rgba(0.0, 0.0, 0.0, 0.20),
                etch: rgba(1.0, 1.0, 1.0, 0.55),
                rim: [rgb(0.16, 0.28, 0.54), rgb(0.09, 0.19, 0.44)],
                gel: [
                    rgb(0.52, 0.70, 0.92),
                    rgb(0.32, 0.54, 0.87),
                    rgb(0.18, 0.42, 0.81),
                    rgb(0.43, 0.67, 0.94),
                ],
                gloss: [0.35, 0.04],
            },
            (Dark, Normal) => Self {
                shadow: rgba(0.0, 0.0, 0.0, 0.60),
                etch: rgba(1.0, 1.0, 1.0, 0.07),
                rim: [rgb(0.09, 0.15, 0.31), rgb(0.04, 0.08, 0.20)],
                gel: [
                    rgb(0.46, 0.65, 0.91),
                    rgb(0.27, 0.49, 0.83),
                    rgb(0.14, 0.35, 0.75),
                    rgb(0.33, 0.57, 0.89),
                ],
                gloss: [0.38, 0.05],
            },
            (Dark, Pressed) => Self {
                shadow: rgba(0.0, 0.0, 0.0, 0.50),
                etch: rgba(1.0, 1.0, 1.0, 0.07),
                rim: [rgb(0.05, 0.10, 0.24), rgb(0.02, 0.05, 0.15)],
                gel: [
                    rgb(0.33, 0.50, 0.78),
                    rgb(0.18, 0.36, 0.70),
                    rgb(0.09, 0.25, 0.61),
                    rgb(0.23, 0.44, 0.76),
                ],
                gloss: [0.22, 0.03],
            },
        }
    }
}

fn vertical(rect: Rect, stops: &[(f32, Color)]) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1)).with_stops(stops)
}

/// Draw an Aqua push button filling `rect` (logical pixels; 22 px tall is the regular size).
///
/// The button has no label yet: text rendering arrives with neue-kit.
pub fn aqua_button(scene: &mut Scene, rect: Rect, state: ButtonState, appearance: Appearance) {
    let p = Palette::new(appearance, state);
    let radius = RADIUS.min(rect.height() / 2.0);

    // Etched highlight one pixel below the button, then the drop shadow.
    scene.fill(
        &RoundedRect::from_rect(rect + Vec2::new(0.0, 1.0), radius),
        p.etch,
    );
    scene.blurred_rounded_rect(rect + Vec2::new(0.0, 0.5), radius, 0.75, p.shadow);

    // Rim.
    let rim = RoundedRect::from_rect(rect, radius);
    scene.fill(&rim, vertical(rect, &[(0.0, p.rim[0]), (1.0, p.rim[1])]));

    // Gel body with a hard step at the midline.
    let body_rect = rect.inset(-RIM);
    let body = RoundedRect::from_rect(body_rect, (radius - RIM).max(0.0));
    let [top, hi, lo, bottom] = p.gel;
    scene.fill(
        &body,
        vertical(
            body_rect,
            &[(0.0, top), (0.5, hi), (0.5, lo), (1.0, bottom)],
        ),
    );

    // Gloss over the upper half, clipped to the body.
    scene.push_clip(&body);
    let gloss_rect = Rect::new(
        body_rect.x0 + 1.0,
        body_rect.y0 + 0.5,
        body_rect.x1 - 1.0,
        body_rect.center().y,
    );
    let gloss_radius = (radius - RIM - 1.0).max(0.0);
    let gloss = RoundedRect::from_rect(
        gloss_rect,
        RoundedRectRadii::new(gloss_radius, gloss_radius, 0.0, 0.0),
    );
    let white = |a: f32| Color::WHITE.with_alpha(a);
    scene.fill(
        &gloss,
        vertical(
            gloss_rect,
            &[(0.0, white(p.gloss[0])), (1.0, white(p.gloss[1]))],
        ),
    );
    scene.pop_layer();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Command;

    #[test]
    fn layers_are_balanced() {
        let mut scene = Scene::new(100.0, 30.0);
        aqua_button(
            &mut scene,
            Rect::new(4.0, 4.0, 96.0, 26.0),
            ButtonState::Pressed,
            Appearance::Dark,
        );
        assert_eq!(scene.open_layers(), 0);
        assert!(matches!(scene.commands().last(), Some(Command::PopLayer)));
    }
}
