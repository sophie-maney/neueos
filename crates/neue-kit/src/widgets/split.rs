//! The split view: panes side by side or stacked, with a divider between each pair. Drag a
//! divider to resize the two panes next to it; double-click it (or press Return while it
//! has focus) to collapse a collapsible pane or bring it back. When the view itself
//! resizes, panes that don't hold their size take the difference.

use std::any::TypeId;

use masonry_core::accesskit::{Action, ActionData, Node, Orientation, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget,
    WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Point, Rect, RoundedRect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{DividerStyle, focus_ring, split_divider};
use neue_theme::materials::split::{COLLAPSE_AT, DISABLED_OPACITY, KEY_STEP, MIN_PANE, THIN_GRAB};
use neue_theme::metrics::control::FOCUS_RING;

use crate::Cx;
use crate::element::{Element, Handle, Wiring};
use crate::theme::ThemeProp;

/// What a split view emits when the user resizes it: the length of every pane along the
/// split, collapsed panes as zero.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Resized(pub(crate) Vec<f64>);

/// How one pane behaves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pane {
    initial: Option<f64>,
    min: f64,
    max: f64,
    holds: bool,
    collapsible: bool,
    collapsed: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self::new()
    }
}

impl Pane {
    /// A pane that shares the room left over by the panes with a size, and can't be
    /// dragged below a small minimum.
    pub const fn new() -> Self {
        Self {
            initial: None,
            min: MIN_PANE,
            max: f64::INFINITY,
            holds: false,
            collapsible: false,
            collapsed: false,
        }
    }

    /// Starts this long (wide in a side-by-side split, tall in a stacked one), in logical
    /// pixels.
    #[must_use]
    pub const fn size(mut self, length: f64) -> Self {
        self.initial = Some(length);
        self
    }

    /// The shortest the pane gets by dragging (a collapsible pane may still collapse).
    #[must_use]
    pub const fn min(mut self, length: f64) -> Self {
        self.min = length;
        self
    }

    /// The longest the pane gets.
    #[must_use]
    pub const fn max(mut self, length: f64) -> Self {
        self.max = length;
        self
    }

    /// Keeps its length when the split view resizes, as long as the other panes can take
    /// the difference. For a sidebar.
    #[must_use]
    pub const fn holds_size(mut self) -> Self {
        self.holds = true;
        self
    }

    /// Collapses to nothing when dragged well below its minimum, or when its divider is
    /// double-clicked.
    #[must_use]
    pub const fn collapsible(mut self) -> Self {
        self.collapsible = true;
        self
    }

    /// Starts collapsed (only a collapsible pane can be).
    #[must_use]
    pub const fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Where a malformed range (a maximum below the minimum, NaN) is made well-formed.
    fn sane(mut self) -> Self {
        self.min = if self.min.is_finite() {
            self.min.max(0.0)
        } else {
            0.0
        };
        self.max = if self.max >= self.min {
            self.max
        } else {
            self.min
        };
        self.collapsed &= self.collapsible;
        self
    }
}

/// Clamps `sizes` to each pane's range (collapsed ones to zero), then gives the difference
/// to `available` to the panes that don't hold their size, in proportion to their size;
/// if none can take it, to the ones that do.
fn fit(sizes: &mut [f64], panes: &[Pane], collapsed: &[bool], available: f64) {
    let n = sizes.len();
    for i in 0..n {
        sizes[i] = if collapsed[i] {
            0.0
        } else {
            sizes[i].clamp(panes[i].min, panes[i].max)
        };
    }
    for _ in 0..=n {
        let diff = available - sizes.iter().sum::<f64>();
        if diff.abs() < 0.01 {
            break;
        }
        let grow = diff > 0.0;
        let can = |i: usize, sizes: &[f64]| {
            !collapsed[i]
                && if grow {
                    sizes[i] < panes[i].max - 0.01
                } else {
                    sizes[i] > panes[i].min + 0.01
                }
        };
        let mut pool: Vec<usize> = (0..n)
            .filter(|&i| can(i, sizes) && !panes[i].holds)
            .collect();
        if pool.is_empty() {
            pool = (0..n).filter(|&i| can(i, sizes)).collect();
        }
        if pool.is_empty() {
            break;
        }
        let weight: f64 = pool.iter().map(|&i| sizes[i].max(1.0)).sum();
        for &i in &pool {
            let share = diff * sizes[i].max(1.0) / weight;
            sizes[i] = (sizes[i] + share).clamp(panes[i].min, panes[i].max);
        }
    }
}

/// Where a pair of neighbouring panes with `total` between them ends up when the divider
/// between them is asked to leave `desired` for the first: its length, the second's, and
/// whether either collapses.
fn drag_pair(a: &Pane, b: &Pane, total: f64, desired: f64) -> (f64, f64, bool, bool) {
    if a.collapsible && desired < a.min * COLLAPSE_AT && total <= b.max {
        return (0.0, total, true, false);
    }
    if b.collapsible && total - desired < b.min * COLLAPSE_AT && total <= a.max {
        return (total, 0.0, false, true);
    }
    let lo = a.min.max(total - b.max);
    let hi = a.max.min(total - b.min);
    let first = if lo > hi { lo } else { desired.clamp(lo, hi) };
    (first, total - first, false, false)
}

/// A divider being dragged.
#[derive(Clone, Copy, Debug)]
struct Drag {
    divider: usize,
    /// The pointer's position along the split when the drag started.
    start: f64,
    /// The first pane's length then.
    first: f64,
}

/// The strip over a divider that a press grabs: wider than a thin divider is drawn, and in
/// front of the panes so that the pointer lands on it, not on what a pane has there. It
/// only gives the resize cursor; the split view handles the press (events bubble up).
#[derive(Debug)]
struct GrabWidget(CursorIcon);

impl Widget for GrabWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::ZERO
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn get_cursor(&self, _ctx: &QueryCtx<'_>, _pos: Point) -> CursorIcon {
        self.0
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_hidden();
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// The Masonry widget behind [`crate::SplitView`].
#[derive(Debug)]
pub(crate) struct SplitWidget {
    /// The axis the panes follow: horizontal for side by side.
    axis: Axis,
    style: DividerStyle,
    panes: Vec<WidgetPod<dyn Widget>>,
    /// The grab strip over each divider, in front of the panes.
    grabs: Vec<WidgetPod<GrabWidget>>,
    specs: Vec<Pane>,
    sizes: Vec<f64>,
    collapsed: Vec<bool>,
    /// A collapsed pane's length before it collapsed.
    restore: Vec<f64>,
    /// Whether `sizes` came from the panes' first sizes yet.
    sized: bool,
    /// The dividers' rectangles from the last layout.
    dividers: Vec<Rect>,
    /// Where a press grabs each divider: the divider, wider for the thin style.
    grab_rects: Vec<Rect>,
    drag: Option<Drag>,
    /// The divider the keyboard moves.
    active: usize,
    fixed: (Option<f64>, Option<f64>),
    replayer: ImagingReplayer,
}

fn along(axis: Axis, p: Point) -> f64 {
    match axis {
        Axis::Horizontal => p.x,
        Axis::Vertical => p.y,
    }
}

impl SplitWidget {
    pub(crate) fn new(axis: Axis, style: DividerStyle) -> Self {
        Self {
            axis,
            style,
            panes: Vec::new(),
            grabs: Vec::new(),
            specs: Vec::new(),
            sizes: Vec::new(),
            collapsed: Vec::new(),
            restore: Vec::new(),
            sized: false,
            dividers: Vec::new(),
            grab_rects: Vec::new(),
            drag: None,
            active: 0,
            fixed: (None, None),
            replayer: ImagingReplayer::new(),
        }
    }

    pub(crate) fn push(&mut self, child: NewWidget<dyn Widget>, pane: Pane) {
        let pane = pane.sane();
        self.panes.push(child.to_pod());
        if self.panes.len() > 1 {
            let cursor = match self.axis {
                Axis::Horizontal => CursorIcon::ColResize,
                Axis::Vertical => CursorIcon::RowResize,
            };
            self.grabs.push(NewWidget::new(GrabWidget(cursor)).to_pod());
        }
        self.sizes.push(pane.initial.unwrap_or(0.0));
        self.collapsed.push(pane.collapsed);
        self.restore.push(pane.initial.unwrap_or(0.0));
        self.specs.push(pane);
    }

    fn gaps(&self) -> f64 {
        self.style.width() * self.panes.len().saturating_sub(1) as f64
    }

    fn lengths(&self) -> Vec<f64> {
        self.sizes.clone()
    }

    /// The pane that takes over the room of pane `i` when it collapses.
    fn neighbour(&self, i: usize) -> usize {
        if i + 1 < self.panes.len() {
            i + 1
        } else {
            i.saturating_sub(1)
        }
    }

    fn collapse(&mut self, i: usize) {
        if i >= self.panes.len() || self.collapsed[i] || !self.specs[i].collapsible {
            return;
        }
        let n = self.neighbour(i);
        if n == i {
            return;
        }
        self.restore[i] = if self.sizes[i] > 0.0 {
            self.sizes[i]
        } else {
            self.specs[i].min.max(MIN_PANE)
        };
        self.sizes[n] += self.sizes[i];
        self.sizes[i] = 0.0;
        self.collapsed[i] = true;
    }

    fn expand(&mut self, i: usize) {
        if i >= self.panes.len() || !self.collapsed[i] {
            return;
        }
        let n = self.neighbour(i);
        let spec = self.specs[i];
        let want = self.restore[i].clamp(spec.min, spec.max);
        let free = (self.sizes[n] - self.specs[n].min).max(0.0);
        let take = want.min(free).max(0.0);
        self.sizes[i] = take;
        self.sizes[n] -= take;
        self.collapsed[i] = false;
    }

    /// Asks for the first pane next to `divider` to be `desired` long.
    fn set_first(&mut self, divider: usize, desired: f64) {
        let (a, b) = (divider, divider + 1);
        if b >= self.panes.len() {
            return;
        }
        let total = self.sizes[a] + self.sizes[b];
        let (la, lb, ca, cb) = drag_pair(&self.specs[a], &self.specs[b], total, desired);
        for (i, collapsed) in [(a, ca), (b, cb)] {
            if collapsed && !self.collapsed[i] {
                self.restore[i] = if self.sizes[i] > 0.0 {
                    self.sizes[i]
                } else {
                    self.specs[i].min.max(MIN_PANE)
                };
            }
            self.collapsed[i] = collapsed;
        }
        self.sizes[a] = la;
        self.sizes[b] = lb;
    }

    /// Collapses or brings back the collapsible pane next to `divider` (the first one if
    /// both are).
    fn toggle(&mut self, divider: usize) {
        let (a, b) = (divider, divider + 1);
        if b >= self.panes.len() {
            return;
        }
        let target = if self.collapsed[a] {
            Some(a)
        } else if self.collapsed[b] {
            Some(b)
        } else if self.specs[a].collapsible {
            Some(a)
        } else if self.specs[b].collapsible {
            Some(b)
        } else {
            None
        };
        if let Some(i) = target {
            if self.collapsed[i] {
                self.expand(i);
            } else {
                self.collapse(i);
            }
        }
    }

    /// The divider under `pos` (local), if any.
    fn divider_at(&self, pos: Point) -> Option<usize> {
        self.grab_rects.iter().position(|r| r.contains(pos))
    }

    fn changed(&mut self, ctx: &mut EventCtx<'_>) {
        ctx.submit_action::<Resized>(Resized(self.lengths()));
        ctx.request_layout();
        ctx.request_accessibility_update();
    }

    /// Moves the keyboard's divider by `delta`, bringing back a collapsed pane in the
    /// direction it moves from.
    fn nudge(&mut self, ctx: &mut EventCtx<'_>, delta: f64) {
        let d = self.active;
        if d + 1 >= self.panes.len() {
            return;
        }
        if delta > 0.0 && self.collapsed[d] {
            self.expand(d);
        } else if delta < 0.0 && self.collapsed[d + 1] {
            self.expand(d + 1);
        } else {
            self.set_first(d, self.sizes[d] + delta);
        }
        self.changed(ctx);
    }

    pub(crate) fn edit_position(this: &mut WidgetMut<'_, Self>, pane: usize, length: f64) {
        if pane + 1 < this.widget.panes.len() && length.is_finite() {
            this.widget.set_first(pane, length);
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }

    pub(crate) fn edit_collapsed(this: &mut WidgetMut<'_, Self>, pane: usize, collapsed: bool) {
        if pane < this.widget.panes.len() {
            if collapsed {
                this.widget.collapse(pane);
            } else {
                this.widget.expand(pane);
            }
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }

    /// Pane lengths and collapsed flags, for tests.
    #[cfg(test)]
    pub(crate) fn state(&self) -> (Vec<f64>, Vec<bool>) {
        (self.sizes.clone(), self.collapsed.clone())
    }

    /// The dividers' rectangles, for tests.
    #[cfg(test)]
    pub(crate) fn divider_rects(&self) -> &[Rect] {
        &self.dividers
    }

    pub(crate) fn set_fixed(&mut self, width: Option<f64>, height: Option<f64>) {
        self.fixed = (width.or(self.fixed.0), height.or(self.fixed.1));
    }
}

impl UsesProperty<ThemeProp> for SplitWidget {}

impl Widget for SplitWidget {
    type Action = Resized;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let pos = ctx.local_position(state.position);
                let Some(d) = self.divider_at(pos) else {
                    return;
                };
                ctx.request_focus();
                self.active = d;
                ctx.set_handled();
                if state.count == 2 {
                    self.toggle(d);
                    self.changed(ctx);
                } else {
                    self.drag = Some(Drag {
                        divider: d,
                        start: along(self.axis, pos),
                        first: self.sizes[d],
                    });
                    ctx.capture_pointer();
                }
                ctx.request_post_paint();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                if let Some(drag) = self.drag {
                    let pos = along(self.axis, ctx.local_position(current.position));
                    self.set_first(drag.divider, drag.first + pos - drag.start);
                    self.changed(ctx);
                }
            }
            PointerEvent::Up(..) | PointerEvent::Cancel(..) => self.drag = None,
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if key.state.is_down() && ctx.is_focus_target() => {
                let d = self.active;
                let (back, forward, before, after) = match self.axis {
                    Axis::Horizontal => (
                        NamedKey::ArrowLeft,
                        NamedKey::ArrowRight,
                        NamedKey::ArrowUp,
                        NamedKey::ArrowDown,
                    ),
                    Axis::Vertical => (
                        NamedKey::ArrowUp,
                        NamedKey::ArrowDown,
                        NamedKey::ArrowLeft,
                        NamedKey::ArrowRight,
                    ),
                };
                let Key::Named(named) = key.key else {
                    return;
                };
                if named == back {
                    self.nudge(ctx, -KEY_STEP);
                } else if named == forward {
                    self.nudge(ctx, KEY_STEP);
                } else if named == before {
                    self.active = d.saturating_sub(1);
                    ctx.request_post_paint();
                } else if named == after {
                    self.active = (d + 1).min(self.panes.len().saturating_sub(2));
                    ctx.request_post_paint();
                } else if named == NamedKey::Home {
                    self.set_first(d, 0.0);
                    self.changed(ctx);
                } else if named == NamedKey::End {
                    self.set_first(d, f64::MAX / 4.0);
                    self.changed(ctx);
                } else if named == NamedKey::Enter {
                    self.toggle(d);
                    self.changed(ctx);
                } else {
                    return;
                }
                ctx.set_handled();
            }
            TextEvent::WindowFocusChange(_) => ctx.request_post_paint(),
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match (&event.action, &event.data) {
            (Action::Increment, _) => self.nudge(ctx, KEY_STEP),
            (Action::Decrement, _) => self.nudge(ctx, -KEY_STEP),
            (Action::SetValue, Some(ActionData::NumericValue(v)))
                if self.active + 1 < self.panes.len() =>
            {
                self.set_first(self.active, *v);
                self.changed(ctx);
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for pane in &mut self.panes {
            ctx.register_child(pane);
        }
        for grab in &mut self.grabs {
            ctx.register_child(grab);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(_) | Update::DisabledChanged(_)) {
            ctx.request_post_paint();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let fixed = match axis {
            Axis::Horizontal => self.fixed.0,
            Axis::Vertical => self.fixed.1,
        };
        if let Some(length) = fixed {
            return Length::px(length);
        }
        if axis == self.axis {
            let panes: f64 = self
                .specs
                .iter()
                .zip(&self.collapsed)
                .filter(|&(_, &c)| !c)
                .map(|(s, _)| s.initial.unwrap_or(s.min.max(MIN_PANE)).clamp(s.min, s.max))
                .sum();
            Length::px(panes + self.gaps())
        } else {
            let mut length = Length::ZERO;
            for pane in &mut self.panes {
                length = length.max(ctx.compute_length(
                    pane,
                    len_req.into(),
                    masonry_core::layout::LayoutSize::maybe(axis.cross(), cross_length),
                    axis,
                    cross_length,
                ));
            }
            length
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let main = size.get_coord(self.axis);
        let cross = size.get_coord(self.axis.cross());
        let room = (main - self.gaps()).max(0.0);
        if !self.sized {
            // First layout: panes without a size share what the others leave.
            self.sized = true;
            let given: f64 = self
                .specs
                .iter()
                .zip(&self.collapsed)
                .filter(|&(_, &c)| !c)
                .filter_map(|(s, _)| s.initial)
                .sum();
            let open = self
                .specs
                .iter()
                .zip(&self.collapsed)
                .filter(|&(s, &c)| !c && s.initial.is_none())
                .count();
            if open > 0 {
                #[expect(clippy::cast_precision_loss, reason = "pane counts are tiny")]
                let each = ((room - given) / open as f64).max(0.0);
                for (i, spec) in self.specs.iter().enumerate() {
                    if spec.initial.is_none() {
                        self.sizes[i] = each;
                    }
                }
            }
        }
        fit(&mut self.sizes, &self.specs, &self.collapsed, room);

        let width = self.style.width();
        self.dividers.clear();
        self.grab_rects.clear();
        let grab = match self.style {
            DividerStyle::Thin => THIN_GRAB,
            _ => 0.0,
        };
        let mut offset = 0.0_f64;
        let last = self.panes.len().saturating_sub(1);
        for (i, pane) in self.panes.iter_mut().enumerate() {
            let start = offset.round();
            let end = (offset + self.sizes[i]).round();
            let length = end - start;
            ctx.set_stashed(pane, self.collapsed[i]);
            if !self.collapsed[i] {
                let (pane_size, origin) = match self.axis {
                    Axis::Horizontal => (Size::new(length, cross), Point::new(start, 0.0)),
                    Axis::Vertical => (Size::new(cross, length), Point::new(0.0, start)),
                };
                ctx.run_layout(pane, pane_size);
                ctx.place_child(pane, origin);
            }
            offset += self.sizes[i];
            if i < last {
                let at = end;
                self.dividers.push(match self.axis {
                    Axis::Horizontal => Rect::new(at, 0.0, at + width, cross),
                    Axis::Vertical => Rect::new(0.0, at, cross, at + width),
                });
                offset += width;
            }
        }
        for (i, divider) in self.dividers.iter().enumerate() {
            let rect = match self.axis {
                Axis::Horizontal => divider.inflate(grab, 0.0),
                Axis::Vertical => divider.inflate(0.0, grab),
            };
            self.grab_rects.push(rect);
            if let Some(pod) = self.grabs.get_mut(i) {
                ctx.run_layout(pod, rect.size());
                ctx.place_child(pod, rect.origin());
            }
        }
        // `offset` carries each divider on after the rounded end of the pane before it.
        ctx.set_paint_insets(Insets::uniform(FOCUS_RING));
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
        let vertical = self.axis == Axis::Horizontal;
        let disabled = ctx.is_disabled();
        if disabled {
            scene.push_opacity(DISABLED_OPACITY);
        }
        for rect in &self.dividers {
            split_divider(&mut scene, *rect, self.style, vertical, theme.variant);
        }
        if disabled {
            scene.pop_layer();
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn post_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if !(ctx.is_focus_target() && ctx.is_window_focused()) {
            return;
        }
        let Some(rect) = self.dividers.get(self.active) else {
            return;
        };
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        focus_ring(
            &mut scene,
            RoundedRect::from_rect(*rect, 0.0),
            theme.variant,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        if self.drag.is_some() || self.divider_at(ctx.to_local(pos)).is_some() {
            match self.axis {
                Axis::Horizontal => CursorIcon::ColResize,
                Axis::Vertical => CursorIcon::RowResize,
            }
        } else {
            CursorIcon::Default
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Splitter
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        // A vertical bar separates side-by-side panes.
        node.set_orientation(match self.axis {
            Axis::Horizontal => Orientation::Vertical,
            Axis::Vertical => Orientation::Horizontal,
        });
        let d = self.active;
        if d + 1 < self.panes.len() {
            node.set_numeric_value(self.sizes[d]);
            node.set_min_numeric_value(self.specs[d].min.min(self.sizes[d]));
            node.set_max_numeric_value(self.sizes[d] + self.sizes[d + 1]);
            node.add_action(Action::SetValue);
        }
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
    }

    fn children_ids(&self) -> ChildrenIds {
        // The grabs come last: they are in front of the panes.
        self.panes
            .iter()
            .map(WidgetPod::id)
            .chain(self.grabs.iter().map(WidgetPod::id))
            .collect()
    }

    fn accepts_focus(&self) -> bool {
        self.panes.len() > 1
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("{:?}", self.sizes))
    }
}

/// Turns the panes' lengths into a message.
type ResizeHandler<M> = Box<dyn FnMut(&[f64]) -> M>;

/// Panes side by side or stacked, with draggable dividers between them. Each pane is any
/// element; give it a [`Pane`] for its size limits, whether it holds its size when the
/// split view resizes, and whether it collapses. Sends the panes' lengths when the user
/// resizes them. Tab focuses the dividers; the arrow keys along the split move the
/// focused one, the ones across pick another divider, Home and End move it to either
/// end, and Return collapses or restores a pane.
pub struct SplitView<M> {
    inner: NewWidget<SplitWidget>,
    panes: Vec<(Element<M>, Pane)>,
    on_resize: Option<ResizeHandler<M>>,
}

impl<M> std::fmt::Debug for SplitView<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplitView")
            .field("inner", &self.inner)
            .field("panes", &self.panes.len())
            .finish()
    }
}

impl<M: 'static> SplitView<M> {
    /// Panes side by side, left to right, with vertical dividers.
    pub fn horizontal() -> Self {
        Self::new(Axis::Horizontal)
    }

    /// Panes stacked, top to bottom, with horizontal dividers.
    pub fn vertical() -> Self {
        Self::new(Axis::Vertical)
    }

    fn new(axis: Axis) -> Self {
        Self {
            inner: NewWidget::new(SplitWidget::new(axis, DividerStyle::Thin)),
            panes: Vec::new(),
            on_resize: None,
        }
    }

    /// Adds a pane that shares the leftover room with the other panes without a size.
    #[must_use]
    pub fn with(self, pane: impl Into<Element<M>>) -> Self {
        self.with_pane(pane, Pane::new())
    }

    /// Adds a pane with its own limits.
    #[must_use]
    pub fn with_pane(mut self, pane: impl Into<Element<M>>, options: Pane) -> Self {
        self.panes.push((pane.into(), options));
        self
    }

    /// A thin line or a thick bar with a grip.
    #[must_use]
    pub fn divider(mut self, style: DividerStyle) -> Self {
        self.inner.widget.style = style;
        self
    }

    /// Sends the message `make` returns, given every pane's length (collapsed ones zero)
    /// after the user moved a divider. Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_resize(mut self, make: impl FnMut(&[f64]) -> M + 'static) -> Self {
        self.on_resize = Some(Box::new(make));
        self
    }

    /// Fixes the width, in logical pixels.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.inner.widget.set_fixed(Some(width), None);
        self
    }

    /// Fixes the height, in logical pixels.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.inner.widget.set_fixed(None, Some(height));
        self
    }

    /// Points `handle` at this split view.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<SplitView<M>> for Element<M> {
    fn from(view: SplitView<M>) -> Self {
        let SplitView {
            mut inner,
            panes,
            on_resize,
        } = view;
        let id = inner.id();
        let mut wiring = Wiring::default();
        for (pane, options) in panes {
            inner.widget.push(wiring.adopt(pane), options);
        }
        let mut element = Element::with_wiring(inner.erased(), wiring);
        if let Some(mut make) = on_resize {
            element.on_action(id, move |action| {
                let Resized(lengths) = action.downcast_ref::<Resized>()?;
                Some(make(lengths))
            });
        }
        element
    }
}

impl<M> Handle<SplitView<M>> {
    /// Makes pane `pane` `length` long, at the expense of (or to the benefit of) the pane
    /// after it, within their limits. Does nothing for the last pane.
    pub fn set_position<N>(&self, cx: &mut Cx<'_, N>, pane: usize, length: f64) {
        cx.edit::<SplitWidget, _>(self, |mut w| {
            SplitWidget::edit_position(&mut w, pane, length);
        });
    }

    /// Collapses a collapsible pane, or brings it back at the length it had.
    pub fn set_collapsed<N>(&self, cx: &mut Cx<'_, N>, pane: usize, collapsed: bool) {
        cx.edit::<SplitWidget, _>(self, |mut w| {
            SplitWidget::edit_collapsed(&mut w, pane, collapsed);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(min: f64, max: f64) -> Pane {
        Pane::new().min(min).max(max).sane()
    }

    #[test]
    fn fit_gives_the_difference_to_panes_that_do_not_hold() {
        let panes = [pane(50.0, 300.0).holds_size(), pane(50.0, 1000.0)];
        let mut sizes = [200.0, 300.0];
        fit(&mut sizes, &panes, &[false; 2], 700.0);
        assert_eq!(sizes, [200.0, 500.0]);
        fit(&mut sizes, &panes, &[false; 2], 400.0);
        assert_eq!(sizes, [200.0, 200.0]);
    }

    #[test]
    fn fit_falls_back_to_the_holders_and_respects_limits() {
        let panes = [pane(50.0, 300.0).holds_size(), pane(50.0, 120.0)];
        let mut sizes = [200.0, 100.0];
        // The second pane stops at 120; the holder takes the rest.
        fit(&mut sizes, &panes, &[false; 2], 400.0);
        assert_eq!(sizes, [280.0, 120.0]);
        // Too little room: both stop at their minimum.
        fit(&mut sizes, &panes, &[false; 2], 40.0);
        assert_eq!(sizes, [50.0, 50.0]);
    }

    #[test]
    fn fit_shares_in_proportion_and_keeps_collapsed_at_zero() {
        let panes = [pane(0.0, 1e9), pane(0.0, 1e9), pane(0.0, 1e9)];
        let mut sizes = [100.0, 300.0, 50.0];
        fit(&mut sizes, &panes, &[false, false, true], 800.0);
        assert_eq!(sizes[2], 0.0);
        assert!((sizes[0] - 200.0).abs() < 1e-6 && (sizes[1] - 600.0).abs() < 1e-6);
    }

    #[test]
    fn drag_stays_within_limits_and_collapses_past_half_the_minimum() {
        let a = pane(100.0, 400.0).collapsible();
        let b = pane(100.0, 600.0);
        assert_eq!(
            drag_pair(&a, &b, 500.0, 250.0),
            (250.0, 250.0, false, false)
        );
        assert_eq!(drag_pair(&a, &b, 500.0, 70.0), (100.0, 400.0, false, false));
        assert_eq!(drag_pair(&a, &b, 500.0, 40.0), (0.0, 500.0, true, false));
        // The second isn't collapsible: it stops at its minimum.
        assert_eq!(
            drag_pair(&a, &b, 500.0, 480.0),
            (400.0, 100.0, false, false)
        );
        // A pane too long for the other's maximum can't collapse into it.
        assert_eq!(
            drag_pair(&a, &pane(0.0, 300.0), 500.0, 10.0),
            (200.0, 300.0, false, false)
        );
    }
}
