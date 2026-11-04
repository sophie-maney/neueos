//! Tables (ADR 0048): a header of column titles over virtual rows of cells, in a bordered
//! scroll view that scrolls both ways.
//!
//! The pieces: the scroll view ([`ScrollWidget`], [`Frame::Border`]) holds the header
//! ([`HeaderWidget`], above the viewport, following the content sideways) and the body (a
//! [`RowsWidget`] with a [`TableBody`]). The body owns the selection, takes keyboard focus,
//! paints the stripes and the selection behind its rows, and sends [`TableAction`]s. Each
//! row ([`TableRowWidget`]) lays out one [`CellWidget`] per column; a cell draws its text,
//! or holds an app element. The header paints the bar and handles sorting and resizing;
//! its [`ColumnHeaderWidget`]s draw the titles and are the column headers for assistive
//! technology.
//!
//! The column widths, the selection and the options live in a [`TableState`], shared
//! between the builder, the body and the closure that builds rows, so a row is built
//! right; the body pushes changes into the rows that exist.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::rc::Rc;
use std::time::Instant;

use masonry_core::accesskit::{Action, Node, NodeId, Role, SortDirection};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx,
    UsesProperty, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Point, Rect, Size, Vec2};
use masonry_core::layout::{LenDef, LenReq, Length, SizeDef};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{
    TableHeader, column_edges, disclosure_rect, disclosure_triangle, outline_text_inset,
    table_header,
};
use neue_theme::materials::table::{
    CELL_INSET, COLUMN_WIDTH, DIVIDER_GRAB, HEADER_HEIGHT, MIN_COLUMN_WIDTH, Palette, ROW_HEIGHT,
    SORT_ARROW_INSET, SORT_ARROW_WIDTH, TITLE_INSET,
};

use super::menu::TypeSelect;
use super::outline::{Change, FlatTree, RowInfo, SharedTree, Tree};
use super::rows::{MakeRow, RowsWidget};
use super::scroll::{Frame, ScrollAxes, ScrollWidget};
use super::stack::Align;
use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::element::{Element, Handle, Wiring};
use crate::theme::ThemeProp;

/// Which way a sorted column runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SortOrder {
    /// Smallest first (A to Z); the indicator points up.
    #[default]
    Ascending,
    /// Largest first; the indicator points down.
    Descending,
}

impl SortOrder {
    /// The other way.
    #[must_use]
    pub fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// One column of a [`Table`]: its title, width and how its text lines up.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    title: String,
    width: f64,
    min_width: f64,
    max_width: f64,
    align: Align,
    sortable: bool,
}

impl Column {
    /// A column titled `title`, of the default width, its text at the start.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            width: COLUMN_WIDTH,
            min_width: MIN_COLUMN_WIDTH,
            max_width: f64::INFINITY,
            align: Align::Start,
            sortable: false,
        }
    }

    /// Its width in logical pixels, before the user resizes it.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.width = width.max(0.0);
        self
    }

    /// The narrowest the user can make it.
    #[must_use]
    pub fn min_width(mut self, width: f64) -> Self {
        self.min_width = width.max(0.0);
        self
    }

    /// The widest the user can make it.
    #[must_use]
    pub fn max_width(mut self, width: f64) -> Self {
        self.max_width = width.max(0.0);
        self
    }

    /// Where text sits in its cells and title: [`Align::Start`] (the default),
    /// [`Align::Center`] or [`Align::End`] (numbers). `Fill` reads as `Start`.
    #[must_use]
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Lets a click on the title sort by this column (see [`Table::on_sort`]).
    #[must_use]
    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    /// The width kept inside the column's limits.
    fn clamp(&self, width: f64) -> f64 {
        width
            .max(self.min_width)
            .min(self.max_width.max(self.min_width))
    }
}

/// What a [`Table`] shows in one cell: text (drawn by the table, white on a focused
/// selection) or any element (a checkbox, a button), which keeps its own colours.
pub struct Cell<M>(CellKind<M>);

enum CellKind<M> {
    Text(String),
    Element(Box<Element<M>>),
}

impl<M> std::fmt::Debug for Cell<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            CellKind::Text(text) => f.debug_tuple("Cell::text").field(text).finish(),
            CellKind::Element(e) => f.debug_tuple("Cell::element").field(e).finish(),
        }
    }
}

impl<M> Cell<M> {
    /// A line of text. Type-select matches the first column's text cells.
    pub fn text(text: impl Into<String>) -> Self {
        Self(CellKind::Text(text.into()))
    }

    /// Any element, inset like text and centred in the row's height.
    pub fn element(element: impl Into<Element<M>>) -> Self {
        Self(CellKind::Element(Box::new(element.into())))
    }

    fn into_text(self) -> Option<String> {
        match self.0 {
            CellKind::Text(text) => Some(text),
            CellKind::Element(_) => None,
        }
    }
}

impl<M> From<&str> for Cell<M> {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

impl<M> From<String> for Cell<M> {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

/// What a table's body sends: the selection changed (by the user), or a row was activated
/// (double-click, Return).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TableAction {
    Selected(Vec<usize>),
    Activated(usize),
}

/// What a table's header sends: the user sorted by a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sorted(pub(crate) usize, pub(crate) SortOrder);

/// The selected rows, the row a Shift-extension starts from (the anchor) and the row the
/// keys move from (the cursor).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) rows: BTreeSet<usize>,
    anchor: Option<usize>,
    cursor: Option<usize>,
}

impl Selection {
    /// Selects only `row`.
    fn only(&mut self, row: usize) {
        self.rows = BTreeSet::from([row]);
        self.anchor = Some(row);
        self.cursor = Some(row);
    }

    /// Adds `row`, or takes it out.
    fn toggle(&mut self, row: usize) {
        if !self.rows.remove(&row) {
            self.rows.insert(row);
        }
        self.anchor = Some(row);
        self.cursor = Some(row);
    }

    /// Selects the rows from the anchor to `row`.
    fn extend_to(&mut self, row: usize) {
        let anchor = *self.anchor.get_or_insert(row);
        self.rows = (anchor.min(row)..=anchor.max(row)).collect();
        self.cursor = Some(row);
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    /// Sets the selection from the app.
    pub(super) fn set(&mut self, rows: impl IntoIterator<Item = usize>, count: usize) {
        self.rows = rows.into_iter().filter(|&r| r < count).collect();
        self.anchor = self.rows.first().copied();
        self.cursor = self.rows.last().copied();
    }

    /// The rows to follow through a change of the outline: the selected rows, then the
    /// anchor and the cursor (those that are set).
    fn keep(&self) -> Vec<usize> {
        self.rows
            .iter()
            .copied()
            .chain(self.anchor)
            .chain(self.cursor)
            .collect()
    }

    /// Follows [`Self::keep`]'s rows to where they are now. Rows that are no longer
    /// visible leave the selection; if `collapsed` hid some, it is selected instead.
    fn remap(&mut self, kept: &[Option<usize>], collapsed: Option<usize>) {
        let mut moved = kept.iter().copied();
        let before = self.rows.len();
        let mut rows: BTreeSet<usize> = moved.by_ref().take(before).flatten().collect();
        let mut anchor = self.anchor.and_then(|_| moved.next().flatten());
        let mut cursor = self.cursor.and_then(|_| moved.next().flatten());
        let lost = rows.len() < before;
        if let Some(row) = collapsed.filter(|_| lost) {
            rows.insert(row);
            anchor = anchor.or(self.anchor.map(|_| row));
            cursor = cursor.or(self.cursor.map(|_| row));
        }
        self.rows = rows;
        self.anchor = anchor;
        self.cursor = cursor;
    }

    /// Forgets rows at or past `count`.
    fn forget_past(&mut self, count: usize) {
        self.rows.retain(|&r| r < count);
        self.anchor = self.anchor.filter(|&r| r < count);
        self.cursor = self.cursor.filter(|&r| r < count);
    }
}

/// What rows are built from and pushed to (widths, the selection), and the options.
#[derive(Debug, Default)]
pub(crate) struct TableState {
    multiple: bool,
    alternating: bool,
    /// The app listens for activation (Return would otherwise reach the default button).
    pub(crate) activates: bool,
    /// Column widths as set (by the app, then by resizing).
    pub(crate) widths: Vec<f64>,
    pub(crate) selection: Selection,
    /// The body has focus in a focused window: the selection is in the accent.
    pub(crate) emphasized: bool,
    /// An outline view's flattened tree (ADR 0049); a plain table has none.
    pub(crate) outline: Option<SharedTree>,
}

impl TableState {
    /// The outline row `row` is, if this is an outline.
    fn outline_row(&self, row: usize) -> Option<RowInfo> {
        self.outline.as_ref()?.borrow().info(row)
    }

    fn look(&self, row: usize) -> Look {
        Look {
            selected: self.selection.rows.contains(&row),
            emphasized: self.emphasized,
        }
    }
}

type Shared = Rc<RefCell<TableState>>;

/// How a row's cells draw their text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Look {
    selected: bool,
    emphasized: bool,
}

/// The widths columns are drawn at in `total`: as set, the last one stretched to fill.
fn stretched(widths: &[f64], total: f64) -> Vec<f64> {
    let mut out = widths.to_vec();
    let sum: f64 = widths.iter().sum();
    if let Some(last) = out.last_mut()
        && sum < total
    {
        *last += total - sum;
    }
    out
}

/// Where text `width` wide sits in a box `x0..x1` with `inset` at either side.
fn aligned(align: Align, (x0, x1): (f64, f64), width: f64, inset: f64) -> f64 {
    match align {
        Align::Start | Align::Fill => x0 + inset,
        Align::Center => x0 + (x1 - x0 - width) / 2.0,
        Align::End => x1 - inset - width,
    }
    .round()
}

/// The command key: Cmd on macOS, Ctrl elsewhere.
fn command(mods: Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        mods.meta()
    } else {
        mods.ctrl()
    }
}

// --- MARK: CELL

enum CellContent {
    Text(Box<TextLine>),
    Element(WidgetPod<dyn Widget>),
}

/// One cell: text it draws itself, or an app element.
pub(crate) struct CellWidget {
    column: usize,
    align: Align,
    content: CellContent,
    look: Look,
    /// The first cell of an outline row: its level and disclosure.
    outline: Option<RowInfo>,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for CellWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match &self.content {
            CellContent::Text(line) => Some(line.text()),
            CellContent::Element(_) => None,
        };
        f.debug_struct("CellWidget")
            .field("column", &self.column)
            .field("text", &text)
            .field("look", &self.look)
            .finish_non_exhaustive()
    }
}

impl CellWidget {
    pub(crate) fn text(&self) -> Option<&str> {
        match &self.content {
            CellContent::Text(line) => Some(line.text()),
            CellContent::Element(_) => None,
        }
    }

    /// How far in from the cell's edge the content starts, beyond the usual inset.
    fn lead(&self) -> f64 {
        self.outline
            .map_or(0.0, |o| outline_text_inset(o.level) - CELL_INSET)
    }

    fn set_look(this: &mut WidgetMut<'_, Self>, look: Look) {
        if this.widget.look != look {
            this.widget.look = look;
            this.ctx.request_paint_only();
        }
    }
}

impl UsesProperty<ThemeProp> for CellWidget {}

impl Widget for CellWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        if let CellContent::Element(child) = &mut self.content {
            ctx.register_child(child);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) => ctx.request_paint_only(),
            Update::FontsChanged => {
                if let CellContent::Text(line) = &mut self.content {
                    line.invalidate();
                }
                ctx.request_layout();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
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
        // The row gives cells their size.
        match (axis, len_req) {
            (_, LenReq::FitContent(space)) => space,
            (Axis::Vertical, _) => Length::px(ROW_HEIGHT),
            (Axis::Horizontal, _) => Length::ZERO,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.set_clip_path(size.to_rect());
        let lead = self.lead();
        if let CellContent::Element(child) = &mut self.content {
            let room = Size::new((size.width - 2.0 * CELL_INSET - lead).max(0.0), size.height);
            let auto = SizeDef::new(LenDef::MaxContent, LenDef::MaxContent);
            let want = ctx.compute_size(child, auto, room.into()).min(room);
            ctx.run_layout(child, want);
            let y = ((size.height - want.height) / 2.0).round();
            ctx.place_child(child, Point::new(CELL_INSET + lead, y));
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let colors = theme.colors();
        let on_selection = self.look.selected && self.look.emphasized;
        let size = ctx.content_box().size();
        if let Some(outline) = self.outline.filter(|o| o.expandable) {
            let color = if ctx.is_disabled() {
                colors.disabled_label
            } else if on_selection {
                colors.selected_label
            } else {
                Palette::new(theme.variant).disclosure
            };
            let mut scene = Scene::new(size.width, size.height);
            let column = disclosure_rect(size.to_rect(), outline.level);
            disclosure_triangle(&mut scene, column, outline.expanded, color);
            self.replayer.replay(&scene, ctx.scale_factor(), painter);
        }
        let lead = self.lead();
        let CellContent::Text(line) = &mut self.content else {
            return;
        };
        let color = if ctx.is_disabled() {
            colors.disabled_label
        } else if on_selection {
            colors.selected_label
        } else {
            colors.label
        };
        let text = line.size(ctx.text_contexts());
        let x = aligned(self.align, (lead, size.width), text.width, CELL_INSET);
        let y = ((size.height - text.height) / 2.0).round();
        line.paint(painter, Affine::translate(Vec2::new(x, y)), color);
    }

    fn accessibility_role(&self) -> Role {
        Role::Cell
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_column_index(self.column);
        if let CellContent::Text(line) = &self.content {
            node.set_label(line.text());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        match &self.content {
            CellContent::Element(child) => ChildrenIds::from_slice(&[child.id()]),
            CellContent::Text(_) => ChildrenIds::new(),
        }
    }

    fn get_debug_text(&self) -> Option<String> {
        self.text().map(str::to_owned)
    }
}

// --- MARK: ROW

/// One row: its cells side by side at the column widths.
#[derive(Debug)]
pub(crate) struct TableRowWidget {
    index: usize,
    cells: Vec<WidgetPod<CellWidget>>,
    /// Column widths as set; the last is stretched to the row's width.
    widths: Vec<f64>,
    look: Look,
    /// An outline row's level and state, for assistive technology.
    outline: Option<RowInfo>,
}

impl TableRowWidget {
    /// The cells' widgets, for tests.
    #[cfg(test)]
    pub(crate) fn cell_ids(&self) -> Vec<WidgetId> {
        self.cells.iter().map(WidgetPod::id).collect()
    }

    fn set_look(this: &mut WidgetMut<'_, Self>, look: Look) {
        if this.widget.look == look {
            return;
        }
        this.widget.look = look;
        for cell in &mut this.widget.cells {
            CellWidget::set_look(&mut this.ctx.get_mut(cell), look);
        }
        this.ctx.request_accessibility_update();
    }

    fn set_widths(this: &mut WidgetMut<'_, Self>, widths: &[f64]) {
        widths.clone_into(&mut this.widget.widths);
        this.ctx.request_layout();
    }
}

impl Widget for TableRowWidget {
    type Action = NoAction;

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
        // A click selects the row: the body handles it as the event bubbles up.
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for cell in &mut self.cells {
            ctx.register_child(cell);
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
        match (axis, len_req) {
            (_, LenReq::FitContent(space)) => space,
            (Axis::Vertical, _) => Length::px(ROW_HEIGHT),
            (Axis::Horizontal, _) => Length::px(self.widths.iter().sum()),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let widths = stretched(&self.widths, size.width);
        let edges = column_edges(size.to_rect(), &widths);
        for (cell, &(x0, x1)) in self.cells.iter_mut().zip(&edges) {
            ctx.run_layout(cell, Size::new(x1 - x0, size.height));
            ctx.place_child(cell, Point::new(x0, 0.0));
        }
        ctx.clear_baselines();
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::Row
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_row_index(self.index);
        node.set_selected(self.look.selected);
        node.add_action(Action::Click);
        if let Some(o) = self.outline {
            node.set_level(o.level + 1);
            node.set_size_of_set(o.set_size);
            node.set_position_in_set(o.position);
            if o.expandable {
                node.set_expanded(o.expanded);
                node.add_action(if o.expanded {
                    Action::Collapse
                } else {
                    Action::Expand
                });
            }
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        self.cells.iter().map(WidgetPod::id).collect()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("row {}", self.index))
    }
}

// --- MARK: BODY

/// What makes a [`RowsWidget`] a table's body.
pub(crate) struct TableBody {
    shared: Shared,
    columns: usize,
    /// The first column's text of a row, if it is text (type-select).
    text_of: Box<dyn FnMut(usize) -> Option<String>>,
    typed: TypeSelect,
    /// The pointer went down on a row: moving it with the button held selects.
    dragging: bool,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for TableBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableBody")
            .field("state", &self.shared.borrow())
            .finish_non_exhaustive()
    }
}

impl TableBody {
    /// How wide the columns are together.
    pub(crate) fn columns_width(&self) -> f64 {
        self.shared.borrow().widths.iter().sum()
    }

    /// Edits the outline with `f` (given the rows to follow), and moves the selection to
    /// where its rows went. The new number of rows, if anything changed.
    fn apply(
        &self,
        f: impl FnOnce(&mut dyn FlatTree, &[usize]) -> Option<Change>,
    ) -> Option<usize> {
        let tree = self.shared.borrow().outline.clone()?;
        let mut state = self.shared.borrow_mut();
        let keep = state.selection.keep();
        let change = f(&mut *tree.borrow_mut(), &keep)?;
        state.selection.remap(&change.kept, change.collapsed);
        Some(change.len)
    }

    /// What assistive technology takes the body for: a table, or an outline's tree grid.
    pub(crate) fn role(&self) -> Role {
        if self.shared.borrow().outline.is_some() {
            Role::TreeGrid
        } else {
            Role::Table
        }
    }

    /// How many rows an outline has now.
    pub(crate) fn outline_len(&self) -> Option<usize> {
        Some(self.shared.borrow().outline.as_ref()?.borrow().len())
    }

    /// The tree an outline's rows are, for a type the caller knows.
    pub(crate) fn tree(&self) -> Option<SharedTree> {
        self.shared.borrow().outline.clone()
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
    /// The table. Only the table hooks call this, and `RowsWidget` calls them only when
    /// it has a table.
    #[expect(clippy::expect_used, reason = "the hooks run only on a table's body")]
    fn table_mut(&mut self) -> &mut TableBody {
        self.table
            .as_deref_mut()
            .expect("a table's body (checked by the caller)")
    }

    /// The row under `y` (body coordinates), if there is one.
    fn row_at(&self, y: f64) -> Option<usize> {
        if y < 0.0 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "non-negative, and saturating"
        )]
        let row = (y / self.row_height).floor() as usize;
        (row < self.count).then_some(row)
    }

    /// Rows moved by Page Up and Page Down: a view less one.
    fn page_rows(&self) -> usize {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "small, non-negative"
        )]
        let rows = (self.window.height() / self.row_height).floor() as usize;
        rows.saturating_sub(1).max(1)
    }

    /// Pushes the selection (selection, emphasis) to the rows, after this event.
    pub(super) fn restyle_later(ctx: &mut impl Later) {
        ctx.later(|mut w| Self::restyle(&mut w.downcast()));
    }

    /// Pushes the selection to the rows that exist.
    fn restyle(this: &mut WidgetMut<'_, Self>) {
        let Some(table) = &this.widget.table else {
            return;
        };
        let emphasized = this.ctx.is_focus_target() && this.ctx.is_window_focused();
        table.shared.borrow_mut().emphasized = emphasized;
        let state = table.shared.borrow();
        for (&i, row) in &mut this.widget.rows {
            let mut row = this.ctx.get_mut(row);
            if let Some(mut row) = row.try_downcast::<TableRowWidget>() {
                TableRowWidget::set_look(&mut row, state.look(i));
            }
        }
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// Changes the column widths (a column was resized).
    pub(super) fn set_widths(this: &mut WidgetMut<'_, Self>, widths: &[f64]) {
        let Some(table) = &this.widget.table else {
            return;
        };
        widths.clone_into(&mut table.shared.borrow_mut().widths);
        for row in this.widget.rows.values_mut() {
            let mut row = this.ctx.get_mut(row);
            if let Some(mut row) = row.try_downcast::<TableRowWidget>() {
                TableRowWidget::set_widths(&mut row, widths);
            }
        }
        this.ctx.request_layout();
    }

    /// Changes an outline's tree from the app with `f` (given the rows to follow), then
    /// shows the new rows. No message goes back.
    pub(super) fn edit_tree(
        this: &mut WidgetMut<'_, Self>,
        f: impl FnOnce(&mut dyn FlatTree, &[usize]) -> Option<Change>,
    ) {
        let Some(len) = this.widget.table.as_ref().and_then(|t| t.apply(f)) else {
            return;
        };
        Self::edit_count_and_reload(this, len);
    }

    /// Runs `f` on an outline's tree, if this is one over nodes of type `Id`.
    pub(super) fn with_tree<Id, R>(
        this: &mut WidgetMut<'_, Self>,
        f: impl FnOnce(&mut Tree<Id>) -> R,
    ) -> Option<R>
    where
        Id: Clone + Eq + Hash + 'static,
    {
        let tree = this.widget.table.as_ref()?.tree()?;
        let mut tree = tree.borrow_mut();
        Some(f(tree.as_any_mut().downcast_mut::<Tree<Id>>()?))
    }

    /// Sets the selection from the app: no message goes back.
    pub(super) fn set_selection(this: &mut WidgetMut<'_, Self>, rows: Vec<usize>) {
        let count = this.widget.count;
        let Some(table) = &this.widget.table else {
            return;
        };
        table.shared.borrow_mut().selection.set(rows, count);
        Self::restyle(this);
    }

    /// The selection changed by the user: tell the app, repaint, restyle the rows.
    fn selection_changed(&mut self, ctx: &mut EventCtx<'_>, before: &BTreeSet<usize>) {
        self.report_selection(ctx, before);
        Self::restyle_later(ctx);
    }

    /// Tells the app the selection changed from `before`, if it did.
    fn report_selection(&mut self, ctx: &mut EventCtx<'_>, before: &BTreeSet<usize>) {
        let rows = self.table_mut().shared.borrow().selection.rows.clone();
        if &rows != before {
            ctx.submit_untyped_action(Box::new(TableAction::Selected(rows.into_iter().collect())));
        }
    }

    /// Changes the selection with `f`, then reports it.
    fn select(&mut self, ctx: &mut EventCtx<'_>, f: impl FnOnce(&mut Selection)) {
        let before = {
            let table = self.table_mut();
            let mut state = table.shared.borrow_mut();
            let before = state.selection.rows.clone();
            f(&mut state.selection);
            before
        };
        self.selection_changed(ctx, &before);
    }

    /// Scrolls row `row` into view (keeping the horizontal scroll where it is).
    fn reveal_row(&self, ctx: &mut EventCtx<'_>, row: usize) {
        let (y0, y1) = self.row_span(row);
        let x = self.window.x0;
        ctx.request_scroll_to(Rect::new(x, y0, x, y1));
    }

    /// Row `row` of the outline, if this is one.
    fn outline_row(&mut self, row: usize) -> Option<RowInfo> {
        self.table_mut().shared.borrow().outline_row(row)
    }

    /// After an outline's rows changed from an event: rebuild them, tell the app if the
    /// selection moved off rows.
    fn outline_changed(&mut self, ctx: &mut EventCtx<'_>, before: &BTreeSet<usize>, len: usize) {
        ctx.request_layout();
        ctx.request_accessibility_update();
        ctx.later(move |mut w| {
            let mut body = w.downcast::<RowsWidget>();
            Self::edit_count_and_reload(&mut body, len);
        });
        // The rows are built again with the selection as it is: no restyling.
        self.report_selection(ctx, before);
    }

    /// Expands or collapses outline row `row` (`None`: the other way), with its
    /// descendants if `recursive`.
    fn outline_toggle(
        &mut self,
        ctx: &mut EventCtx<'_>,
        row: usize,
        expanded: Option<bool>,
        recursive: bool,
    ) {
        let before = self.table_mut().shared.borrow().selection.rows.clone();
        let len = self.table_mut().apply(|tree, keep| {
            let expanded = expanded.unwrap_or_else(|| !tree.info(row).is_some_and(|i| i.expanded));
            tree.set_expanded(row, expanded, recursive, keep)
        });
        if let Some(len) = len {
            self.outline_changed(ctx, &before, len);
        }
    }

    /// Right and Left in an outline: opens or closes the cursor row, or moves into or out
    /// of it. The row to select instead, if the key moves; `Err` if it did nothing.
    fn outline_arrow(
        &mut self,
        ctx: &mut EventCtx<'_>,
        row: usize,
        right: bool,
        recursive: bool,
    ) -> Result<Option<usize>, ()> {
        let Some(info) = self.outline_row(row) else {
            return Err(());
        };
        if right {
            if info.expandable && (!info.expanded || recursive) {
                self.outline_toggle(ctx, row, Some(true), recursive);
                Ok(None)
            } else if info.expanded {
                Ok(Some(row + 1))
            } else {
                Err(())
            }
        } else if info.expanded {
            self.outline_toggle(ctx, row, Some(false), recursive);
            Ok(None)
        } else {
            let tree = self.table_mut().tree().ok_or(())?;
            let parent = tree.borrow().parent(row);
            parent.map(Some).ok_or(())
        }
    }

    pub(super) fn table_pointer(&mut self, ctx: &mut EventCtx<'_>, event: &PointerEvent) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                let at = ctx.local_position(state.position);
                let y = at.y;
                let mods = state.modifiers;
                let multiple = self.table_mut().shared.borrow().multiple;
                let row = self.row_at(y);
                // A press on a disclosure triangle opens or closes the row, and leaves the
                // selection alone.
                if let Some(row) = row
                    && let Some(info) = self.outline_row(row).filter(|i| i.expandable)
                {
                    let cell = Rect::new(0.0, 0.0, f64::INFINITY, self.row_height);
                    let column = disclosure_rect(cell, info.level);
                    if at.x >= column.x0 && at.x < column.x1 {
                        self.table_mut().dragging = false;
                        return self.outline_toggle(ctx, row, None, mods.alt());
                    }
                }
                self.table_mut().dragging = row.is_some();
                match row {
                    Some(row) if mods.shift() && multiple => {
                        self.select(ctx, |s| s.extend_to(row));
                    }
                    Some(row) if command(mods) => {
                        self.select(ctx, |s| {
                            if multiple || s.rows.contains(&row) {
                                s.toggle(row);
                            } else {
                                s.only(row);
                            }
                        });
                    }
                    Some(row) => {
                        self.select(ctx, |s| s.only(row));
                        if state.count == 2 && self.table_mut().shared.borrow().activates {
                            ctx.submit_untyped_action(Box::new(TableAction::Activated(row)));
                        }
                    }
                    None if !mods.shift() && !command(mods) => self.select(ctx, Selection::clear),
                    None => {}
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. })
                if ctx.is_active() && self.table_mut().dragging =>
            {
                // Dragging extends the selection (or moves a single one).
                let y = ctx.local_position(current.position).y;
                let last = self.count.checked_sub(1);
                let row = self.row_at(y.max(0.0)).or(last);
                let Some(row) = row else { return };
                let multiple = self.table_mut().shared.borrow().multiple;
                let cursor = self.table_mut().shared.borrow().selection.cursor;
                if cursor != Some(row) && !command(current.modifiers) {
                    self.select(ctx, |s| {
                        if multiple {
                            s.extend_to(row);
                        } else {
                            s.only(row);
                        }
                    });
                    self.reveal_row(ctx, row);
                }
            }
            PointerEvent::Up(..) | PointerEvent::Cancel(..) => self.table_mut().dragging = false,
            _ => {}
        }
    }

    pub(super) fn table_text(&mut self, ctx: &mut EventCtx<'_>, event: &TextEvent) {
        let key = match event {
            TextEvent::Keyboard(key) if key.state.is_down() => key,
            TextEvent::WindowFocusChange(_) => return Self::restyle_later(ctx),
            _ => return,
        };
        if !ctx.is_focus_target() || self.count == 0 {
            return;
        }
        let mods = key.modifiers;
        let last = self.count - 1;
        let cursor = self.table_mut().shared.borrow().selection.cursor;
        let target = match &key.key {
            Key::Named(NamedKey::ArrowUp) => cursor.map_or(last, |c| c.saturating_sub(1)),
            Key::Named(NamedKey::ArrowDown) => cursor.map_or(0, |c| (c + 1).min(last)),
            Key::Named(NamedKey::Home) => 0,
            Key::Named(NamedKey::End) => last,
            Key::Named(NamedKey::PageUp) => {
                cursor.map_or(0, |c| c.saturating_sub(self.page_rows()))
            }
            Key::Named(NamedKey::PageDown) => {
                cursor.map_or(0, |c| (c + self.page_rows()).min(last))
            }
            Key::Named(arrow @ (NamedKey::ArrowRight | NamedKey::ArrowLeft))
                if self.table_mut().shared.borrow().outline.is_some() =>
            {
                let Some(row) = cursor else { return };
                let right = *arrow == NamedKey::ArrowRight;
                match self.outline_arrow(ctx, row, right, mods.alt()) {
                    Ok(Some(target)) => target,
                    Ok(None) => return ctx.set_handled(),
                    Err(()) => return,
                }
            }
            Key::Named(NamedKey::Enter) => {
                let state = self.table_mut().shared.borrow();
                if !state.activates {
                    return;
                }
                let row = cursor.filter(|r| state.selection.rows.contains(r));
                drop(state);
                if let Some(row) = row {
                    ctx.submit_untyped_action(Box::new(TableAction::Activated(row)));
                }
                return ctx.set_handled();
            }
            Key::Character(c) if command(mods) && c.eq_ignore_ascii_case("a") => {
                if self.table_mut().shared.borrow().multiple {
                    let count = self.count;
                    self.select(ctx, |s| s.set(0..count, count));
                    ctx.set_handled();
                }
                return;
            }
            Key::Character(c) if !command(mods) && !mods.ctrl() && !mods.alt() => {
                if let Some(row) = self.type_select(c) {
                    self.select(ctx, |s| s.only(row));
                    self.reveal_row(ctx, row);
                }
                return ctx.set_handled();
            }
            _ => return,
        };
        let multiple = self.table_mut().shared.borrow().multiple;
        self.select(ctx, |s| {
            if mods.shift() && multiple {
                s.extend_to(target);
            } else {
                s.only(target);
            }
        });
        self.reveal_row(ctx, target);
        ctx.set_handled();
    }

    /// The first row whose first-column text starts with what was typed lately.
    fn type_select(&mut self, typed: &str) -> Option<usize> {
        let count = self.count;
        let table = self.table_mut();
        let prefix = table.typed.push(typed, Instant::now()).to_owned();
        (0..count).find(|&row| {
            (table.text_of)(row).is_some_and(|text| text.to_lowercase().starts_with(&prefix))
        })
    }

    pub(super) fn table_access(&mut self, ctx: &mut EventCtx<'_>, event: &AccessEvent) {
        if !matches!(
            event.action,
            Action::Click | Action::Expand | Action::Collapse
        ) {
            return;
        }
        let target = ctx.target();
        let row = self
            .rows
            .iter()
            .find_map(|(&i, pod)| (pod.id() == target).then_some(i));
        let Some(row) = row else { return };
        match event.action {
            Action::Expand => self.outline_toggle(ctx, row, Some(true), false),
            Action::Collapse => self.outline_toggle(ctx, row, Some(false), false),
            _ => self.select(ctx, |s| s.only(row)),
        }
        ctx.set_handled();
    }

    pub(super) fn table_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let colors = theme.colors();
        let size = ctx.content_box().size();
        let window = self.window.intersect(size.to_rect());
        let shared = Rc::clone(&self.table_mut().shared);
        let state = shared.borrow();
        let focused = state.emphasized && !ctx.is_disabled();
        let mut scene = Scene::new(size.width, size.height);
        scene.fill(&window, colors.content_background);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "non-negative, and saturating"
        )]
        let (first, last) = (
            (window.y0 / self.row_height).floor() as usize,
            (window.y1 / self.row_height).ceil() as usize,
        );
        // Stripes go on below the last row, to the bottom of the view.
        for i in first..last {
            let (y0, y1) = self.row_span(i);
            let band = Rect::new(window.x0, y0, window.x1, y1);
            if i < self.count && state.selection.rows.contains(&i) {
                let fill = if focused {
                    colors.selection
                } else {
                    colors.unfocused_selection
                };
                scene.fill(&band, fill);
            } else if state.alternating && i % 2 == 1 {
                scene.fill(&band, colors.alternate_row);
            }
        }
        drop(state);
        self.table_mut()
            .replayer
            .replay(&scene, ctx.scale_factor(), painter);
    }

    pub(super) fn table_accessibility(&mut self, node: &mut Node) {
        let count = self.count;
        let table = self.table_mut();
        node.set_row_count(count);
        node.set_column_count(table.columns);
        if table.shared.borrow().multiple {
            node.set_multiselectable();
        }
        let cursor = table.shared.borrow().selection.cursor;
        if let Some(pod) = cursor.and_then(|c| self.rows.get(&c)) {
            node.set_active_descendant(NodeId(pod.id().to_raw()));
        }
    }
}

/// The contexts a table's body can ask for a mutation after the event from.
pub(super) trait Later {
    fn later(&mut self, f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static);
}

impl Later for EventCtx<'_> {
    fn later(&mut self, f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static) {
        self.mutate_self_later(f);
    }
}

impl Later for UpdateCtx<'_> {
    fn later(&mut self, f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static) {
        self.mutate_self_later(f);
    }
}

// --- MARK: HEADER

/// A column's title in the header: it draws the title and is the column header for
/// assistive technology; the header takes the pointer.
#[derive(Debug)]
pub(crate) struct ColumnHeaderWidget {
    index: usize,
    title: TextLine,
    align: Align,
    sortable: bool,
    sorted: Option<SortOrder>,
}

impl ColumnHeaderWidget {
    fn set_sorted(this: &mut WidgetMut<'_, Self>, sorted: Option<SortOrder>) {
        if this.widget.sorted != sorted {
            this.widget.sorted = sorted;
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }
}

impl UsesProperty<ThemeProp> for ColumnHeaderWidget {}

impl Widget for ColumnHeaderWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) => ctx.request_paint_only(),
            Update::FontsChanged => {
                self.title.invalidate();
                ctx.request_layout();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_paint_only();
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            LenReq::MinContent | LenReq::MaxContent => Length::ZERO,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // The title stops short of the divider, and of the sort indicator.
        let arrow = if self.sorted.is_some() {
            SORT_ARROW_WIDTH + SORT_ARROW_INSET
        } else {
            0.0
        };
        let clip = Rect::new(0.0, 0.0, (size.width - arrow - 1.0).max(0.0), size.height);
        ctx.set_clip_path(clip);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let p = Palette::new(theme.variant);
        let color = if ctx.is_disabled() {
            p.disabled_title
        } else {
            p.title
        };
        let size = ctx.content_box().size();
        let text = self.title.size(ctx.text_contexts());
        let x = aligned(self.align, (0.0, size.width), text.width, TITLE_INSET);
        // Centred above the bottom border.
        let y = ((size.height - 1.0 - text.height) / 2.0).round();
        self.title
            .paint(painter, Affine::translate(Vec2::new(x, y)), color);
    }

    fn accessibility_role(&self) -> Role {
        Role::ColumnHeader
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.title.text());
        node.set_column_index(self.index);
        match self.sorted {
            Some(SortOrder::Ascending) => node.set_sort_direction(SortDirection::Ascending),
            Some(SortOrder::Descending) => node.set_sort_direction(SortDirection::Descending),
            None => {}
        }
        if self.sortable {
            node.add_action(Action::Click);
        }
    }

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.title.text().to_owned())
    }
}

/// Resizing a column: which one, and where the drag started.
#[derive(Debug, Clone, Copy)]
struct Resize {
    column: usize,
    start_x: f64,
    start_width: f64,
}

/// The header bar: the material, sorting by a click on a title, resizing by dragging a
/// divider.
#[derive(Debug)]
pub(crate) struct HeaderWidget {
    columns: Vec<Column>,
    titles: Vec<WidgetPod<ColumnHeaderWidget>>,
    /// Widths as set; the last is stretched to the header's width.
    widths: Vec<f64>,
    /// Widths as drawn, from the last layout.
    drawn: Vec<f64>,
    sorted: Option<(usize, SortOrder)>,
    pressed: Option<usize>,
    resize: Option<Resize>,
    /// The table's body, told about new widths.
    body: WidgetId,
    replayer: ImagingReplayer,
}

impl HeaderWidget {
    fn new(columns: Vec<Column>, body: WidgetId, sorted: Option<(usize, SortOrder)>) -> Self {
        let sorted = sorted.filter(|&(c, _)| c < columns.len());
        let titles = columns
            .iter()
            .enumerate()
            .map(|(index, c)| {
                NewWidget::new(ColumnHeaderWidget {
                    index,
                    title: TextLine::new(c.title.clone(), TextStyle::Small),
                    align: c.align,
                    sortable: c.sortable,
                    sorted: sorted.filter(|&(s, _)| s == index).map(|(_, o)| o),
                })
                .to_pod()
            })
            .collect();
        let widths = columns.iter().map(|c| c.clamp(c.width)).collect();
        Self {
            columns,
            titles,
            widths,
            drawn: Vec::new(),
            sorted,
            pressed: None,
            resize: None,
            body,
            replayer: ImagingReplayer::new(),
        }
    }

    /// The divider under `x`, if a press there would grab it (the right edge of a column;
    /// the last column's only while there's room past it).
    fn divider_at(&self, x: f64) -> Option<usize> {
        let edges = column_edges(Rect::new(0.0, 0.0, 0.0, 0.0), &self.drawn);
        edges
            .iter()
            .enumerate()
            .rev()
            .find(|&(_, &(_, x1))| (x - x1).abs() <= DIVIDER_GRAB)
            .map(|(i, _)| i)
    }

    /// The column under `x`.
    fn column_at(&self, x: f64) -> Option<usize> {
        column_edges(Rect::new(0.0, 0.0, 0.0, 0.0), &self.drawn)
            .iter()
            .position(|&(x0, x1)| x >= x0 && x < x1)
    }

    /// Sorts by `column` (the other way if it's sorted already) and tells the app.
    fn sort_by(&mut self, ctx: &mut EventCtx<'_>, column: usize) {
        if !self.columns.get(column).is_some_and(|c| c.sortable) {
            return;
        }
        let order = match self.sorted {
            Some((c, order)) if c == column => order.reversed(),
            _ => SortOrder::Ascending,
        };
        self.sorted = Some((column, order));
        ctx.submit_action::<Sorted>(Sorted(column, order));
        ctx.request_paint_only();
        ctx.mutate_self_later(|mut w| Self::sync_titles(&mut w.downcast()));
    }

    /// Column widths as set, for tests.
    #[cfg(test)]
    pub(crate) fn widths(&self) -> &[f64] {
        &self.widths
    }

    /// Tells the titles which one is sorted.
    fn sync_titles(this: &mut WidgetMut<'_, Self>) {
        let sorted = this.widget.sorted;
        for (i, title) in this.widget.titles.iter_mut().enumerate() {
            let order = sorted.filter(|&(c, _)| c == i).map(|(_, o)| o);
            ColumnHeaderWidget::set_sorted(&mut this.ctx.get_mut(title), order);
        }
    }

    pub(super) fn set_sorted(this: &mut WidgetMut<'_, Self>, sorted: Option<(usize, SortOrder)>) {
        this.widget.sorted = sorted.filter(|&(c, _)| c < this.widget.columns.len());
        Self::sync_titles(this);
        this.ctx.request_paint_only();
    }
}

impl UsesProperty<ThemeProp> for HeaderWidget {}

impl Widget for HeaderWidget {
    type Action = Sorted;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let x = ctx.local_position(state.position).x;
                if let Some(column) = self.divider_at(x) {
                    self.resize = Some(Resize {
                        column,
                        start_x: x,
                        start_width: self.widths[column],
                    });
                } else {
                    self.pressed = self.column_at(x).filter(|&c| self.columns[c].sortable);
                }
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let x = ctx.local_position(current.position).x;
                let Some(resize) = self.resize.filter(|_| ctx.is_active()) else {
                    return;
                };
                let column = &self.columns[resize.column];
                let width = column.clamp(resize.start_width + x - resize.start_x);
                if width != self.widths[resize.column] {
                    self.widths[resize.column] = width;
                    ctx.request_layout();
                    let widths = self.widths.clone();
                    ctx.mutate_later(self.body, move |mut body| {
                        RowsWidget::set_widths(&mut body.downcast(), &widths);
                    });
                }
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let x = ctx.local_position(state.position).x;
                if self.resize.take().is_none()
                    && let Some(column) = self.pressed.take()
                    && ctx.is_hovered()
                    && self.column_at(x) == Some(column)
                {
                    self.sort_by(ctx, column);
                }
                self.pressed = None;
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) => {
                self.pressed = None;
                self.resize = None;
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action != Action::Click {
            return;
        }
        let target = ctx.target();
        if let Some(column) = self.titles.iter().position(|t| t.id() == target) {
            self.sort_by(ctx, column);
            ctx.set_handled();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for title in &mut self.titles {
            ctx.register_child(title);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::DisabledChanged(_)) {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
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
        match (axis, len_req) {
            (_, LenReq::FitContent(space)) => space,
            (Axis::Vertical, _) => Length::px(HEADER_HEIGHT),
            (Axis::Horizontal, _) => Length::px(self.widths.iter().sum()),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.drawn = stretched(&self.widths, size.width);
        let edges = column_edges(size.to_rect(), &self.drawn);
        for (title, &(x0, x1)) in self.titles.iter_mut().zip(&edges) {
            ctx.run_layout(title, Size::new(x1 - x0, size.height));
            ctx.place_child(title, Point::new(x0, 0.0));
        }
        ctx.clear_baselines();
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        let pressed = self.pressed.filter(|_| ctx.is_active() && ctx.is_hovered());
        let header = TableHeader {
            widths: &self.drawn,
            pressed,
            sorted: self
                .sorted
                .map(|(c, order)| (c, order == SortOrder::Ascending)),
        };
        table_header(&mut scene, size.to_rect(), header, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn get_cursor(&self, _ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        if self.resize.is_some() || self.divider_at(pos.x).is_some() {
            CursorIcon::ColResize
        } else {
            CursorIcon::Default
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Row
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        self.titles.iter().map(WidgetPod::id).collect()
    }
}

// --- MARK: BUILDER

type OnSelect<M> = Box<dyn FnMut(&[usize]) -> Option<M>>;
type OnActivate<M> = Box<dyn FnMut(usize) -> Option<M>>;
type OnSort<M> = Box<dyn FnMut(usize, SortOrder) -> M>;

/// Rows of cells under a header of column titles, in a bordered view that scrolls both
/// ways. Only the rows on screen exist (like [`crate::VirtualList`]), so it stays fast
/// with many rows.
///
/// `cell(row, column)` gives each cell as its row scrolls into view; like a virtual
/// list's rows it can't see the model, so it reads shared data, and after the data
/// changes [`Handle::reload`] builds the visible rows again.
///
/// A click selects a row; with [`multiple`](Self::multiple), Shift-click extends the
/// selection and Cmd-click (Ctrl-click off macOS) adds or removes a row. With keyboard
/// focus (Tab), Up and Down move the selection (Shift extends it), Home, End, Page Up and
/// Page Down jump, typing selects the first row whose first column starts with what was
/// typed, and Return or a double-click activates the selected row. A click on a sortable
/// title sorts by it (the app sorts); dragging a divider resizes a column. The last
/// column stretches to fill the view.
pub struct Table<M> {
    inner: NewWidget<ScrollWidget>,
    pub(super) shared: Shared,
    count: usize,
    body: WidgetId,
    /// The header is built last, with the initial sort.
    columns: Vec<Column>,
    sorted: Option<(usize, SortOrder)>,
    on_select: Option<OnSelect<M>>,
    on_activate: Option<OnActivate<M>>,
    on_sort: Option<OnSort<M>>,
}

impl<M> std::fmt::Debug for Table<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M: 'static> Table<M> {
    /// `count` rows of `columns`, their cells from `cell(row, column)`.
    pub fn new<C: Into<Cell<M>>>(
        columns: impl IntoIterator<Item = Column>,
        count: usize,
        cell: impl FnMut(usize, usize) -> C + 'static,
    ) -> Self {
        let columns: Vec<Column> = columns.into_iter().collect();
        let widths = columns.iter().map(|c| c.clamp(c.width)).collect();
        let shared: Shared = Rc::new(RefCell::new(TableState {
            widths,
            ..TableState::default()
        }));
        let cell = Rc::new(RefCell::new(cell));
        let aligns: Vec<Align> = columns.iter().map(|c| c.align).collect();

        let make: MakeRow = {
            let shared = Rc::clone(&shared);
            let cell = Rc::clone(&cell);
            Box::new(move |row| {
                let mut wiring = Wiring::<M>::default();
                let state = shared.borrow();
                let look = state.look(row);
                let outline = state.outline_row(row);
                let cells = aligns
                    .iter()
                    .enumerate()
                    .map(|(column, &align)| {
                        let content = match (cell.borrow_mut())(row, column).into().0 {
                            CellKind::Text(text) => {
                                CellContent::Text(Box::new(TextLine::new(text, TextStyle::Body)))
                            }
                            CellKind::Element(e) => CellContent::Element(wiring.adopt(*e).to_pod()),
                        };
                        NewWidget::new(CellWidget {
                            column,
                            align,
                            content,
                            look,
                            outline: outline.filter(|_| column == 0),
                            replayer: ImagingReplayer::new(),
                        })
                        .to_pod()
                    })
                    .collect();
                let widget = TableRowWidget {
                    index: row,
                    cells,
                    widths: state.widths.clone(),
                    look,
                    outline,
                };
                (
                    NewWidget::new(widget).erased(),
                    Box::new(wiring) as Box<dyn Any>,
                )
            })
        };
        let text_of = Box::new(move |row| (cell.borrow_mut())(row, 0).into().into_text());
        let body = TableBody {
            shared: Rc::clone(&shared),
            columns: columns.len(),
            text_of,
            typed: TypeSelect::default(),
            dragging: false,
            replayer: ImagingReplayer::new(),
        };
        let rows = NewWidget::new(RowsWidget::new(count, ROW_HEIGHT, make).with_table(body));
        let body = rows.id();
        let mut scroll = ScrollWidget::new(rows.erased(), ScrollAxes::Both, true);
        scroll.frame = Frame::Border;
        Self {
            inner: NewWidget::new(scroll),
            shared,
            count,
            body,
            columns,
            sorted: None,
            on_select: None,
            on_activate: None,
            on_sort: None,
        }
    }

    /// Lets the user select more than one row.
    #[must_use]
    pub fn multiple(self, multiple: bool) -> Self {
        self.shared.borrow_mut().multiple = multiple;
        self
    }

    /// Stripes every other row.
    #[must_use]
    pub fn alternating_rows(self, alternating: bool) -> Self {
        self.shared.borrow_mut().alternating = alternating;
        self
    }

    /// Starts with `rows` selected.
    #[must_use]
    pub fn selected(self, rows: impl IntoIterator<Item = usize>) -> Self {
        self.shared.borrow_mut().selection.set(rows, self.count);
        self
    }

    /// Starts with the sort indicator on `column`.
    #[must_use]
    pub fn sorted(mut self, column: usize, order: SortOrder) -> Self {
        self.sorted = Some((column, order));
        self
    }

    /// Sends the message `make` returns, given the selected rows (in order), when the
    /// user changes the selection. Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_select(self, mut make: impl FnMut(&[usize]) -> M + 'static) -> Self {
        self.on_select_opt(move |rows| Some(make(rows)))
    }

    /// Like [`on_select`](Self::on_select), where `make` can decline to send anything.
    pub(super) fn on_select_opt(
        mut self,
        make: impl FnMut(&[usize]) -> Option<M> + 'static,
    ) -> Self {
        self.on_select = Some(Box::new(make));
        self
    }

    /// Sends the message `make` returns, given the row, when the user double-clicks a row
    /// or presses Return. Without it, Return goes on to the default button.
    #[must_use]
    pub fn on_activate(self, mut make: impl FnMut(usize) -> M + 'static) -> Self {
        self.on_activate_opt(move |row| Some(make(row)))
    }

    /// Like [`on_activate`](Self::on_activate), where `make` can decline to send anything.
    pub(super) fn on_activate_opt(
        mut self,
        make: impl FnMut(usize) -> Option<M> + 'static,
    ) -> Self {
        self.shared.borrow_mut().activates = true;
        self.on_activate = Some(Box::new(make));
        self
    }

    /// The scroll view's widget, which a handle points at.
    pub(super) fn scroll_id(&self) -> WidgetId {
        self.inner.id()
    }

    /// Sends the message `make` returns, given the column and order, when the user clicks
    /// a sortable column's title. The table shows the new order; the app sorts its data
    /// and calls [`Handle::reload`].
    #[must_use]
    pub fn on_sort(mut self, make: impl FnMut(usize, SortOrder) -> M + 'static) -> Self {
        self.on_sort = Some(Box::new(make));
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

    /// Points `handle` at this table.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<Table<M>> for Element<M> {
    fn from(mut table: Table<M>) -> Self {
        let header = NewWidget::new(HeaderWidget::new(table.columns, table.body, table.sorted));
        let header_id = header.id();
        table
            .inner
            .widget
            .set_header(header.erased(), HEADER_HEIGHT);
        let mut element = Element::new(table.inner.erased());
        let (mut on_select, mut on_activate) = (table.on_select, table.on_activate);
        if on_select.is_some() || on_activate.is_some() {
            element.on_action(table.body, move |action| {
                match action.downcast_ref::<TableAction>()? {
                    TableAction::Selected(rows) => on_select.as_mut().and_then(|f| f(rows)),
                    TableAction::Activated(row) => on_activate.as_mut().and_then(|f| f(*row)),
                }
            });
        }
        if let Some(mut on_sort) = table.on_sort {
            element.on_action(header_id, move |action| {
                let Sorted(column, order) = *action.downcast_ref::<Sorted>()?;
                Some(on_sort(column, order))
            });
        }
        element
    }
}

impl<M> Handle<Table<M>> {
    /// Changes the number of rows; selected rows past the end are deselected.
    pub fn set_count<N>(&self, cx: &mut Cx<'_, N>, count: usize) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| {
                RowsWidget::edit_count(rows, count);
                RowsWidget::restyle(rows);
            });
        });
    }

    /// Builds the visible rows again, after the data they show changed.
    pub fn reload<N>(&self, cx: &mut Cx<'_, N>) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, RowsWidget::edit_reload);
        });
    }

    /// Selects `rows` (those in range). Sends no message.
    pub fn set_selection<N>(&self, cx: &mut Cx<'_, N>, rows: impl IntoIterator<Item = usize>) {
        let rows: Vec<usize> = rows.into_iter().collect();
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |body| RowsWidget::set_selection(body, rows));
        });
    }

    /// Puts the sort indicator on `column` (`None`: on no column). Sends no message.
    pub fn set_sorted<N>(&self, cx: &mut Cx<'_, N>, sorted: Option<(usize, SortOrder)>) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_header(&mut w, |mut header| {
                HeaderWidget::set_sorted(&mut header.downcast(), sorted);
            });
        });
    }

    /// Scrolls as little as it takes to show row `index` whole.
    pub fn scroll_to_row<N>(&self, cx: &mut Cx<'_, N>, index: usize) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            let span = ScrollWidget::with_rows(&mut w, |rows| {
                let (y0, y1) = rows.widget.row_span(index);
                let x = rows.widget.window.x0;
                Rect::new(x, y0, x, y1)
            });
            if let Some(target) = span {
                ScrollWidget::edit_reveal(&mut w, target);
            }
        });
    }

    /// Enables or disables the table.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ScrollWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(s: &Selection) -> Vec<usize> {
        s.rows.iter().copied().collect()
    }

    #[test]
    fn shift_extends_from_the_anchor_and_command_toggles() {
        let mut s = Selection::default();
        s.only(3);
        s.extend_to(6);
        assert_eq!(rows(&s), [3, 4, 5, 6]);
        s.extend_to(1);
        assert_eq!(rows(&s), [1, 2, 3], "from the same anchor, the other way");
        s.toggle(9);
        assert_eq!(rows(&s), [1, 2, 3, 9]);
        s.toggle(2);
        assert_eq!(rows(&s), [1, 3, 9]);
        s.extend_to(11);
        assert_eq!(
            rows(&s),
            [2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            "anchor moved to 2"
        );
    }

    #[test]
    fn rows_past_the_end_are_forgotten() {
        let mut s = Selection::default();
        s.set([1, 4, 8], 10);
        s.forget_past(5);
        assert_eq!(rows(&s), [1, 4]);
        assert_eq!(s.cursor, None);
        s.set([2, 20], 10);
        assert_eq!(rows(&s), [2], "out of range from the app");
    }

    #[test]
    fn the_last_column_stretches_to_fill() {
        assert_eq!(stretched(&[50.0, 30.0], 120.0), [50.0, 70.0]);
        assert_eq!(
            stretched(&[50.0, 30.0], 60.0),
            [50.0, 30.0],
            "never shrinks"
        );
        assert!(stretched(&[], 60.0).is_empty());
    }

    #[test]
    fn columns_keep_their_limits() {
        let c = Column::new("Name").min_width(40.0).max_width(200.0);
        assert_eq!(c.clamp(10.0), 40.0);
        assert_eq!(c.clamp(500.0), 200.0);
        assert_eq!(
            Column::new("x").min_width(50.0).max_width(20.0).clamp(30.0),
            50.0
        );
    }
}
