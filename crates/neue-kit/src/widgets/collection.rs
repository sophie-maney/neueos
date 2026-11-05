//! Collection views (task 2.20c, ADR 0052): items of an icon over a label, in a scrolling
//! grid of cells.
//!
//! A [`Collection`] is a [`VirtualGrid`](crate::VirtualGrid)'s [`RowsWidget`] with a
//! [`CollectionBody`], as a table's body is the same widget with a table. The body owns the
//! selection, takes keyboard focus, paints the background and sends [`TableAction`]s (the
//! selected indices, an index activated); the items ([`CollectionItemWidget`]) draw their
//! own icon, label and selection. The builder maps indices to the app's ids, so the app
//! speaks in ids and the selection survives the items being reordered ([`set_items`]).
//!
//! [`set_items`]: Handle::set_items

use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;
use std::rc::Rc;
use std::time::Instant;

use masonry_core::accesskit::{Action, Node, NodeId, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, BrushIndex, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId,
    WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Point, Rect, RoundedRect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use masonry_core::parley::{FontContext, LayoutContext};
use neue_paint::Scene;
use neue_paint::icon::Icon;
use neue_paint::imaging::ImagingReplayer;
use neue_theme::materials::collection::{
    CELL_INSET, ICON_PADDING, ICON_RADIUS, ICON_SIZE, ICON_SIZES, LABEL_GAP, LABEL_HEIGHT,
    LABEL_PADDING, Palette, cell_size,
};

use super::menu::TypeSelect;
use super::rows::{MakeRow, RowsWidget};
use super::scroll::{Frame, ScrollAxes, ScrollWidget};
use super::table::{Later, Look, Selection, TableAction, command};
use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::element::{Element, Handle, Wiring};
use crate::theme::ThemeProp;

/// What a [`Collection`] shows for one item: a label and, usually, an icon above it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    label: String,
    icon: Option<Icon>,
}

impl Item {
    /// An item labelled `label`, with no icon.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
        }
    }

    /// Shows `icon` (a [`crate::Glyph`], say) above the label.
    #[must_use]
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

// --- MARK: IDS

/// The ids of a collection's items, in order, and where each is.
pub(crate) struct IdList<Id> {
    ids: Vec<Id>,
    index: HashMap<Id, usize>,
}

impl<Id: Clone + Eq + Hash> IdList<Id> {
    fn new(ids: Vec<Id>) -> Self {
        let mut list = Self {
            ids: Vec::new(),
            index: HashMap::new(),
        };
        list.set(ids);
        list
    }

    /// Replaces the ids. Should an id be there twice, the first one is the item it names.
    fn set(&mut self, ids: Vec<Id>) {
        self.index.clear();
        for (i, id) in ids.iter().enumerate() {
            self.index.entry(id.clone()).or_insert(i);
        }
        self.ids = ids;
    }
}

/// An [`IdList`] with the id type erased, for the body.
pub(crate) trait Ids {
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<Id: Clone + Eq + Hash + 'static> Ids for IdList<Id> {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// --- MARK: STATE

/// What items are built from and pushed to (the selection, the icon size), and the options.
#[derive(Debug)]
pub(crate) struct CollectionState {
    multiple: bool,
    /// The app listens for activation (Return would otherwise reach the default button).
    activates: bool,
    selection: Selection,
    /// The body has focus in a focused window: the selection is in the accent.
    emphasized: bool,
    icon_size: f64,
    /// Items to a row, as the grid lays them out (for assistive technology).
    columns: usize,
}

impl CollectionState {
    fn look(&self, item: usize) -> Look {
        Look {
            selected: self.selection.rows.contains(&item),
            emphasized: self.emphasized,
            hovered: false,
        }
    }

    /// The icon size, kept in the range that is drawn.
    fn icon(&self) -> f64 {
        self.icon_size.clamp(ICON_SIZES.0, ICON_SIZES.1)
    }
}

type Shared = Rc<RefCell<CollectionState>>;

// --- MARK: ITEM

/// One item: an icon over a label in a pill. It draws the selection too.
pub(crate) struct CollectionItemWidget {
    index: usize,
    /// The label as given.
    plain: String,
    /// The label as drawn: `plain`, or the front of it and an ellipsis if it is too wide.
    shown: TextLine,
    /// The width `shown` was fitted to, as bits.
    fitted: Option<u64>,
    icon: Option<Icon>,
    icon_size: f64,
    look: Look,
    shared: Shared,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for CollectionItemWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectionItemWidget")
            .field("index", &self.index)
            .field("label", &self.plain)
            .field("look", &self.look)
            .finish_non_exhaustive()
    }
}

impl CollectionItemWidget {
    fn set_look(this: &mut WidgetMut<'_, Self>, look: Look) {
        if this.widget.look != look {
            this.widget.look = look;
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
    }

    /// Lays the label out in `max` logical pixels: all of it, or as much of its front as
    /// fits with an ellipsis.
    fn fit(
        &mut self,
        (fonts, layouts): (&mut FontContext, &mut LayoutContext<BrushIndex>),
        max: f64,
    ) {
        let key = max.to_bits();
        if self.fitted == Some(key) {
            return;
        }
        self.fitted = Some(key);
        self.shown.set_text(self.plain.clone());
        if self.shown.size((&mut *fonts, &mut *layouts)).width <= max {
            return;
        }
        let ends: Vec<usize> = self
            .plain
            .char_indices()
            .map(|(i, _)| i)
            .chain([self.plain.len()])
            .collect();
        // The most characters whose front and an ellipsis fit (none: just the ellipsis).
        let (mut lo, mut hi) = (0, ends.len() - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            self.shown
                .set_text(format!("{}…", &self.plain[..ends[mid]]));
            if self.shown.size((&mut *fonts, &mut *layouts)).width <= max {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        self.shown.set_text(format!("{}…", &self.plain[..ends[lo]]));
        self.shown.size((&mut *fonts, &mut *layouts));
    }
}

impl UsesProperty<ThemeProp> for CollectionItemWidget {}

impl Widget for CollectionItemWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) => ctx.request_paint_only(),
            Update::FontsChanged => {
                self.shown.invalidate();
                self.fitted = None;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ThemeProp>() {
            ctx.request_paint_only();
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // The grid gives items their size.
        let (width, height) = cell_size(self.icon_size);
        match (axis, len_req) {
            (_, LenReq::FitContent(space)) => space,
            (Axis::Horizontal, _) => Length::px(width),
            (Axis::Vertical, _) => Length::px(height),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let colors = theme.colors();
        let palette = Palette::new(theme.variant);
        let disabled = ctx.is_disabled();
        let size = ctx.content_box().size();
        let selected = self.look.selected && !disabled;
        let focused = selected && self.look.emphasized;

        let backdrop_side = self.icon_size.clamp(ICON_SIZES.0, ICON_SIZES.1) + 2.0 * ICON_PADDING;
        let backdrop_x = ((size.width - backdrop_side) / 2.0).round();
        let backdrop = Rect::from_origin_size(
            Point::new(backdrop_x, CELL_INSET),
            Size::new(backdrop_side, backdrop_side),
        );
        let mut scene = Scene::new(size.width, size.height);
        if selected {
            let fill = if focused {
                palette.icon_backdrop
            } else {
                palette.unfocused_icon_backdrop
            };
            scene.fill(&RoundedRect::from_rect(backdrop, ICON_RADIUS), fill);
        }
        if let Some(icon) = &self.icon {
            let color = if disabled {
                colors.disabled_label
            } else {
                colors.secondary_label
            };
            super::icon::draw(
                &mut scene,
                backdrop.inflate(-ICON_PADDING, -ICON_PADDING),
                icon,
                color,
                theme.variant,
                disabled,
            );
        }

        // The label sits in a pill that is as wide as its text, up to the cell.
        let room = (size.width - CELL_INSET).max(2.0 * LABEL_PADDING);
        self.fit(ctx.text_contexts(), room - 2.0 * LABEL_PADDING);
        let text = self.shown.size(ctx.text_contexts());
        let pill_width = (text.width + 2.0 * LABEL_PADDING).min(room);
        let pill = Rect::from_origin_size(
            Point::new(
                ((size.width - pill_width) / 2.0).round(),
                backdrop.y1 + LABEL_GAP,
            ),
            Size::new(pill_width, LABEL_HEIGHT),
        );
        if selected {
            let fill = if focused {
                colors.selection
            } else {
                colors.unfocused_selection
            };
            scene.fill(&RoundedRect::from_rect(pill, LABEL_HEIGHT / 2.0), fill);
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let color = if disabled {
            colors.disabled_label
        } else if focused {
            colors.selected_label
        } else {
            colors.label
        };
        let at = Vec2::new(
            (pill.x0 + (pill.width() - text.width) / 2.0).round(),
            pill.y0 + ((LABEL_HEIGHT - text.height) / 2.0).round(),
        );
        self.shown.paint(painter, Affine::translate(at), color);
    }

    fn accessibility_role(&self) -> Role {
        Role::GridCell
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let columns = self.shared.borrow().columns.max(1);
        node.set_label(self.plain.as_str());
        node.set_row_index(self.index / columns);
        node.set_column_index(self.index % columns);
        node.set_selected(self.look.selected);
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.plain.clone())
    }
}

// --- MARK: BODY

/// What makes a [`RowsWidget`] a collection's body.
pub(crate) struct CollectionBody {
    shared: Shared,
    ids: Rc<RefCell<dyn Ids>>,
    /// The label of an item (type-select).
    label_of: Box<dyn FnMut(usize) -> Option<String>>,
    typed: TypeSelect,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for CollectionBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectionBody")
            .field("state", &self.shared.borrow())
            .finish_non_exhaustive()
    }
}

impl CollectionBody {
    /// The size of a cell now.
    pub(crate) fn cell(&self) -> (f64, f64) {
        cell_size(self.shared.borrow().icon())
    }

    pub(crate) fn forget_past(&self, count: usize) {
        self.shared.borrow_mut().selection.forget_past(count);
    }

    #[cfg(test)]
    pub(crate) fn selection(&self) -> Selection {
        self.shared.borrow().selection.clone()
    }
}

impl RowsWidget {
    /// The collection. Only the collection hooks call this, and `RowsWidget` calls them
    /// only when it has one.
    #[expect(
        clippy::expect_used,
        reason = "the hooks run only on a collection's body"
    )]
    fn collection_mut(&mut self) -> &mut CollectionBody {
        self.collection
            .as_deref_mut()
            .expect("a collection's body (checked by the caller)")
    }

    /// The item under `at` (body coordinates), if there is one.
    fn item_at(&self, at: Point) -> Option<usize> {
        let width = self.window.width();
        if at.x < 0.0 || at.y < 0.0 || at.x >= width {
            return None;
        }
        let columns = self.columns.max(1);
        #[expect(clippy::cast_precision_loss, reason = "column counts far below 2^52")]
        let cell = width / columns as f64;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "non-negative, and saturating"
        )]
        let (column, row) = (
            ((at.x / cell).floor() as usize).min(columns - 1),
            (at.y / self.row_height).floor() as usize,
        );
        let item = row.saturating_mul(columns).saturating_add(column);
        (item < self.count).then_some(item)
    }

    /// Items moved by Page Up and Page Down: a view's rows less one.
    fn collection_page_rows(&self) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "small, non-negative"
        )]
        let rows = (self.window.height() / self.row_height).floor() as usize;
        rows.saturating_sub(1).max(1)
    }

    /// The columns changed (the width did): tell assistive technology where items are.
    pub(super) fn collection_columns_changed(this: &mut WidgetMut<'_, Self>) {
        let Some(body) = &this.widget.collection else {
            return;
        };
        body.shared.borrow_mut().columns = this.widget.columns;
        for item in this.widget.rows.values_mut() {
            this.ctx.get_mut(item).ctx.request_accessibility_update();
        }
    }

    /// Pushes the selection (selection, emphasis) to the items, after this event.
    pub(super) fn collection_restyle_later(ctx: &mut impl Later) {
        ctx.later(|mut w| Self::collection_restyle(&mut w.downcast()));
    }

    /// Pushes the selection to the items that exist.
    pub(super) fn collection_restyle(this: &mut WidgetMut<'_, Self>) {
        let Some(body) = &this.widget.collection else {
            return;
        };
        let emphasized = this.ctx.is_focus_target() && this.ctx.is_window_focused();
        body.shared.borrow_mut().emphasized = emphasized;
        let state = body.shared.borrow();
        for (&i, item) in &mut this.widget.rows {
            let mut item = this.ctx.get_mut(item);
            if let Some(mut item) = item.try_downcast::<CollectionItemWidget>() {
                CollectionItemWidget::set_look(&mut item, state.look(i));
            }
        }
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// Takes the cell size from the icon size, and builds the items again.
    fn collection_resize(this: &mut WidgetMut<'_, Self>) {
        let Some((width, height)) = this.widget.collection.as_ref().map(|b| b.cell()) else {
            return;
        };
        Self::set_cell(this, width, height);
        Self::edit_reload(this);
    }

    /// Changes the size of the icons from the app: the cells follow.
    pub(super) fn collection_set_icon_size(this: &mut WidgetMut<'_, Self>, size: f64) {
        let Some(body) = &this.widget.collection else {
            return;
        };
        body.shared.borrow_mut().icon_size = size;
        Self::collection_resize(this);
    }

    /// Runs `f` on a collection's ids and state, if this is one over ids of type `Id`.
    pub(super) fn collection_with_ids<Id, R>(
        this: &mut WidgetMut<'_, Self>,
        f: impl FnOnce(&mut IdList<Id>, &mut CollectionState) -> R,
    ) -> Option<R>
    where
        Id: Clone + Eq + Hash + 'static,
    {
        let body = this.widget.collection.as_ref()?;
        let (ids, shared) = (Rc::clone(&body.ids), Rc::clone(&body.shared));
        let mut ids = ids.borrow_mut();
        let list = ids.as_any_mut().downcast_mut::<IdList<Id>>()?;
        let mut state = shared.borrow_mut();
        Some(f(list, &mut state))
    }

    /// Replaces the items from the app, keeping the selection by id. The new number of
    /// items. No message goes back.
    pub(super) fn collection_set_items<Id>(this: &mut WidgetMut<'_, Self>, new: Vec<Id>)
    where
        Id: Clone + Eq + Hash + 'static,
    {
        let len = Self::collection_with_ids::<Id, _>(this, |list, state| {
            let kept: Vec<Id> = state
                .selection
                .rows
                .iter()
                .filter_map(|&i| list.ids.get(i).cloned())
                .collect();
            list.set(new);
            let rows: Vec<usize> = kept
                .iter()
                .filter_map(|id| list.index.get(id).copied())
                .collect();
            state.selection.set(rows, list.ids.len());
            list.ids.len()
        });
        if let Some(len) = len {
            Self::edit_count_and_reload(this, len);
        }
    }

    /// Selects the items `ids` from the app. No message goes back.
    pub(super) fn collection_set_selection<Id>(this: &mut WidgetMut<'_, Self>, ids: Vec<Id>)
    where
        Id: Clone + Eq + Hash + 'static,
    {
        Self::collection_with_ids::<Id, _>(this, |list, state| {
            let rows: Vec<usize> = ids
                .iter()
                .filter_map(|id| list.index.get(id).copied())
                .collect();
            state.selection.set(rows, list.ids.len());
        });
        Self::collection_restyle(this);
    }

    /// The row span of item `id`, if it is there.
    pub(super) fn collection_span<Id>(this: &mut WidgetMut<'_, Self>, id: &Id) -> Option<Rect>
    where
        Id: Clone + Eq + Hash + 'static,
    {
        let index =
            Self::collection_with_ids::<Id, _>(this, |list, _| list.index.get(id).copied())??;
        let (y0, y1) = this.widget.row_span(index);
        Some(Rect::new(0.0, y0, 0.0, y1))
    }

    /// The selection changed by the user: tell the app, repaint, restyle the items.
    fn collection_selection_changed(&mut self, ctx: &mut EventCtx<'_>, before: &BTreeSet<usize>) {
        let rows = self.collection_mut().shared.borrow().selection.rows.clone();
        if &rows != before {
            ctx.submit_untyped_action(Box::new(TableAction::Selected(rows.into_iter().collect())));
        }
        Self::collection_restyle_later(ctx);
    }

    /// Changes the selection with `f`, then reports it.
    fn collection_select(&mut self, ctx: &mut EventCtx<'_>, f: impl FnOnce(&mut Selection)) {
        let before = {
            let mut state = self.collection_mut().shared.borrow_mut();
            let before = state.selection.rows.clone();
            f(&mut state.selection);
            before
        };
        self.collection_selection_changed(ctx, &before);
    }

    /// Scrolls item `item` into view.
    fn collection_reveal(&self, ctx: &mut EventCtx<'_>, item: usize) {
        let (y0, y1) = self.row_span(item);
        ctx.request_scroll_to(Rect::new(0.0, y0, 0.0, y1));
    }

    pub(super) fn collection_pointer(&mut self, ctx: &mut EventCtx<'_>, event: &PointerEvent) {
        let PointerEvent::Down(PointerButtonEvent {
            button: None | Some(PointerButton::Primary),
            state,
            ..
        }) = event
        else {
            return;
        };
        ctx.request_focus();
        let at = ctx.local_position(state.position);
        let mods = state.modifiers;
        let multiple = self.collection_mut().shared.borrow().multiple;
        match self.item_at(at) {
            Some(item) if mods.shift() && multiple => {
                self.collection_select(ctx, |s| s.extend_to(item));
            }
            Some(item) if command(mods) => self.collection_select(ctx, |s| {
                if multiple || s.rows.contains(&item) {
                    s.toggle(item);
                } else {
                    s.only(item);
                }
            }),
            Some(item) => {
                self.collection_select(ctx, |s| s.only(item));
                if state.count == 2 && self.collection_mut().shared.borrow().activates {
                    ctx.submit_untyped_action(Box::new(TableAction::Activated(item)));
                }
            }
            None if !mods.shift() && !command(mods) => {
                self.collection_select(ctx, Selection::clear);
            }
            None => {}
        }
    }

    pub(super) fn collection_text(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) {
        let key = match event {
            TextEvent::Keyboard(key) if key.state.is_down() => key,
            TextEvent::WindowFocusChange(_) => return Self::collection_restyle_later(ctx),
            _ => return,
        };
        if !ctx.is_focus_target() || self.count == 0 {
            return;
        }
        let mods = key.modifiers;
        let last = self.count - 1;
        let columns = self.columns.max(1);
        let cursor = self.collection_mut().shared.borrow().selection.cursor;
        let target = match &key.key {
            Key::Named(NamedKey::ArrowLeft) => cursor.map_or(last, |c| c.saturating_sub(1)),
            Key::Named(NamedKey::ArrowRight) => cursor.map_or(0, |c| (c + 1).min(last)),
            Key::Named(NamedKey::ArrowUp) => {
                cursor.map_or(last, |c| if c >= columns { c - columns } else { c })
            }
            Key::Named(NamedKey::ArrowDown) => cursor.map_or(0, |c| {
                if c + columns <= last {
                    c + columns
                } else if c / columns < last / columns {
                    // Below a short last row: its last item.
                    last
                } else {
                    c
                }
            }),
            Key::Named(NamedKey::Home) => 0,
            Key::Named(NamedKey::End) => last,
            Key::Named(NamedKey::PageUp) => cursor.map_or(0, |c| {
                (c / columns).saturating_sub(self.collection_page_rows()) * columns + c % columns
            }),
            Key::Named(NamedKey::PageDown) => cursor.map_or(0, |c| {
                (((c / columns) + self.collection_page_rows()) * columns + c % columns).min(last)
            }),
            Key::Named(NamedKey::Enter) => {
                let state = self.collection_mut().shared.borrow();
                if !state.activates {
                    return;
                }
                let item = cursor.filter(|i| state.selection.rows.contains(i));
                drop(state);
                if let Some(item) = item {
                    ctx.submit_untyped_action(Box::new(TableAction::Activated(item)));
                }
                return ctx.set_handled();
            }
            Key::Character(c) if command(mods) && c.eq_ignore_ascii_case("a") => {
                if self.collection_mut().shared.borrow().multiple {
                    let count = self.count;
                    self.collection_select(ctx, |s| s.set(0..count, count));
                    ctx.set_handled();
                }
                return;
            }
            Key::Character(c) if !command(mods) && !mods.ctrl() && !mods.alt() => {
                if let Some(item) = self.collection_type_select(c) {
                    self.collection_select(ctx, |s| s.only(item));
                    self.collection_reveal(ctx, item);
                }
                return ctx.set_handled();
            }
            _ => return,
        };
        let multiple = self.collection_mut().shared.borrow().multiple;
        self.collection_select(ctx, |s| {
            if mods.shift() && multiple {
                s.extend_to(target);
            } else {
                s.only(target);
            }
        });
        self.collection_reveal(ctx, target);
        ctx.set_handled();
    }

    /// The first item whose label starts with what was typed lately.
    fn collection_type_select(&mut self, typed: &str) -> Option<usize> {
        let count = self.count;
        let body = self.collection_mut();
        let prefix = body.typed.push(typed, Instant::now()).to_owned();
        (0..count).find(|&item| {
            (body.label_of)(item).is_some_and(|label| label.to_lowercase().starts_with(&prefix))
        })
    }

    pub(super) fn collection_access(&mut self, ctx: &mut EventCtx<'_>, event: &AccessEvent) {
        if event.action != Action::Click {
            return;
        }
        let target = ctx.target();
        let item = self
            .rows
            .iter()
            .find_map(|(&i, pod)| (pod.id() == target).then_some(i));
        if let Some(item) = item {
            self.collection_select(ctx, |s| s.only(item));
            ctx.set_handled();
        }
    }

    pub(super) fn collection_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let window = self.window.intersect(size.to_rect());
        let mut scene = Scene::new(size.width, size.height);
        scene.fill(&window, theme.colors().content_background);
        self.collection_mut()
            .replayer
            .replay(&scene, ctx.scale_factor(), painter);
    }

    pub(super) fn collection_accessibility(&mut self, node: &mut Node) {
        let (count, columns) = (self.count, self.columns.max(1));
        let body = self.collection_mut();
        node.set_row_count(count.div_ceil(columns));
        node.set_column_count(columns);
        if body.shared.borrow().multiple {
            node.set_multiselectable();
        }
        let cursor = body.shared.borrow().selection.cursor;
        if let Some(pod) = cursor.and_then(|c| self.rows.get(&c)) {
            node.set_active_descendant(NodeId(pod.id().to_raw()));
        }
    }
}

// --- MARK: BUILDER

type OnSelect<M, Id> = Box<dyn FnMut(&[Id]) -> M>;
type OnActivate<M, Id> = Box<dyn FnMut(&Id) -> M>;

/// A scrolling grid of items, each an icon over a label: a collection view.
///
/// The items are named by ids, in order (`ids`), and shown as `item(id)`; like a
/// [`crate::VirtualGrid`]'s items they are built only while they are near the view, so a
/// collection stays fast with very many items. `Id` must be stable: the selection is
/// remembered by it, through [`Handle::set_items`]. `item` can't see the model, so it reads
/// shared data, and [`Handle::reload`] shows items again after the data changes.
///
/// The cells' size follows the icon size ([`icon_size`](Self::icon_size)), and as many
/// columns as fit the width are used. Clicking selects an item (Shift extends the selection
/// from the item last clicked, in order, and Cmd toggles one; both only
/// with [`multiple`](Self::multiple)); clicking between items clears it. The selection is
/// the accent while the collection has keyboard focus and grey without. The arrow keys move
/// the selection over the grid, Left and Right across the end of a row, Home and End go to
/// the first and last item, Page Up and Page Down by a view, and typing selects the first
/// item whose label starts with what was typed. Return or a double-click activates the
/// selected item.
pub struct Collection<M, Id> {
    inner: NewWidget<ScrollWidget>,
    shared: Shared,
    ids: Rc<RefCell<IdList<Id>>>,
    body: WidgetId,
    on_select: Option<OnSelect<M, Id>>,
    on_activate: Option<OnActivate<M, Id>>,
}

impl<M, Id> std::fmt::Debug for Collection<M, Id> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collection")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M: 'static, Id: Clone + Eq + Hash + 'static> Collection<M, Id> {
    /// A collection of the items named by `ids`, each shown as `item(id)`.
    pub fn new(ids: impl IntoIterator<Item = Id>, item: impl FnMut(&Id) -> Item + 'static) -> Self {
        let ids = Rc::new(RefCell::new(IdList::new(ids.into_iter().collect())));
        let count = ids.borrow().ids.len();
        let shared: Shared = Rc::new(RefCell::new(CollectionState {
            multiple: false,
            activates: false,
            selection: Selection::default(),
            emphasized: false,
            icon_size: ICON_SIZE,
            columns: 1,
        }));
        let item = Rc::new(RefCell::new(item));

        let make: MakeRow = {
            let (shared, ids, item) = (Rc::clone(&shared), Rc::clone(&ids), Rc::clone(&item));
            Box::new(move |i| {
                let id = ids.borrow().ids.get(i).cloned();
                let Item { label, icon } =
                    id.map_or_else(|| Item::new(""), |id| (item.borrow_mut())(&id));
                let (look, icon_size) = {
                    let state = shared.borrow();
                    (state.look(i), state.icon())
                };
                let widget = CollectionItemWidget {
                    index: i,
                    shown: TextLine::new(label.clone(), TextStyle::Body),
                    plain: label,
                    fitted: None,
                    icon,
                    icon_size,
                    look,
                    shared: Rc::clone(&shared),
                    replayer: ImagingReplayer::new(),
                };
                (
                    NewWidget::new(widget).erased(),
                    Box::new(Wiring::<M>::default()) as Box<dyn Any>,
                )
            })
        };
        let label_of = {
            let (ids, item) = (Rc::clone(&ids), Rc::clone(&item));
            Box::new(move |i: usize| {
                let id = ids.borrow().ids.get(i).cloned()?;
                let Item { label, .. } = (item.borrow_mut())(&id);
                Some(label)
            })
        };
        let (width, height) = cell_size(ICON_SIZE);
        let body = CollectionBody {
            shared: Rc::clone(&shared),
            ids: Rc::clone(&ids) as Rc<RefCell<dyn Ids>>,
            label_of,
            typed: TypeSelect::default(),
            replayer: ImagingReplayer::new(),
        };
        let rows = NewWidget::new(
            RowsWidget::new(count, height, make)
                .with_cell_width(width)
                .with_collection(body),
        );
        let body = rows.id();
        let mut scroll = ScrollWidget::new(rows.erased(), ScrollAxes::Vertical, true);
        scroll.frame = Frame::Border;
        Self {
            inner: NewWidget::new(scroll),
            shared,
            ids,
            body,
            on_select: None,
            on_activate: None,
        }
    }

    /// The icons' width and height, in logical pixels (kept between 16 and 128); the cells
    /// grow with it. The default is 48.
    #[must_use]
    pub fn icon_size(self, size: f64) -> Self {
        self.shared.borrow_mut().icon_size = size;
        self
    }

    /// Lets the user select more than one item.
    #[must_use]
    pub fn multiple(self, multiple: bool) -> Self {
        self.shared.borrow_mut().multiple = multiple;
        self
    }

    /// Starts with the items `ids` selected.
    #[must_use]
    pub fn selected(self, ids: impl IntoIterator<Item = Id>) -> Self {
        {
            let list = self.ids.borrow();
            let rows: Vec<usize> = ids
                .into_iter()
                .filter_map(|id| list.index.get(&id).copied())
                .collect();
            self.shared.borrow_mut().selection.set(rows, list.ids.len());
        }
        self
    }

    /// Sends the message `make` returns, given the selected items (in order), when the
    /// user changes the selection. Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_select(mut self, make: impl FnMut(&[Id]) -> M + 'static) -> Self {
        self.on_select = Some(Box::new(make));
        self
    }

    /// Sends the message `make` returns, given the item, when the user double-clicks it or
    /// presses Return. Without it, Return goes on to the default button.
    #[must_use]
    pub fn on_activate(mut self, make: impl FnMut(&Id) -> M + 'static) -> Self {
        self.shared.borrow_mut().activates = true;
        self.on_activate = Some(Box::new(make));
        self
    }

    /// The width it asks for, in logical pixels.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.inner.widget.width = Some(width.max(0.0));
        self
    }

    /// The height it asks for, in logical pixels.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.inner.widget.height = Some(height.max(0.0));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this collection.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static, Id: Clone + Eq + Hash + 'static> From<Collection<M, Id>> for Element<M> {
    fn from(collection: Collection<M, Id>) -> Self {
        let Collection {
            inner,
            ids,
            body,
            on_select,
            on_activate,
            ..
        } = collection;
        let mut element = Element::new(inner.erased());
        let (mut on_select, mut on_activate) = (on_select, on_activate);
        if on_select.is_some() || on_activate.is_some() {
            // The body's actions speak in indices; the app's in ids.
            element.on_action(body, move |action| {
                let list = ids.borrow();
                match action.downcast_ref::<TableAction>()? {
                    TableAction::Selected(rows) => {
                        let selected: Vec<Id> = rows
                            .iter()
                            .filter_map(|&r| list.ids.get(r).cloned())
                            .collect();
                        on_select.as_mut().map(|f| f(&selected))
                    }
                    TableAction::Activated(row) => {
                        let id = list.ids.get(*row)?;
                        on_activate.as_mut().map(|f| f(id))
                    }
                }
            });
        }
        element
    }
}

impl<M, Id: Clone + Eq + Hash + 'static> Handle<Collection<M, Id>> {
    /// Replaces the items with those named by `ids`. The selection is kept by id; ids that
    /// are gone are deselected. Sends no message.
    pub fn set_items<N>(&self, cx: &mut Cx<'_, N>, ids: impl IntoIterator<Item = Id>) {
        let ids: Vec<Id> = ids.into_iter().collect();
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| RowsWidget::collection_set_items(rows, ids));
        });
    }

    /// Builds the visible items again, after the data behind `item` changed.
    pub fn reload<N>(&self, cx: &mut Cx<'_, N>) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, RowsWidget::edit_reload);
        });
    }

    /// Selects the items `ids` (those that exist). Sends no message.
    pub fn set_selection<N>(&self, cx: &mut Cx<'_, N>, ids: impl IntoIterator<Item = Id>) {
        let ids: Vec<Id> = ids.into_iter().collect();
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| {
                RowsWidget::collection_set_selection(rows, ids);
            });
        });
    }

    /// Scrolls as little as it takes to show the row of item `id` whole.
    pub fn scroll_to<N>(&self, cx: &mut Cx<'_, N>, id: &Id) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            let span =
                ScrollWidget::with_rows(&mut w, |rows| RowsWidget::collection_span(rows, id));
            if let Some(Some(target)) = span {
                ScrollWidget::edit_reveal(&mut w, target);
            }
        });
    }

    /// Changes the icon size (see [`Collection::icon_size`]); the items are built again.
    pub fn set_icon_size<N>(&self, cx: &mut Cx<'_, N>, size: f64) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| {
                RowsWidget::collection_set_icon_size(rows, size);
            });
        });
    }

    /// Enables or disables the collection.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ScrollWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
