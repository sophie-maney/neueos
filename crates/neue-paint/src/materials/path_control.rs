//! The path control, standard style: segments of text separated by chevrons. The segment
//! under the pointer gets a soft rounded fill, darker while held; the keyboard-focused one
//! gets the focus ring. The text is the caller's.
//!
//! Colours and metrics come from [`neue_theme::materials::path_control`].

use neue_theme::materials::path_control::{
    CHEVRON_HEIGHT, CHEVRON_SLOT, CHEVRON_STROKE, CHEVRON_WIDTH, DISABLED_OPACITY, RADIUS,
    SEGMENT_INSET, chevron, hover_fill, pressed_fill,
};
use vello_common::kurbo::{BezPath, Cap, Join, Point, Rect, RoundedRect, Stroke};

use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a path control looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathControl<'a> {
    /// The segments' rectangles, left to right ([`path_segment_rects`]).
    pub segments: &'a [Rect],
    /// The segment under the pointer.
    pub hovered: Option<usize>,
    /// The segment held down by the pointer.
    pub pressed: Option<usize>,
    /// The segment with the keyboard focus ring.
    pub focused: Option<usize>,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
}

/// The rectangles of segments `widths` wide (their text plus padding) inside `rect`: left
/// to right from `rect`'s left edge, [`CHEVRON_SLOT`] apart, inset from its top and bottom.
pub fn path_segment_rects(rect: Rect, widths: &[f64]) -> Vec<Rect> {
    let mut x = rect.x0;
    let mut rects = Vec::with_capacity(widths.len());
    for w in widths {
        rects.push(Rect::new(
            x,
            rect.y0 + SEGMENT_INSET,
            x + w,
            rect.y1 - SEGMENT_INSET,
        ));
        x += w + CHEVRON_SLOT;
    }
    rects
}

/// The chevron in the slot after `segment`.
fn chevron_path(segment: Rect) -> BezPath {
    let centre = Point::new(segment.x1 + CHEVRON_SLOT / 2.0, segment.center().y);
    let mut path = BezPath::new();
    path.move_to(Point::new(
        centre.x - CHEVRON_WIDTH / 2.0,
        centre.y - CHEVRON_HEIGHT / 2.0,
    ));
    path.line_to(Point::new(centre.x + CHEVRON_WIDTH / 2.0, centre.y));
    path.line_to(Point::new(
        centre.x - CHEVRON_WIDTH / 2.0,
        centre.y + CHEVRON_HEIGHT / 2.0,
    ));
    path
}

/// The outline of one segment: the shape its fill and focus ring follow.
pub fn path_segment_outline(segment: Rect) -> RoundedRect {
    RoundedRect::from_rect(segment, RADIUS)
}

/// Draw a path control (logical pixels). The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside the focused segment.
pub fn path_control(scene: &mut Scene, control: PathControl<'_>, variant: impl Into<Variant>) {
    let variant = variant.into();
    if control.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }

    for (i, &segment) in control.segments.iter().enumerate() {
        let outline = path_segment_outline(segment);
        if control.pressed == Some(i) {
            scene.fill(&outline, pressed_fill(variant));
        } else if control.hovered == Some(i) {
            scene.fill(&outline, hover_fill(variant));
        }
    }

    let stroke = Stroke::new(CHEVRON_STROKE)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    for &segment in control
        .segments
        .iter()
        .take(control.segments.len().saturating_sub(1))
    {
        scene.stroke(&chevron_path(segment), &stroke, chevron(variant));
    }

    if control.disabled {
        scene.pop_layer();
    } else if let Some(&segment) = control.focused.and_then(|i| control.segments.get(i)) {
        focus_ring(scene, path_segment_outline(segment), variant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn segments_sit_a_chevron_slot_apart() {
        let rects = path_segment_rects(Rect::new(4.0, 0.0, 200.0, 22.0), &[30.0, 40.0]);
        assert_eq!(rects[0], Rect::new(4.0, 2.0, 34.0, 20.0));
        assert_eq!(rects[1].x0, 34.0 + CHEVRON_SLOT);
        assert_eq!(rects[1].width(), 40.0);
    }

    #[test]
    fn layers_are_balanced() {
        let rects = path_segment_rects(Rect::new(4.0, 4.0, 200.0, 26.0), &[30.0, 40.0, 20.0]);
        for disabled in [false, true] {
            let mut scene = Scene::new(210.0, 30.0);
            let control = PathControl {
                segments: &rects,
                hovered: Some(0),
                pressed: Some(1),
                focused: Some(2),
                disabled,
            };
            path_control(&mut scene, control, Appearance::Dark);
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn no_segments_draws_nothing() {
        let mut scene = Scene::new(10.0, 10.0);
        let control = PathControl {
            segments: &[],
            hovered: None,
            pressed: None,
            focused: None,
            disabled: false,
        };
        path_control(&mut scene, control, Appearance::Light);
        assert!(scene.commands().is_empty());
    }
}
