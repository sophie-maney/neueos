//! The stepper: two small gel halves, an up arrow over a down arrow, for the number field
//! beside it.
//!
//! Colours and metrics come from [`neue_theme::materials::stepper`] and the push button's
//! neutral gel.

use neue_theme::materials::aqua_button::Palette;
use neue_theme::materials::stepper::{
    ARROW_HEIGHT, ARROW_WIDTH, DISABLED_OPACITY, DIVIDER, PRESSED_DIM, RADIUS, arrow, divider,
};
use vello_common::kurbo::{BezPath, Point, Rect, RoundedRect};
use vello_common::peniko::Color;

use super::aqua_button::draw;
use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// One of the stepper's halves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StepperPart {
    /// The upper half: increments.
    Up,
    /// The lower half: decrements.
    Down,
}

/// Everything that changes how a stepper looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stepper {
    /// The half held down by the pointer.
    pub pressed: Option<StepperPart>,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring.
    pub focused: bool,
}

/// The outline of a stepper filling `rect`: the shape focus rings follow.
pub fn stepper_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.width() / 2.0))
}

/// Which half of a stepper filling `rect` the height `y` falls in: the lower half from the
/// midline down.
pub fn stepper_part_at(rect: Rect, y: f64) -> StepperPart {
    if y < rect.center().y {
        StepperPart::Up
    } else {
        StepperPart::Down
    }
}

/// Draw a stepper filling `rect` (logical pixels;
/// [`neue_theme::metrics::control::STEPPER_WIDTH`] × `STEPPER_HEIGHT` is the regular size).
/// The focus ring reaches [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn stepper(scene: &mut Scene, rect: Rect, stepper: Stepper, variant: impl Into<Variant>) {
    let variant = variant.into();
    let outline = stepper_outline(rect);
    if stepper.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    draw(
        scene,
        rect,
        outline.radii().top_left,
        &Palette::plain(variant),
    );

    let mid = rect.center().y;
    scene.push_clip(&outline);
    if let Some(part) = stepper.pressed {
        let half = match part {
            StepperPart::Up => Rect::new(rect.x0, rect.y0, rect.x1, mid),
            StepperPart::Down => Rect::new(rect.x0, mid, rect.x1, rect.y1),
        };
        scene.fill(&half, Color::new([0.0, 0.0, 0.0, 1.0 - PRESSED_DIM]));
    }
    scene.fill(
        &Rect::new(rect.x0, mid - DIVIDER / 2.0, rect.x1, mid + DIVIDER / 2.0),
        divider(variant),
    );
    scene.pop_layer();

    let colour = arrow(variant);
    let cx = rect.center().x;
    let quarter = rect.height() / 4.0;
    for (cy, up) in [(rect.y0 + quarter, true), (rect.y1 - quarter, false)] {
        let (tip, base) = if up {
            (cy - ARROW_HEIGHT / 2.0, cy + ARROW_HEIGHT / 2.0)
        } else {
            (cy + ARROW_HEIGHT / 2.0, cy - ARROW_HEIGHT / 2.0)
        };
        let mut path = BezPath::new();
        path.move_to(Point::new(cx, tip));
        path.line_to(Point::new(cx + ARROW_WIDTH / 2.0, base));
        path.line_to(Point::new(cx - ARROW_WIDTH / 2.0, base));
        path.close_path();
        scene.fill(&path, colour);
    }

    if stepper.disabled {
        scene.pop_layer();
    } else if stepper.focused {
        focus_ring(scene, outline, variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    fn rect() -> Rect {
        Rect::new(2.0, 2.0, 15.0, 24.0)
    }

    #[test]
    fn the_midline_splits_the_halves() {
        assert_eq!(stepper_part_at(rect(), 5.0), StepperPart::Up);
        assert_eq!(stepper_part_at(rect(), 12.9), StepperPart::Up);
        assert_eq!(stepper_part_at(rect(), 13.0), StepperPart::Down);
        assert_eq!(stepper_part_at(rect(), 23.0), StepperPart::Down);
    }

    #[test]
    fn layers_are_balanced() {
        for disabled in [false, true] {
            for pressed in [None, Some(StepperPart::Up), Some(StepperPart::Down)] {
                let mut scene = Scene::new(20.0, 30.0);
                let s = Stepper {
                    pressed,
                    disabled,
                    focused: true,
                };
                stepper(&mut scene, rect(), s, Appearance::Light);
                assert_eq!(scene.open_layers(), 0);
            }
        }
    }
}
