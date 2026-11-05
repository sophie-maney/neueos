//! Toolbar items: the pill behind a pressed button and the separator between groups.
//! Colours and metrics come from [`neue_theme::materials::toolbar`]; the glyphs and labels
//! are the caller's.

use neue_theme::materials::toolbar::{Palette, RADIUS, SEPARATOR_WIDTH};
use vello_common::kurbo::{Rect, RoundedRect};

use super::focus_ring;
use crate::Variant;
use crate::scene::Scene;

/// Everything that changes how a toolbar button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolbarItem {
    /// Held down: a dark pill sits behind the icon and label.
    pub pressed: bool,
    /// Greyed out: reduced opacity, no pill, no focus ring.
    pub disabled: bool,
    /// Draws the keyboard focus ring around the button.
    pub focused: bool,
}

/// The outline of a toolbar button filling `rect`: the shape focus rings follow.
pub fn toolbar_item_outline(rect: Rect) -> RoundedRect {
    RoundedRect::from_rect(rect, RADIUS.min(rect.height() / 2.0))
}

/// Draw the background of a toolbar button filling `rect` (logical pixels). At rest it is
/// empty: toolbar buttons have no chrome. The focus ring reaches
/// [`neue_theme::metrics::control::FOCUS_RING`] outside `rect`.
pub fn toolbar_item(scene: &mut Scene, rect: Rect, item: ToolbarItem, variant: impl Into<Variant>) {
    let variant = variant.into();
    let outline = toolbar_item_outline(rect);
    if item.disabled {
        return;
    }
    if item.pressed {
        scene.fill(&outline, Palette::new(variant).pressed);
    }
    if item.focused {
        focus_ring(scene, outline, variant);
    }
}

/// Draw a separator in `rect`: a dark line with a light line beside it, as wide as
/// [`SEPARATOR_WIDTH`], centred in `rect`.
pub fn toolbar_separator(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let x = (rect.center().x - SEPARATOR_WIDTH / 2.0).round();
    let line = Rect::new(x, rect.y0, x + 1.0, rect.y1);
    scene.fill(&line, p.separator);
    scene.fill(
        &(line + vello_common::kurbo::Vec2::new(1.0, 0.0)),
        p.separator_highlight,
    );
}
