//! Checkbox and radio button: a small gel box or disc that fades from neutral to accent
//! gel, with a white mark (check, dash or dot) over it.
//!
//! Reuses the push button's gel ([`neue_theme::materials::aqua_button::Palette`]); sizes
//! and mark colours come from [`neue_theme::materials::toggle`].

use neue_theme::materials::aqua_button::Palette;
use neue_theme::materials::toggle::{
    DISABLED_OPACITY, DOT, MARK, MARK_SHADOW, MARK_SHADOW_OFFSET, MARK_WIDTH, PRESSED_DIM, RADIUS,
};
use vello_common::kurbo::{BezPath, Cap, Circle, Join, Point, Rect, RoundedRect, Stroke, Vec2};

use super::aqua_button::draw;
use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// A square box or a disc.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToggleKind {
    /// A rounded square with a check mark or a dash.
    #[default]
    Checkbox,
    /// A disc with a dot.
    Radio,
}

/// The mark of a checkbox that is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mark {
    /// A check mark.
    #[default]
    Check,
    /// A dash: the mixed state.
    Dash,
}

/// Everything that changes how a checkbox or radio button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Toggle {
    /// Checkbox or radio button.
    pub kind: ToggleKind,
    /// The checkbox's mark while on. Ignored for radio buttons, which show a dot.
    pub mark: Mark,
    /// How far on it is, from 0 (neutral, no mark) to 1 (accent gel with the mark).
    pub on: f32,
    /// Held down.
    pub pressed: bool,
    /// Greyed out: reduced opacity, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring.
    pub focused: bool,
}

/// The outline of a box or disc filling `rect`: the shape focus rings follow.
pub fn toggle_outline(rect: Rect, kind: ToggleKind) -> RoundedRect {
    let radius = match kind {
        ToggleKind::Checkbox => RADIUS,
        ToggleKind::Radio => rect.width().min(rect.height()) / 2.0,
    };
    RoundedRect::from_rect(rect, radius)
}

/// Draw a checkbox or radio button filling `rect` (logical pixels;
/// [`neue_theme::metrics::control::CHECKBOX_SIZE`] square is the regular size; the label
/// is the caller's). The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn toggle(scene: &mut Scene, rect: Rect, toggle: Toggle, variant: impl Into<Variant>) {
    let variant = variant.into();
    let on = toggle.on.clamp(0.0, 1.0);
    let neutral = if toggle.pressed {
        Palette::plain(variant).dimmed(PRESSED_DIM)
    } else {
        Palette::plain(variant)
    };
    let palette = neutral.mix_with(Palette::new(variant, toggle.pressed), on);
    let outline = toggle_outline(rect, toggle.kind);

    if toggle.disabled {
        scene.push_opacity(DISABLED_OPACITY);
    }
    draw(scene, rect, outline.radii().top_left, &palette);
    if on > 0.0 {
        scene.push_opacity(on);
        mark(scene, rect, toggle);
        scene.pop_layer();
    }
    if toggle.disabled {
        scene.pop_layer();
    } else if toggle.focused {
        focus_ring(scene, outline, variant);
    }
}

/// The white mark with its shadow just below.
fn mark(scene: &mut Scene, rect: Rect, toggle: Toggle) {
    let (w, h) = (rect.width(), rect.height());
    let at = |x: f64, y: f64| Point::new(rect.x0 + x * w, rect.y0 + y * h);
    let shadow = Vec2::new(0.0, MARK_SHADOW_OFFSET);
    match toggle.kind {
        ToggleKind::Radio => {
            let dot = Circle::new(rect.center(), DOT * w.min(h) / 2.0);
            scene.fill(&Circle::new(dot.center + shadow, dot.radius), MARK_SHADOW);
            scene.fill(&dot, MARK);
        }
        ToggleKind::Checkbox => {
            let mut path = BezPath::new();
            match toggle.mark {
                Mark::Check => {
                    path.move_to(at(0.24, 0.52));
                    path.line_to(at(0.43, 0.74));
                    path.line_to(at(0.78, 0.26));
                }
                Mark::Dash => {
                    path.move_to(at(0.26, 0.5));
                    path.line_to(at(0.74, 0.5));
                }
            }
            let stroke = Stroke::new(MARK_WIDTH)
                .with_caps(Cap::Round)
                .with_join(Join::Round);
            let mut shadowed = path.clone();
            shadowed.apply_affine(vello_common::kurbo::Affine::translate(shadow));
            scene.stroke(&shadowed, &stroke, MARK_SHADOW);
            scene.stroke(&path, &stroke, MARK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    fn unit() -> Rect {
        Rect::new(2.0, 2.0, 16.0, 16.0)
    }

    #[test]
    fn layers_are_balanced() {
        for kind in [ToggleKind::Checkbox, ToggleKind::Radio] {
            for disabled in [false, true] {
                let mut scene = Scene::new(20.0, 20.0);
                let t = Toggle {
                    kind,
                    on: 1.0,
                    disabled,
                    focused: true,
                    ..Toggle::default()
                };
                toggle(&mut scene, unit(), t, Appearance::Light);
                assert_eq!(scene.open_layers(), 0);
            }
        }
    }

    #[test]
    fn the_mark_appears_only_when_on() {
        let draw_with = |on: f32| {
            let mut scene = Scene::new(20.0, 20.0);
            let t = Toggle {
                on,
                ..Toggle::default()
            };
            toggle(&mut scene, unit(), t, Appearance::Dark);
            scene.commands().len()
        };
        assert!(draw_with(1.0) > draw_with(0.0));
        assert_eq!(draw_with(0.5), draw_with(1.0));
    }

    #[test]
    fn a_radio_outline_is_a_circle() {
        let outline = toggle_outline(unit(), ToggleKind::Radio);
        assert_eq!(outline.radii().top_left, 7.0);
    }
}
