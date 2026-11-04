//! Virtual rows: the content of a [`crate::VirtualList`]. Only the rows on screen, and
//! half a screen either side, exist as widgets; the scroll view tells the rows which part
//! of them shows, and they build and drop rows to match (ADR 0043). A table's body is the
//! same widget with a [`TableBody`]: selection, keys and row backgrounds (ADR 0048). A
//! [`crate::VirtualGrid`] is the same widget with several items to a row.

use std::any::Any;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::ops::Range;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
    UsesProperty, Widget, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, Size};
use masonry_core::layout::{LenReq, Length};

use super::scroll::{ScrollAxes, ScrollWidget};
use super::table::TableBody;
use crate::Cx;
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// The rows built a row widget or dropped one: the app takes the new rows' message
/// handlers ([`RowsWidget::take_wiring`]) and forgets the dropped rows' ones.
#[derive(Debug)]
pub(crate) struct Rewired;

/// Builds row `i`: its widget, and its wiring (an `Element`'s `Wiring<M>`, type-erased
/// because widgets aren't generic over the app's message type).
pub(crate) type MakeRow = Box<dyn FnMut(usize) -> (NewWidget<dyn Widget>, Box<dyn Any>)>;

/// The Masonry widget holding a virtual list's rows.
pub(crate) struct RowsWidget {
    pub(super) count: usize,
    /// The height of a row (of items, in a grid).
    pub(super) row_height: f64,
    /// A grid's cell width; `None` for a list, whose rows fill the width.
    cell_width: Option<f64>,
    /// Items to a row: one in a list, as many as fit the width in a grid.
    pub(crate) columns: usize,
    make: MakeRow,
    pub(super) rows: BTreeMap<usize, WidgetPod<dyn Widget>>,
    /// The part on screen, in logical pixels.
    pub(super) window: Rect,
    /// A table's body: selection, keys and row backgrounds.
    pub(crate) table: Option<Box<TableBody>>,
    /// Wiring of rows built since the app last took it.
    pending: Vec<Box<dyn Any>>,
    /// Rows were dropped since the app last took the wiring.
    dropped: bool,
}

impl std::fmt::Debug for RowsWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowsWidget")
            .field("count", &self.count)
            .field("row_height", &self.row_height)
            .field("columns", &self.columns)
            .field("rows", &self.rows.keys().collect::<Vec<_>>())
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

impl RowsWidget {
    pub(crate) fn new(count: usize, row_height: f64, make: MakeRow) -> Self {
        Self {
            count,
            row_height: if row_height.is_finite() && row_height > 0.0 {
                row_height
            } else {
                1.0
            },
            cell_width: None,
            columns: 1,
            make,
            rows: BTreeMap::new(),
            window: Rect::ZERO,
            table: None,
            pending: Vec::new(),
            dropped: false,
        }
    }

    /// Lays the items out in a grid of cells `cell_width` wide.
    pub(super) fn with_cell_width(mut self, cell_width: f64) -> Self {
        self.cell_width = Some(if cell_width.is_finite() && cell_width > 0.0 {
            cell_width
        } else {
            1.0
        });
        self
    }

    /// Items to a row in a view `width` wide.
    fn columns_for(&self, width: f64) -> usize {
        let Some(cell) = self.cell_width else {
            return 1;
        };
        // `as` saturates: a negative or NaN width is 0 columns, then at least one.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "saturating, then at least one"
        )]
        let columns = (width / cell).floor() as usize;
        columns.max(1)
    }

    /// The body of a table.
    pub(super) fn with_table(mut self, table: TableBody) -> Self {
        self.table = Some(Box::new(table));
        self
    }

    /// The items to have built: those in the window, and half a window either side, whole
    /// rows of them.
    fn wanted(&self) -> Range<usize> {
        let (top, bottom) = (self.window.y0, self.window.y1);
        let overscan = (bottom - top) / 2.0;
        if bottom <= top {
            return 0..0;
        }
        // Row index at `y` rows down, kept inside `0..=count`.
        let index = |rows: f64| {
            // `as` saturates: negative rows become 0, then `min` caps at the count.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "saturating, then capped"
            )]
            let i = rows as usize;
            i.min(self.count)
        };
        let first = index(((top - overscan) / self.row_height).floor());
        let last = index(((bottom + overscan) / self.row_height).ceil());
        let items = |rows: usize| rows.saturating_mul(self.columns).min(self.count);
        items(first)..items(last)
    }

    /// The scroll view moved or resized: build and drop rows to match. A grid's columns
    /// follow the width.
    pub(crate) fn set_window(this: &mut WidgetMut<'_, Self>, window: Rect) {
        this.widget.window = window;
        let columns = this.widget.columns_for(window.width());
        if columns != this.widget.columns {
            this.widget.columns = columns;
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
        Self::sync(this);
    }

    /// Drops rows outside [`Self::wanted`] and builds the missing ones.
    fn sync(this: &mut WidgetMut<'_, Self>) {
        let wanted = this.widget.wanted();
        let gone: Vec<usize> = this
            .widget
            .rows
            .keys()
            .copied()
            .filter(|i| !wanted.contains(i))
            .collect();
        let mut changed = !gone.is_empty();
        for i in gone {
            if let Some(row) = this.widget.rows.remove(&i) {
                this.ctx.remove_child(row);
            }
        }
        this.widget.dropped |= changed;
        for i in wanted {
            if this.widget.rows.contains_key(&i) {
                continue;
            }
            let (widget, wiring) = (this.widget.make)(i);
            this.widget.rows.insert(i, widget.to_pod());
            this.widget.pending.push(wiring);
            changed = true;
        }
        if changed {
            this.ctx.children_changed();
            this.ctx.submit_action::<Rewired>(Rewired);
        }
    }

    /// Changes the number of rows; rows past the end go.
    pub(crate) fn edit_count(this: &mut WidgetMut<'_, Self>, count: usize) {
        Self::set_count(this, count);
        Self::sync(this);
    }

    /// Sets the number of rows without building rows for it.
    fn set_count(this: &mut WidgetMut<'_, Self>, count: usize) {
        this.widget.count = count;
        if let Some(table) = &this.widget.table {
            table.forget_past(count);
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
        this.ctx.request_layout();
    }

    /// Changes the number of rows and builds the visible ones again (an outline's rows
    /// changed). One edit: rows built by a count change aren't in the tree yet to drop.
    pub(crate) fn edit_count_and_reload(this: &mut WidgetMut<'_, Self>, count: usize) {
        Self::drop_rows(this);
        Self::set_count(this, count);
        Self::sync(this);
    }

    fn drop_rows(this: &mut WidgetMut<'_, Self>) {
        let rows = std::mem::take(&mut this.widget.rows);
        this.widget.dropped |= !rows.is_empty();
        for row in rows.into_values() {
            this.ctx.remove_child(row);
        }
    }

    /// Drops every row and builds the visible ones again (the data changed).
    pub(crate) fn edit_reload(this: &mut WidgetMut<'_, Self>) {
        Self::drop_rows(this);
        Self::sync(this);
    }

    /// The wiring of rows built since the last call, and whether rows were dropped.
    pub(crate) fn take_wiring(this: &mut WidgetMut<'_, Self>) -> (Vec<Box<dyn Any>>, bool) {
        let dropped = std::mem::take(&mut this.widget.dropped);
        (std::mem::take(&mut this.widget.pending), dropped)
    }

    /// Where item `i`'s row sits, from the top.
    pub(crate) fn row_span(&self, i: usize) -> (f64, f64) {
        #[expect(clippy::cast_precision_loss, reason = "row counts far below 2^52")]
        let y = (i / self.columns) as f64 * self.row_height;
        (y, y + self.row_height)
    }

    /// The indices of the rows that exist as widgets, for tests.
    #[cfg(test)]
    pub(crate) fn built(&self) -> Vec<usize> {
        self.rows.keys().copied().collect()
    }

    /// Row `i`'s widget, if it's built.
    #[cfg(test)]
    pub(crate) fn row_id(&self, i: usize) -> Option<masonry_core::core::WidgetId> {
        self.rows.get(&i).map(WidgetPod::id)
    }
}

impl UsesProperty<ThemeProp> for RowsWidget {}

impl Widget for RowsWidget {
    /// A table's body also sends [`super::table::TableAction`]s, untyped.
    type Action = Rewired;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.table.is_some() {
            self.table_pointer(ctx, event);
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if self.table.is_some() {
            self.table_text(ctx, event);
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if self.table.is_some() {
            self.table_access(ctx, event);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // An outline's rows can have changed (initial expansion) since the builder set
        // the count.
        if matches!(event, Update::WidgetAdded)
            && let Some(len) = self.table.as_ref().and_then(|t| t.outline_len())
        {
            self.count = len;
            ctx.request_layout();
        }
        if self.table.is_some()
            && matches!(event, Update::FocusChanged(_) | Update::DisabledChanged(_))
        {
            Self::restyle_later(ctx);
        }
        // The pointer left the body: a source list's group header isn't hovered any more.
        if self.table.is_some()
            && matches!(
                event,
                Update::HoveredChanged(false) | Update::ChildHoveredChanged(false)
            )
        {
            self.clear_hover(ctx);
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ThemeProp>() {
            ctx.request_paint_only();
        }
    }

    fn accepts_focus(&self) -> bool {
        self.table.is_some()
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for row in self.rows.values_mut() {
            ctx.register_child(row);
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        // A table is at least as wide as its columns.
        let columns = self.table.as_ref().map_or(0.0, |t| t.columns_width());
        match axis {
            Axis::Vertical => {
                // A grid's rows depend on the width it's given.
                let per_row = cross_length.map_or(self.columns, |w| self.columns_for(w.get()));
                #[expect(clippy::cast_precision_loss, reason = "row counts far below 2^52")]
                let rows = self.count.div_ceil(per_row) as f64;
                Length::px(rows * self.row_height)
            }
            // As wide as the view: rows are laid out to its width.
            Axis::Horizontal => match len_req {
                LenReq::FitContent(space) => Length::px(space.get().max(columns)),
                LenReq::MinContent | LenReq::MaxContent => {
                    Length::px(self.cell_width.unwrap_or(columns))
                }
            },
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let height = self.row_height;
        let columns = self.columns_for(size.width);
        // A grid spreads the slack of a row evenly, and centres each item in its cell.
        #[expect(clippy::cast_precision_loss, reason = "column counts far below 2^52")]
        let cell = size.width / columns as f64;
        let item = self.cell_width.map_or(size.width, |w| w.min(cell));
        for (&i, row) in &mut self.rows {
            #[expect(clippy::cast_precision_loss, reason = "row counts far below 2^52")]
            let (x, y) = (
                (i % columns) as f64 * cell + (cell - item) / 2.0,
                (i / columns) as f64 * height,
            );
            ctx.run_layout(row, Size::new(item, height));
            ctx.place_child(row, Point::new(x, y));
        }
        ctx.clear_baselines();
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if self.table.is_some() {
            self.table_paint(ctx, props, painter);
        }
    }

    fn accessibility_role(&self) -> Role {
        if self.table.is_some() {
            // `accessibility_role` takes `&self`; a table's role is read through its state.
            self.table.as_ref().map_or(Role::List, |t| t.role())
        } else {
            Role::List
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if self.table.is_some() {
            self.table_accessibility(node);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        self.rows.values().map(WidgetPod::id).collect()
    }
}

/// A scrolling list of `count` rows of one height, built only while they're on screen
/// (and half a screen either side), so it stays fast with millions of rows. The base of
/// tables and collection views.
///
/// `row` builds row `i` each time it scrolls into view; rows that scroll far out of view
/// are dropped, with their state. It can't see the app's model, so it reads the data
/// from something it shares with the app (an `Rc<RefCell<…>>`, say); after the data
/// changes, [`Handle::reload`] builds the visible rows again.
///
/// It scrolls like a [`crate::ScrollView`] (vertically) and is as wide as its container
/// lets it be; give it a [`height`](Self::height) in a [`crate::Stack`].
pub struct VirtualList<M> {
    inner: NewWidget<ScrollWidget>,
    kind: PhantomData<fn() -> M>,
}

impl<M> std::fmt::Debug for VirtualList<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualList")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<M: 'static> VirtualList<M> {
    /// `count` rows, `row_height` logical pixels high, built by `row`.
    pub fn new<E: Into<Element<M>>>(
        count: usize,
        row_height: f64,
        mut row: impl FnMut(usize) -> E + 'static,
    ) -> Self {
        let make: MakeRow = Box::new(move |i| {
            let (widget, wiring) = row(i).into().into_parts();
            (widget, Box::new(wiring) as Box<dyn Any>)
        });
        let rows = NewWidget::new(RowsWidget::new(count, row_height, make)).erased();
        Self {
            inner: NewWidget::new(ScrollWidget::new(rows, ScrollAxes::Vertical, true)),
            kind: PhantomData,
        }
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

    /// Points `handle` at this list.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<VirtualList<M>> for Element<M> {
    fn from(list: VirtualList<M>) -> Self {
        Element::new(list.inner.erased())
    }
}

impl<M> Handle<VirtualList<M>> {
    /// Changes the number of rows.
    pub fn set_count<N>(&self, cx: &mut Cx<'_, N>, count: usize) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| RowsWidget::edit_count(rows, count));
        });
    }

    /// Builds the visible rows again, after the data they show changed.
    pub fn reload<N>(&self, cx: &mut Cx<'_, N>) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, RowsWidget::edit_reload);
        });
    }

    /// Scrolls as little as it takes to show row `index` whole.
    pub fn scroll_to_row<N>(&self, cx: &mut Cx<'_, N>, index: usize) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            let span = ScrollWidget::with_rows(&mut w, |rows| rows.widget.row_span(index));
            if let Some((y0, y1)) = span {
                ScrollWidget::edit_reveal(&mut w, Rect::new(0.0, y0, 0.0, y1));
            }
        });
    }
}

/// A scrolling grid of `count` items in cells of one size, built only while they're on
/// screen (and half a screen either side). The base of collection views.
///
/// The grid has as many columns as fit its width (at least one); the slack of a row is
/// spread between the cells, and each item is centred in its cell, so items carry their
/// own spacing. It builds items, drops them and rewires their handlers like a
/// [`VirtualList`] (ADR 0043, addendum), and scrolls vertically.
pub struct VirtualGrid<M> {
    inner: NewWidget<ScrollWidget>,
    kind: PhantomData<fn() -> M>,
}

impl<M> std::fmt::Debug for VirtualGrid<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualGrid")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<M: 'static> VirtualGrid<M> {
    /// `count` items in cells `cell_width` by `cell_height` logical pixels, built by
    /// `item`.
    pub fn new<E: Into<Element<M>>>(
        count: usize,
        cell_width: f64,
        cell_height: f64,
        mut item: impl FnMut(usize) -> E + 'static,
    ) -> Self {
        let make: MakeRow = Box::new(move |i| {
            let (widget, wiring) = item(i).into().into_parts();
            (widget, Box::new(wiring) as Box<dyn Any>)
        });
        let rows =
            NewWidget::new(RowsWidget::new(count, cell_height, make).with_cell_width(cell_width))
                .erased();
        Self {
            inner: NewWidget::new(ScrollWidget::new(rows, ScrollAxes::Vertical, true)),
            kind: PhantomData,
        }
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

    /// Points `handle` at this grid.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<VirtualGrid<M>> for Element<M> {
    fn from(grid: VirtualGrid<M>) -> Self {
        Element::new(grid.inner.erased())
    }
}

impl<M> Handle<VirtualGrid<M>> {
    /// Changes the number of items.
    pub fn set_count<N>(&self, cx: &mut Cx<'_, N>, count: usize) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, |rows| RowsWidget::edit_count(rows, count));
        });
    }

    /// Builds the visible items again, after the data they show changed.
    pub fn reload<N>(&self, cx: &mut Cx<'_, N>) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::with_rows(&mut w, RowsWidget::edit_reload);
        });
    }

    /// Scrolls as little as it takes to show item `index`'s row whole.
    pub fn scroll_to_item<N>(&self, cx: &mut Cx<'_, N>, index: usize) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            let span = ScrollWidget::with_rows(&mut w, |rows| rows.widget.row_span(index));
            if let Some((y0, y1)) = span {
                ScrollWidget::edit_reveal(&mut w, Rect::new(0.0, y0, 0.0, y1));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::LabelWidget;

    fn rows(count: usize, window: (f64, f64)) -> RowsWidget {
        let make: MakeRow = Box::new(|i| {
            (
                NewWidget::new(LabelWidget::new(i.to_string())).erased(),
                Box::new(()),
            )
        });
        let mut rows = RowsWidget::new(count, 20.0, make);
        rows.window = Rect::new(0.0, window.0, 100.0, window.1);
        rows
    }

    #[test]
    fn wanted_covers_the_window_and_half_a_window_either_side() {
        // 100 px window at 200: rows 10..15 show; 50 px either side adds 2.5 rows.
        assert_eq!(rows(1000, (200.0, 300.0)).wanted(), 7..18);
        assert_eq!(
            rows(1000, (0.0, 100.0)).wanted(),
            0..8,
            "clamped at the top"
        );
        assert_eq!(rows(12, (200.0, 300.0)).wanted(), 7..12, "and at the end");
        assert_eq!(rows(1000, (0.0, 0.0)).wanted(), 0..0, "not laid out yet");
        assert_eq!(rows(0, (0.0, 100.0)).wanted(), 0..0);
    }

    /// A grid of 40-wide, 20-high cells.
    fn grid(count: usize, window: Rect) -> RowsWidget {
        let mut grid = rows(count, (0.0, 0.0)).with_cell_width(40.0);
        grid.window = window;
        grid.columns = grid.columns_for(window.width());
        grid
    }

    #[test]
    fn columns_follow_the_width_and_are_at_least_one() {
        let grid = rows(10, (0.0, 0.0)).with_cell_width(40.0);
        assert_eq!(grid.columns_for(200.0), 5);
        assert_eq!(grid.columns_for(199.9), 4);
        assert_eq!(grid.columns_for(10.0), 1, "narrower than a cell");
        assert_eq!(grid.columns_for(0.0), 1);
        assert_eq!(rows(10, (0.0, 0.0)).columns_for(500.0), 1, "a list");
    }

    #[test]
    fn a_grid_wants_whole_rows_of_items() {
        // 4 columns; the 100 px window at 200 shows rows 10..15, 50 px either side adds
        // 2.5 rows: rows 7..18, items 28..72.
        let window = Rect::new(0.0, 200.0, 160.0, 300.0);
        assert_eq!(grid(1000, window).wanted(), 28..72);
        let top = Rect::new(0.0, 0.0, 160.0, 100.0);
        assert_eq!(grid(1000, top).wanted(), 0..32, "clamped at the top");
        assert_eq!(grid(30, window).wanted(), 28..30, "a short last row");
        assert_eq!(grid(0, top).wanted(), 0..0);
    }

    #[test]
    fn a_grids_rows_hold_a_row_of_items() {
        let window = Rect::new(0.0, 0.0, 160.0, 100.0);
        let grid = grid(1000, window);
        assert_eq!(grid.row_span(0), (0.0, 20.0));
        assert_eq!(grid.row_span(3), (0.0, 20.0), "the last of the first row");
        assert_eq!(grid.row_span(4), (20.0, 40.0));
    }
}
