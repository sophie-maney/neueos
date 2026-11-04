//! Split-view dividers: a thin line or a thick bar with a grip of dots.
//!
//! Colours and metrics come from [`neue_theme::materials::split`].

use neue_theme::materials::split::{GRIP_DOT, GRIP_DOTS, GRIP_PITCH, Palette, THICK, THIN};
use vello_common::kurbo::{Circle, Point, Rect, Vec2};
use vello_common::peniko::Gradient;

use crate::Variant;
use crate::scene::Scene;

/// How a split view's divider looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DividerStyle {
    /// A one-pixel line.
    #[default]
    Thin,
    /// A bar with a grip, wide enough to be a comfortable target.
    Thick,
}

impl DividerStyle {
    /// The divider's width across the split, in logical pixels.
    pub const fn width(self) -> f64 {
        match self {
            Self::Thin => THIN,
            Self::Thick => THICK,
        }
    }
}

/// Draws a divider filling `rect`. `vertical` is whether it is a vertical bar (the panes
/// sit side by side), and so which way the grip and the gradient run.
pub fn split_divider(
    scene: &mut Scene,
    rect: Rect,
    style: DividerStyle,
    vertical: bool,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into());
    if style == DividerStyle::Thin {
        scene.fill(&rect, p.line);
        return;
    }
    // The gradient runs across the bar, from the first pane to the second.
    let (from, to) = if vertical {
        (Point::new(rect.x0, 0.0), Point::new(rect.x1, 0.0))
    } else {
        (Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
    };
    scene.fill(
        &rect,
        Gradient::new_linear(from, to).with_stops([(0.0, p.bar[0]), (1.0, p.bar[1])]),
    );
    let edge = THIN;
    let (first, second) = if vertical {
        (
            Rect::new(rect.x0, rect.y0, rect.x0 + edge, rect.y1),
            Rect::new(rect.x1 - edge, rect.y0, rect.x1, rect.y1),
        )
    } else {
        (
            Rect::new(rect.x0, rect.y0, rect.x1, rect.y0 + edge),
            Rect::new(rect.x0, rect.y1 - edge, rect.x1, rect.y1),
        )
    };
    scene.fill(&first, p.edge);
    scene.fill(&second, p.edge);

    // The grip: dots along the divider, centred, each with a highlight just below it.
    let centre = rect.center();
    #[expect(clippy::cast_precision_loss, reason = "a handful of dots")]
    let start = -(GRIP_DOTS as f64 - 1.0) / 2.0 * GRIP_PITCH;
    for i in 0..GRIP_DOTS {
        #[expect(clippy::cast_precision_loss, reason = "a handful of dots")]
        let along = start + i as f64 * GRIP_PITCH;
        let offset = if vertical {
            Vec2::new(0.0, along)
        } else {
            Vec2::new(along, 0.0)
        };
        let at = centre + offset;
        scene.fill(
            &Circle::new(at + Vec2::new(0.0, 1.0), GRIP_DOT / 2.0),
            p.grip_highlight,
        );
        scene.fill(&Circle::new(at, GRIP_DOT / 2.0), p.grip);
    }
}
