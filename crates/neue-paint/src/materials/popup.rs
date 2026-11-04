//! Pop-up and pull-down buttons: neutral gel with an accent cap on the right that carries
//! the arrows (up and down for a pop-up, down for a pull-down), and the combo box's arrow
//! button, a small gel button inside the end of the field's well.
//!
//! Colours and metrics come from [`neue_theme::materials::popup`], the push button's gel
//! and [`neue_theme::metrics::control`].

use neue_theme::materials::aqua_button::{Palette, RADIUS};
use neue_theme::materials::popup::{
    ARROW_GAP, ARROW_HEIGHT, ARROW_WIDTH, CAP_ARROW, COMBO_INSET, DISABLED_OPACITY, OPEN_DIM,
};
use neue_theme::materials::stepper::arrow;
use neue_theme::materials::text_field::BORDER;
use neue_theme::metrics::control::{COMBO_ARROW_WIDTH, POPUP_CAP_WIDTH};
use vello_common::kurbo::{BezPath, Point, Rect, RoundedRect};
use vello_common::peniko::Color;

use super::aqua_button::{draw, gel_body};
use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// The two buttons that open a menu from a cap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PopUpKind {
    /// Shows the chosen item; the menu opens over it. Up and down arrows.
    #[default]
    PopUp,
    /// Shows a fixed title; the menu opens below. A down arrow.
    PullDown,
}

/// Everything that changes how a pop-up or pull-down button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopUpButton {
    pub kind: PopUpKind,
    /// Its menu is open: the cap darkens.
    pub open: bool,
    /// Greyed out: neutral cap, reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring. Pass `false` while the window is inactive.
    pub focused: bool,
}

/// The outline of a pop-up or pull-down button filling `rect`: the shape focus rings follow.
pub fn popup_button_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.height() / 2.0))
}

/// The cap of a button filling `rect`: the strip on the right that carries the arrows.
pub fn popup_cap_rect(rect: Rect) -> Rect {
    Rect::new(rect.x1 - POPUP_CAP_WIDTH, rect.y0, rect.x1, rect.y1)
}

fn triangle(centre: Point, up: bool) -> BezPath {
    let (tip, base) = if up {
        (centre.y - ARROW_HEIGHT / 2.0, centre.y + ARROW_HEIGHT / 2.0)
    } else {
        (centre.y + ARROW_HEIGHT / 2.0, centre.y - ARROW_HEIGHT / 2.0)
    };
    let mut path = BezPath::new();
    path.move_to(Point::new(centre.x, tip));
    path.line_to(Point::new(centre.x + ARROW_WIDTH / 2.0, base));
    path.line_to(Point::new(centre.x - ARROW_WIDTH / 2.0, base));
    path.close_path();
    path
}

/// Draw a pop-up or pull-down button filling `rect` (logical pixels;
/// [`neue_theme::metrics::control::REGULAR_HEIGHT`] tall is the regular size; the title is
/// the caller's). The focus ring reaches [`neue_theme::metrics::control::FOCUS_RING`]
/// outside `rect`.
pub fn popup_button(
    scene: &mut Scene,
    rect: Rect,
    button: PopUpButton,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    let outline = popup_button_outline(rect);
    let radius = outline.radii().top_left;
    if button.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    draw(scene, rect, radius, &Palette::plain(variant));

    // The cap: accent gel (neutral when disabled) up to the rim, clipped to the body.
    let cap = if button.disabled {
        Palette::plain(variant)
    } else {
        Palette::new(variant, button.open)
    };
    let inner = rect.inset(-neue_theme::metrics::control::RIM);
    let body = RoundedRect::from_rect(inner, (radius - 1.0).max(0.0));
    scene.push_clip(&body);
    let cap_rect = Rect::new(rect.x1 - POPUP_CAP_WIDTH, inner.y0, inner.x1, inner.y1);
    gel_body(scene, cap_rect, 0.0, &cap);
    scene.fill(
        &Rect::new(cap_rect.x0 - 1.0, inner.y0, cap_rect.x0, inner.y1),
        cap.rim[1],
    );
    scene.pop_layer();

    let colour = if button.disabled {
        arrow(variant)
    } else {
        CAP_ARROW
    };
    let centre = Point::new((cap_rect.x0 + inner.x1) / 2.0, rect.center().y);
    match button.kind {
        PopUpKind::PopUp => {
            let offset = (ARROW_HEIGHT + ARROW_GAP) / 2.0;
            scene.fill(
                &triangle(Point::new(centre.x, centre.y - offset), true),
                colour,
            );
            scene.fill(
                &triangle(Point::new(centre.x, centre.y + offset), false),
                colour,
            );
        }
        PopUpKind::PullDown => scene.fill(&triangle(centre, false), colour),
    }

    if button.disabled {
        scene.pop_layer();
    } else if button.focused {
        focus_ring(scene, outline, variant);
    }
}

/// The arrow button inside a combo box field filling `field`: at the right end of the well.
pub fn combo_arrow_rect(field: Rect) -> Rect {
    let inset = BORDER + COMBO_INSET;
    Rect::new(
        field.x1 - inset - COMBO_ARROW_WIDTH,
        field.y0 + inset,
        field.x1 - inset,
        field.y1 - inset,
    )
}

/// Draw a combo box's arrow button in `rect` (from [`combo_arrow_rect`]), darker while its
/// menu is `open`, faded when `disabled`.
pub fn combo_arrow(
    scene: &mut Scene,
    rect: Rect,
    open: bool,
    disabled: bool,
    variant: impl Into<Variant>,
) {
    let variant = variant.into();
    if disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    let radius = RADIUS.min(rect.height() / 2.0);
    draw(scene, rect, radius, &Palette::plain(variant));
    if open {
        scene.fill(
            &RoundedRect::from_rect(rect, radius),
            Color::new([0.0, 0.0, 0.0, 1.0 - OPEN_DIM]),
        );
    }
    scene.fill(&triangle(rect.center(), false), arrow(variant));
    if disabled {
        scene.pop_layer();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn layers_are_balanced() {
        let rect = Rect::new(4.0, 4.0, 124.0, 26.0);
        for kind in [PopUpKind::PopUp, PopUpKind::PullDown] {
            for (open, disabled, focused) in [
                (false, false, true),
                (true, false, false),
                (false, true, true),
            ] {
                let mut scene = Scene::new(130.0, 30.0);
                let button = PopUpButton {
                    kind,
                    open,
                    disabled,
                    focused,
                };
                popup_button(&mut scene, rect, button, Appearance::Light);
                assert_eq!(scene.open_layers(), 0, "{button:?}");
            }
        }
        let mut scene = Scene::new(130.0, 30.0);
        combo_arrow(
            &mut scene,
            combo_arrow_rect(rect),
            true,
            true,
            Appearance::Dark,
        );
        assert_eq!(scene.open_layers(), 0);
    }

    #[test]
    fn the_cap_sits_at_the_right_end() {
        let rect = Rect::new(0.0, 0.0, 100.0, 22.0);
        assert_eq!(popup_cap_rect(rect).x1, rect.x1);
        let arrow = combo_arrow_rect(rect);
        assert!(rect.contains_rect(arrow));
        assert_eq!(arrow.width(), COMBO_ARROW_WIDTH);
    }
}
