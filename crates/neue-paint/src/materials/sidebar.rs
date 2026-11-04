//! Source-list sidebars: the tinted background, the gradient of the selected row (in the
//! accent with focus, grey without), count badges and icons (see [`crate::icon`]). Rows are
//! laid out by the caller from the geometry functions here, so painting and hit-testing
//! agree; the text is the caller's.
//!
//! Colours and metrics come from [`neue_theme::materials::sidebar`].

use neue_theme::materials::sidebar::{
    ACTION_WIDTH, BADGE_HEIGHT, BADGE_INSET, BADGE_PADDING, DISCLOSURE_WIDTH, EDGE, GROUP_INSET,
    ICON_GAP, ICON_SIZE, INDENT, ITEM_INSET, Palette,
};
use vello_common::kurbo::{Point, Rect, RoundedRect};
use vello_common::peniko::{Color, Gradient};

use crate::Variant;
use crate::scene::Scene;

/// How a source-list row is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceState {
    /// Not selected.
    #[default]
    Normal,
    /// Selected, in a list with keyboard focus: the accent gradient.
    Selected,
    /// Selected, in a list without focus: the grey gradient.
    SelectedUnfocused,
}

impl SourceState {
    /// A row's state from whether it is selected and the list has focus.
    pub fn new(selected: bool, focused: bool) -> Self {
        match (selected, focused) {
            (false, _) => Self::Normal,
            (true, true) => Self::Selected,
            (true, false) => Self::SelectedUnfocused,
        }
    }
}

fn vertical(rect: Rect, [top, bottom]: [Color; 2]) -> Gradient {
    Gradient::new_linear(Point::new(0.0, rect.y0), Point::new(0.0, rect.y1))
        .with_stops([(0.0, top), (1.0, bottom)])
}

/// Draw the sidebar background filling `rect`, with a line down its right edge.
pub fn sidebar(scene: &mut Scene, rect: Rect, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    scene.fill(&rect, p.background);
    scene.fill(
        &Rect::new(rect.x1 - EDGE, rect.y0, rect.x1, rect.y1),
        p.edge,
    );
}

/// Draw the selected row filling `rect`: a gradient with a line along its top and bottom,
/// in the accent when `focused` and grey otherwise. Nothing for [`SourceState::Normal`].
pub fn source_list_selection(
    scene: &mut Scene,
    rect: Rect,
    state: SourceState,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into());
    let (fill, [top, bottom]) = match state {
        SourceState::Normal => return,
        SourceState::Selected => (p.selection, p.selection_edge),
        SourceState::SelectedUnfocused => (p.unfocused_selection, p.unfocused_edge),
    };
    scene.fill(&rect, vertical(rect, fill));
    scene.fill(&Rect::new(rect.x0, rect.y0, rect.x1, rect.y0 + EDGE), top);
    scene.fill(
        &Rect::new(rect.x0, rect.y1 - EDGE, rect.x1, rect.y1),
        bottom,
    );
}

/// Where the parts of an item row go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceItem {
    /// The disclosure column, left of the icon: the triangle is centred in it, and a press
    /// in it expands or collapses the row.
    pub disclosure: Rect,
    /// The icon (reserved even if the item has none, so labels line up only when it does).
    pub icon: Rect,
    /// Where the label starts.
    pub label_x: f64,
}

/// The parts of an item at `level` (1 for the first level under a group; deeper items
/// indent) in a row filling `row`; `has_icon` tells whether a label follows an icon.
pub fn source_item(row: Rect, level: usize, has_icon: bool) -> SourceItem {
    #[expect(clippy::cast_precision_loss, reason = "levels far below 2^52")]
    let depth = level.saturating_sub(1) as f64;
    let x0 = row.x0 + ITEM_INSET + depth * INDENT;
    let y0 = (row.y0 + (row.height() - ICON_SIZE) / 2.0).round();
    let icon = Rect::new(x0, y0, x0 + ICON_SIZE, y0 + ICON_SIZE);
    SourceItem {
        disclosure: Rect::new(x0 - DISCLOSURE_WIDTH, row.y0, x0, row.y1),
        icon,
        label_x: if has_icon { icon.x1 + ICON_GAP } else { x0 },
    }
}

/// Where a group header's text starts, in a row filling `row`.
pub fn source_group_x(row: Rect) -> f64 {
    row.x0 + GROUP_INSET
}

/// The press target of a group header's Show or Hide: the right end of a row filling `row`.
pub fn source_group_action(row: Rect) -> Rect {
    Rect::new((row.x1 - ACTION_WIDTH).max(row.x0), row.y0, row.x1, row.y1)
}

/// Where a count badge for text `text_width` wide goes in a row filling `row`: at its right
/// end, vertically centred, a circle for a short count.
pub fn source_badge_rect(row: Rect, text_width: f64) -> Rect {
    let width = (text_width + 2.0 * BADGE_PADDING).max(BADGE_HEIGHT).round();
    let x1 = row.x1 - BADGE_INSET;
    let y0 = (row.y0 + (row.height() - BADGE_HEIGHT) / 2.0).round();
    Rect::new(x1 - width, y0, x1, y0 + BADGE_HEIGHT)
}

/// Draw a count badge filling `rect` (see [`source_badge_rect`]); its text is the caller's.
pub fn source_badge(
    scene: &mut Scene,
    rect: Rect,
    state: SourceState,
    variant: impl Into<Variant>,
) {
    let p = Palette::new(variant.into());
    let fill = match state {
        SourceState::Normal => p.badge,
        SourceState::Selected | SourceState::SelectedUnfocused => p.selected_badge,
    };
    scene.fill(&RoundedRect::from_rect(rect, rect.height() / 2.0), fill);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: Rect = Rect::new(0.0, 0.0, 180.0, 22.0);

    #[test]
    fn items_nest_by_the_indent_and_the_label_follows_the_icon() {
        let first = source_item(ROW, 1, true);
        let nested = source_item(ROW, 2, true);
        assert_eq!(first.icon.x0, ITEM_INSET);
        assert_eq!(nested.icon.x0 - first.icon.x0, INDENT);
        assert_eq!(
            first.disclosure.x1, first.icon.x0,
            "the triangle sits left of it"
        );
        assert_eq!(first.label_x, first.icon.x1 + ICON_GAP);
        assert_eq!(source_item(ROW, 1, false).label_x, ITEM_INSET);
        assert_eq!(first.icon.height(), ICON_SIZE);
        assert!(first.icon.y0 >= 0.0 && first.icon.y1 <= ROW.y1);
    }

    #[test]
    fn badges_hug_the_right_edge_and_short_counts_are_circles() {
        let one = source_badge_rect(ROW, 2.0);
        assert_eq!((one.width(), one.height()), (BADGE_HEIGHT, BADGE_HEIGHT));
        assert_eq!(one.x1, ROW.x1 - BADGE_INSET);
        let many = source_badge_rect(ROW, 30.0);
        assert_eq!(many.width(), 30.0 + 2.0 * BADGE_PADDING);
        assert_eq!(many.x1, one.x1);
    }

    #[test]
    fn the_group_action_is_the_right_end_of_the_row() {
        let a = source_group_action(ROW);
        assert_eq!((a.x1, a.width()), (ROW.x1, ACTION_WIDTH));
        assert_eq!(source_group_action(Rect::new(0.0, 0.0, 10.0, 22.0)).x0, 0.0);
    }

    #[test]
    fn a_row_is_selected_only_when_it_says_so() {
        assert_eq!(SourceState::new(false, true), SourceState::Normal);
        assert_eq!(SourceState::new(true, true), SourceState::Selected);
        assert_eq!(
            SourceState::new(true, false),
            SourceState::SelectedUnfocused
        );
    }
}
