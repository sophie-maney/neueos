//! Source-list sidebars (ADR 0050): an outline in the sidebar style. The top level of the
//! tree is the *groups*, small-capital headers that show Show or Hide under the pointer and
//! can't be selected; what is under them are *items* of an icon, a label and a count badge.
//!
//! A [`SourceList`] is an [`Outline`] whose table is built in the sidebar style
//! ([`Table::build`]): one column, no header or border, rows drawn on the sidebar
//! background with the source-list selection gradient. Its cells are [`SourceContent`]s,
//! which draw themselves; the selection, keys, expansion and virtual rows are the table's.

use std::hash::Hash;

use masonry_core::core::PaintCtx;
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Vec2};
use neue_paint::Scene;
use neue_paint::icon::Icon;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{
    SourceState, disclosure_triangle, source_badge, source_badge_rect, source_group_x, source_item,
};
use neue_theme::Theme;
use neue_theme::materials::sidebar::{ACTION_INSET, Palette};
use neue_theme::materials::table::Palette as TablePalette;

use super::outline::{Outline, RowInfo};
use super::table::{Cell, Column, Look};
use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::element::{Element, Handle};

/// What a [`SourceList`] shows for one node: a group's title, or an item's label with an
/// optional icon and count badge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    label: String,
    icon: Option<Icon>,
    badge: Option<String>,
}

impl Entry {
    /// An entry labelled `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            badge: None,
        }
    }

    /// Shows `icon` (a [`crate::Glyph`], say) before the label (items only).
    #[must_use]
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Shows `badge` (a count, say) in a pill at the right end (items only).
    #[must_use]
    pub fn badge(mut self, badge: impl ToString) -> Self {
        self.badge = Some(badge.to_string());
        self
    }

    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn badge_text(&self) -> Option<String> {
        self.badge.clone()
    }

    pub(super) fn into_label(self) -> String {
        self.label
    }
}

impl<M> From<Entry> for Cell<M> {
    fn from(entry: Entry) -> Self {
        Cell::source(entry)
    }
}

/// A source-list cell: it draws a group header or an item itself.
pub(crate) struct SourceContent {
    /// The label (a group's in capitals).
    label: TextLine,
    plain: String,
    badge: Option<TextLine>,
    /// A group header's Show or Hide.
    action: TextLine,
    icon: Option<Icon>,
    group: bool,
}

impl SourceContent {
    pub(super) fn new(entry: Entry, group: bool) -> Self {
        let Entry { label, icon, badge } = entry;
        let shown = if group {
            // Small capitals: capitals at the small size.
            TextLine::new(label.to_uppercase(), TextStyle::Small)
        } else {
            TextLine::new(label.clone(), TextStyle::Body)
        };
        Self {
            label: shown,
            plain: label,
            badge: badge
                .filter(|_| !group)
                .map(|b| TextLine::new(b, TextStyle::Small)),
            action: TextLine::new(String::new(), TextStyle::Small),
            icon: icon.filter(|_| !group),
            group,
        }
    }

    /// The label as given (not in capitals).
    pub(super) fn label(&self) -> &str {
        &self.plain
    }

    pub(super) fn invalidate(&mut self) {
        self.label.invalidate();
        self.action.invalidate();
        if let Some(badge) = &mut self.badge {
            badge.invalidate();
        }
    }

    /// Draws the cell filling the painted area: a group header, or an item.
    pub(super) fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        painter: &mut Painter<'_>,
        replayer: &mut ImagingReplayer,
        theme: Theme,
        look: Look,
        info: Option<RowInfo>,
    ) {
        let colors = theme.colors();
        let p = Palette::new(theme.variant);
        let disabled = ctx.is_disabled();
        let size = ctx.content_box().size();
        let row = size.to_rect();
        let selected = look.selected && !disabled;
        let on_gradient = selected && look.emphasized;
        let state = SourceState::new(selected, look.emphasized);
        let level = info.map_or(0, |i| i.level);
        let center = |height: f64| ((size.height - height) / 2.0).round();
        let scale = ctx.scale_factor();

        if self.group {
            let color = if disabled {
                colors.disabled_label
            } else {
                p.group_label
            };
            let text = self.label.size(ctx.text_contexts());
            let at = Vec2::new(source_group_x(row), center(text.height));
            self.label.paint(painter, Affine::translate(at), color);
            let expandable = info.is_some_and(|i| i.expandable);
            if look.hovered && expandable && !disabled {
                let open = info.is_some_and(|i| i.expanded);
                self.action
                    .set_text(if open { "Hide" } else { "Show" }.to_owned());
                let text = self.action.size(ctx.text_contexts());
                let at = Vec2::new(
                    (size.width - ACTION_INSET - text.width).round(),
                    center(text.height),
                );
                self.action
                    .paint(painter, Affine::translate(at), p.group_action);
            }
            return;
        }

        let rects = source_item(row, level.max(1), self.icon.is_some());
        let (label_color, icon_color, triangle) = if disabled {
            (
                colors.disabled_label,
                colors.disabled_label,
                colors.disabled_label,
            )
        } else if on_gradient {
            (
                colors.selected_label,
                colors.selected_label,
                colors.selected_label,
            )
        } else {
            (
                colors.label,
                p.icon,
                TablePalette::new(theme.variant).disclosure,
            )
        };
        let mut scene = Scene::new(size.width, size.height);
        if info.is_some_and(|i| i.expandable) {
            disclosure_triangle(
                &mut scene,
                rects.disclosure,
                info.is_some_and(|i| i.expanded),
                triangle,
            );
        }
        if let Some(icon) = &self.icon {
            super::icon::draw(
                &mut scene,
                rects.icon,
                icon,
                icon_color,
                theme.variant,
                disabled,
            );
        }
        let mut badge = None;
        if let Some(line) = &mut self.badge {
            let text = line.size(ctx.text_contexts());
            let rect = source_badge_rect(row, text.width);
            let shown = if disabled { SourceState::Normal } else { state };
            if disabled {
                scene.push_opacity(0.5);
            }
            source_badge(&mut scene, rect, shown, theme.variant);
            if disabled {
                scene.pop_layer();
            }
            let color = match shown {
                _ if disabled => p.badge_label.with_alpha(0.5),
                SourceState::Normal => p.badge_label,
                SourceState::Selected => p.selected_badge_label,
                SourceState::SelectedUnfocused => p.unfocused_badge_label,
            };
            badge = Some((rect, text, color));
        }
        replayer.replay(&scene, scale, painter);
        if let (Some((rect, text, color)), Some(line)) = (badge, &self.badge) {
            let at = Vec2::new(
                (rect.x0 + (rect.width() - text.width) / 2.0).round(),
                center(text.height),
            );
            line.paint(painter, Affine::translate(at), color);
        }
        let text = self.label.size(ctx.text_contexts());
        let at = Vec2::new(rects.label_x.round(), center(text.height));
        if on_gradient {
            let shadow = at + Vec2::new(0.0, 1.0);
            self.label
                .paint(painter, Affine::translate(shadow), p.selected_shadow);
        }
        self.label
            .paint(painter, Affine::translate(at), label_color);
    }
}

type OnSelect<M, Id> = Box<dyn FnMut(Option<&Id>) -> M>;

/// A sidebar of groups of items: a source list.
///
/// The top level of the tree is the *groups*: small-capital headers that can't be selected
/// and, when they have items, show Show or Hide while the pointer is over them (a click on it
/// collapses or expands the group). Under a group are *items*, each an icon, a label and
/// optionally a count badge (`entry`), which can have items of their own, shown indented
/// with a disclosure triangle. The selected item is drawn with the source-list gradient:
/// the accent while the list has keyboard focus, grey without.
///
/// `Id` names a node and must be stable: what is expanded and selected is remembered by it.
/// `children(None)` gives the groups and `children(Some(id))` the items inside a node, in
/// order; like an [`Outline`]'s closures, they and `entry` can't see the model, so they read
/// shared data, and [`Handle::reload`] reads the tree again after it changes. One item is
/// selected at a time. Up and Down move the selection from item to item, skipping group
/// headers, Right and Left open and close an item that has items, and typing selects the
/// first item whose label starts with what was typed. Return or a double-click activates
/// the selected item. Group headers are expanded and collapsed by the pointer and by
/// assistive technology, not the keyboard.
pub struct SourceList<M, Id> {
    outline: Outline<M, Id>,
    on_select: Option<OnSelect<M, Id>>,
}

impl<M, Id> std::fmt::Debug for SourceList<M, Id> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceList")
            .field("outline", &self.outline)
            .finish_non_exhaustive()
    }
}

impl<M: 'static, Id: Clone + Eq + Hash + 'static> SourceList<M, Id> {
    /// A source list over the tree `children` describes, each node shown as `entry(id)`.
    /// Groups start expanded.
    pub fn new(
        children: impl FnMut(Option<&Id>) -> Vec<Id> + 'static,
        mut entry: impl FnMut(&Id) -> Entry + 'static,
    ) -> Self {
        let mut children = children;
        let groups = children(None);
        let outline =
            Outline::source(Box::new(children), Box::new(move |id| entry(id))).expanded(groups);
        Self {
            outline,
            on_select: None,
        }
    }

    /// Starts with the nodes `ids` expanded (items with items of their own, anywhere in the
    /// tree; they show once their group is open).
    #[must_use]
    pub fn expanded(self, ids: impl IntoIterator<Item = Id>) -> Self {
        Self {
            outline: self.outline.expanded(ids),
            ..self
        }
    }

    /// Starts with the groups `ids` collapsed.
    #[must_use]
    pub fn collapsed(self, ids: impl IntoIterator<Item = Id>) -> Self {
        Self {
            outline: self.outline.collapsed(ids),
            ..self
        }
    }

    /// Starts with the item `id` selected.
    #[must_use]
    pub fn selected(mut self, id: Id) -> Self {
        self.outline = self.outline.selected([id]);
        self
    }

    /// Sends the message `make` returns, given the selected item (`None`: nothing is
    /// selected any more), when the user changes the selection. Changes through a
    /// [`Handle`] send nothing.
    #[must_use]
    pub fn on_select(mut self, make: impl FnMut(Option<&Id>) -> M + 'static) -> Self {
        self.on_select = Some(Box::new(make));
        self
    }

    /// Sends the message `make` returns, given the item, when the user double-clicks it or
    /// presses Return. Without it, Return goes on to the default button.
    #[must_use]
    pub fn on_activate(mut self, make: impl FnMut(&Id) -> M + 'static) -> Self {
        self.outline = self.outline.on_activate(make);
        self
    }

    /// The width it asks for, in logical pixels.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.outline = self.outline.width(width);
        self
    }

    /// The height it asks for, in logical pixels.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.outline = self.outline.height(height);
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.outline = self.outline.disabled(disabled);
        self
    }

    /// Points `handle` at this source list.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        let mut inner = Handle::default();
        let outline = self.outline.bind(&mut inner);
        if let Some(id) = inner.id() {
            *handle = Handle::new(id);
        }
        Self { outline, ..self }
    }
}

impl<M: 'static, Id: Clone + Eq + Hash + 'static> From<SourceList<M, Id>> for Element<M> {
    fn from(list: SourceList<M, Id>) -> Self {
        let SourceList {
            mut outline,
            on_select,
        } = list;
        if let Some(mut f) = on_select {
            outline = outline.on_select(move |ids| f(ids.first()));
        }
        outline.into()
    }
}

impl<M, Id: Clone + Eq + Hash + 'static> Handle<SourceList<M, Id>> {
    /// The handle as an outline's, whose methods this one's are.
    fn outline(&self) -> Handle<Outline<M, Id>> {
        self.id().map_or_else(Handle::default, Handle::new)
    }

    /// Reads the tree again, after the data behind `children` and `entry` changed. What is
    /// expanded and selected is kept (by node); nodes that are gone are dropped.
    pub fn reload<N>(&self, cx: &mut Cx<'_, N>) {
        self.outline().reload(cx);
    }

    /// Expands or collapses the node `id` (a group, or an item with items). A node whose
    /// parents are closed only remembers. Sends no message.
    pub fn set_expanded<N>(&self, cx: &mut Cx<'_, N>, id: &Id, expanded: bool) {
        self.outline().set_expanded(cx, id, expanded);
    }

    /// Selects the item `id` (`None`: no item). Sends no message.
    pub fn set_selection<N>(&self, cx: &mut Cx<'_, N>, id: Option<Id>) {
        self.outline().set_selection(cx, id);
    }

    /// Scrolls as little as it takes to show the row of the visible node `id` whole.
    pub fn scroll_to<N>(&self, cx: &mut Cx<'_, N>, id: &Id) {
        self.outline().scroll_to(cx, id);
    }

    /// Enables or disables the source list.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        self.outline().set_enabled(cx, enabled);
    }
}

/// The columns of a source list: one, as wide as the view.
pub(super) fn columns() -> Vec<Column> {
    vec![Column::new("")]
}
