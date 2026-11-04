//! The well of text, search and secure fields: a hairline border, darker along the top,
//! around the content background, with an inner shadow under the top edge. Search fields
//! are rounded and carry a magnifier, and a cancel button while they hold text.
//!
//! Colours and metrics come from [`neue_theme::materials::text_field`]. The text, the
//! selection and the caret are the widget's: it lays them out.

use neue_theme::materials::text_field::{
    BORDER, CANCEL, CANCEL_CROSS, DISABLED_OPACITY, MAGNIFIER_LENS, MAGNIFIER_STROKE, PRESSED_DIM,
    Palette, SHADOW,
};
use vello_common::kurbo::{Cap, Circle, Line, Point, Rect, RoundedRect, Stroke, Vec2};
use vello_common::peniko::Color;

use super::aqua_button::vertical;
use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// The shape of a field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FieldShape {
    /// Square corners: text and secure fields.
    #[default]
    Square,
    /// Fully rounded ends: search fields.
    Round,
}

/// Everything that changes how a field's well looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextField {
    pub shape: FieldShape,
    /// Draws the keyboard focus ring. Pass `false` while the window is inactive.
    pub focused: bool,
    /// Greyed out: the well at reduced opacity, no focus ring.
    pub disabled: bool,
}

/// The outline of a field filling `rect`: the shape focus rings follow.
pub fn text_field_outline(rect: Rect, shape: FieldShape) -> RoundedRect {
    let radius = match shape {
        FieldShape::Square => 0.0,
        FieldShape::Round => rect.height() / 2.0,
    };
    RoundedRect::from_rect(rect, radius)
}

/// The inside of a field filling `rect`, inside its border: where text may be drawn (the
/// widget clips its text to this).
pub fn text_field_inside(rect: Rect, shape: FieldShape) -> RoundedRect {
    let outline = text_field_outline(rect, shape);
    let radius = (outline.radii().top_left - BORDER).max(0.0);
    RoundedRect::from_rect(rect.inset(-BORDER), radius)
}

/// Draw the well of a field filling `rect` (logical pixels;
/// [`neue_theme::metrics::control::REGULAR_HEIGHT`] tall is the regular size). The focus
/// ring reaches [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn text_field(scene: &mut Scene, rect: Rect, field: TextField, variant: impl Into<Variant>) {
    let variant = variant.into();
    let p = Palette::new(variant);
    let outline = text_field_outline(rect, field.shape);
    if field.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    // The border: darker along the top, the sides blending into the lighter bottom.
    let top = (BORDER / rect.height()) as f32;
    scene.fill(
        &outline,
        vertical(
            rect,
            &[
                (0.0, p.border_top),
                (top.min(1.0), p.border),
                (1.0, p.border),
            ],
        ),
    );
    let inside = text_field_inside(rect, field.shape);
    scene.fill(&inside, p.fill);
    // The inner shadow under the top edge.
    let band = Rect::new(
        inside.rect().x0,
        inside.rect().y0,
        inside.rect().x1,
        inside.rect().y0 + SHADOW,
    );
    scene.push_clip(&inside);
    scene.fill(
        &band,
        vertical(band, &[(0.0, p.shadow), (1.0, p.shadow.with_alpha(0.0))]),
    );
    scene.pop_layer();
    if field.disabled {
        scene.pop_layer();
    } else if field.focused {
        focus_ring(scene, outline, variant);
    }
}

/// Draw a search field's magnifier centred on `center`.
pub fn magnifier(scene: &mut Scene, center: Point, variant: impl Into<Variant>) {
    let colour = Palette::new(variant.into()).glyph;
    let r = MAGNIFIER_LENS / 2.0;
    // The lens sits up and left of centre so lens and handle together are centred.
    let lens = center - Vec2::new(1.0, 1.0);
    let stroke = Stroke::new(MAGNIFIER_STROKE).with_caps(Cap::Round);
    scene.stroke(&Circle::new(lens, r), &stroke, colour);
    let d = r * std::f64::consts::FRAC_1_SQRT_2;
    scene.stroke(
        &Line::new(lens + Vec2::new(d, d), lens + Vec2::new(d + 3.0, d + 3.0)),
        &Stroke::new(MAGNIFIER_STROKE + 0.5).with_caps(Cap::Round),
        colour,
    );
}

/// The cancel button of a search field filling `rect`: a [`CANCEL`] disc centred in the
/// round end on the right.
pub fn cancel_button_rect(rect: Rect) -> Rect {
    let c = Point::new(rect.x1 - rect.height() / 2.0, rect.center().y);
    Rect::from_center_size(c, (CANCEL, CANCEL))
}

/// Draw a search field's cancel button (a disc with a cross) in `rect` (from
/// [`cancel_button_rect`]), darker while `pressed`.
pub fn cancel_button(scene: &mut Scene, rect: Rect, pressed: bool, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let c = rect.center();
    scene.fill(&Circle::new(c, rect.width() / 2.0), p.cancel);
    if pressed {
        scene.fill(
            &Circle::new(c, rect.width() / 2.0),
            Color::new([0.0, 0.0, 0.0, 1.0 - PRESSED_DIM]),
        );
    }
    let stroke = Stroke::new(1.5).with_caps(Cap::Round);
    let k = CANCEL_CROSS;
    for (a, b) in [((-k, -k), (k, k)), ((-k, k), (k, -k))] {
        scene.stroke(
            &Line::new(c + Vec2::from(a), c + Vec2::from(b)),
            &stroke,
            p.fill,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn layers_are_balanced() {
        for shape in [FieldShape::Square, FieldShape::Round] {
            for disabled in [false, true] {
                let mut scene = Scene::new(120.0, 30.0);
                let rect = Rect::new(4.0, 4.0, 110.0, 26.0);
                let field = TextField {
                    shape,
                    focused: true,
                    disabled,
                };
                text_field(&mut scene, rect, field, Appearance::Dark);
                magnifier(&mut scene, Point::new(14.0, 15.0), Appearance::Dark);
                cancel_button(&mut scene, cancel_button_rect(rect), true, Appearance::Dark);
                assert_eq!(scene.open_layers(), 0);
            }
        }
    }

    #[test]
    fn the_inside_is_within_the_border() {
        let rect = Rect::new(0.0, 0.0, 100.0, 22.0);
        let inside = text_field_inside(rect, FieldShape::Round);
        assert_eq!(inside.rect(), Rect::new(1.0, 1.0, 99.0, 21.0));
        assert_eq!(inside.radii().top_left, 10.0);
        assert_eq!(
            text_field_inside(rect, FieldShape::Square).radii().top_left,
            0.0
        );
        assert!(cancel_button_rect(rect).x1 < rect.x1);
    }
}
