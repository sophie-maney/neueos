//! Virtual rows: the content of a [`crate::VirtualList`]. Only the rows on screen, and
//! half a screen either side, exist as widgets; the scroll view tells the rows which part
//! of them shows, and they build and drop rows to match (ADR 0043).

use std::any::Any;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::ops::Range;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesRef, RegisterCtx,
    Widget, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, Size};
use masonry_core::layout::{LenReq, Length};

use super::scroll::{ScrollAxes, ScrollWidget};
use crate::Cx;
use crate::element::{Element, Handle};

/// The rows built a row widget or dropped one: the app takes the new rows' message
/// handlers ([`RowsWidget::take_wiring`]) and forgets the dropped rows' ones.
#[derive(Debug)]
pub(crate) struct Rewired;

/// Builds row `i`: its widget, and its wiring (an `Element`'s `Wiring<M>`, type-erased
/// because widgets aren't generic over the app's message type).
pub(crate) type MakeRow = Box<dyn FnMut(usize) -> (NewWidget<dyn Widget>, Box<dyn Any>)>;

/// The Masonry widget holding a virtual list's rows.
pub(crate) struct RowsWidget {
    count: usize,
    row_height: f64,
    make: MakeRow,
    rows: BTreeMap<usize, WidgetPod<dyn Widget>>,
    /// The part on screen, from the top, in logical pixels.
    window: (f64, f64),
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
            make,
            rows: BTreeMap::new(),
            window: (0.0, 0.0),
            pending: Vec::new(),
            dropped: false,
        }
    }

    /// The rows to have built: those in the window, and half a window either side.
    fn wanted(&self) -> Range<usize> {
        let (top, bottom) = self.window;
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
        first..last
    }

    /// The scroll view moved or resized: build and drop rows to match.
    pub(crate) fn set_window(this: &mut WidgetMut<'_, Self>, window: (f64, f64)) {
        this.widget.window = window;
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
        this.widget.count = count;
        this.ctx.request_layout();
        Self::sync(this);
    }

    /// Drops every row and builds the visible ones again (the data changed).
    pub(crate) fn edit_reload(this: &mut WidgetMut<'_, Self>) {
        let rows = std::mem::take(&mut this.widget.rows);
        this.widget.dropped |= !rows.is_empty();
        for row in rows.into_values() {
            this.ctx.remove_child(row);
        }
        Self::sync(this);
    }

    /// The wiring of rows built since the last call, and whether rows were dropped.
    pub(crate) fn take_wiring(this: &mut WidgetMut<'_, Self>) -> (Vec<Box<dyn Any>>, bool) {
        let dropped = std::mem::take(&mut this.widget.dropped);
        (std::mem::take(&mut this.widget.pending), dropped)
    }

    /// Where row `i` sits, from the top.
    pub(crate) fn row_span(&self, i: usize) -> (f64, f64) {
        #[expect(clippy::cast_precision_loss, reason = "row counts far below 2^52")]
        let y = i as f64 * self.row_height;
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

impl Widget for RowsWidget {
    type Action = Rewired;

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
        _cross_length: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Vertical => Length::px(self.row_span(self.count).0),
            // As wide as the view: rows are laid out to its width.
            Axis::Horizontal => match len_req {
                LenReq::FitContent(space) => space,
                LenReq::MinContent | LenReq::MaxContent => Length::ZERO,
            },
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let height = self.row_height;
        for (&i, row) in &mut self.rows {
            #[expect(clippy::cast_precision_loss, reason = "row counts far below 2^52")]
            let y = i as f64 * height;
            ctx.run_layout(row, Size::new(size.width, height));
            ctx.place_child(row, Point::new(0.0, y));
        }
        ctx.clear_baselines();
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::List
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
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
        rows.window = window;
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
}
