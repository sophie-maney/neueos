//! The scroll view: a clipped viewport onto content larger than itself, with overlay
//! scrollers (ADR 0043).
//!
//! The view scrolls by translating its content at compose time, so scrolling repaints but
//! never re-lays-out the content. The scrollers are child widgets above the content, so
//! they take the pointer where they're drawn; they're stashed (no paint, no pointer)
//! while hidden. A [`RowsWidget`] content is told which rows are on screen
//! (virtual scrolling). A table's header sits above the viewport and follows the content
//! sideways (ADR 0048).

use std::any::TypeId;
use std::time::Duration;

use masonry_core::accesskit::{Action, ActionData, Node, Orientation, Role, ScrollUnit};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, AllowRawMut, ChildrenIds, ComposeCtx, EventCtx, LayoutCtx, MeasureCtx,
    NewWidget, NoAction, PaintCtx, PointerButtonEvent, PointerEvent, PointerScrollEvent,
    PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
    UsesProperty, Widget, WidgetMut, WidgetPod,
};
use masonry_core::dpi::{LogicalPosition, PhysicalPosition};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Point, Rect, RoundedRect, Size, Vec2};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{
    FieldShape, Scroller, TextField as WellSpec, focus_ring, scroller, scroller_knob,
    scroller_travel, table_border, text_field, text_field_inside,
};
use neue_theme::Spring;
use neue_theme::materials::table::BORDER;
use neue_theme::metrics::control::{FOCUS_RING, SCROLL_LINE, SCROLLER_WIDTH};
use neue_theme::motion::SCROLLER_HIDE_DELAY;

use super::rows::RowsWidget;
use crate::Cx;
use crate::anim::Animated;
use crate::element::{Element, Handle, Wiring};
use crate::theme::ThemeProp;

/// The directions a [`crate::ScrollView`] scrolls in. Along the others, the content is
/// as long as the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScrollAxes {
    /// Up and down; the content is as wide as the view (lists, documents).
    #[default]
    Vertical,
    /// Left and right; the content is as tall as the view.
    Horizontal,
    /// Both ways (images, canvases).
    Both,
}

impl ScrollAxes {
    fn scrolls(self, axis: Axis) -> bool {
        matches!(
            (self, axis),
            (Self::Both, _)
                | (Self::Vertical, Axis::Vertical)
                | (Self::Horizontal, Axis::Horizontal)
        )
    }
}

/// `v` along `axis`.
fn get(v: Vec2, axis: Axis) -> f64 {
    match axis {
        Axis::Horizontal => v.x,
        Axis::Vertical => v.y,
    }
}

/// Where a viewport starting at `start`, `len` long, starts once it shows as much of
/// `target` as fits (its start, if it doesn't fit), moving as little as it can.
fn reveal(start: f64, len: f64, target: (f64, f64)) -> f64 {
    let (t0, t1) = target;
    if t1 - t0 >= len || t0 < start {
        t0
    } else if t1 > start + len {
        t1 - len
    } else {
        start
    }
}

/// What a scroller asks its scroll view for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Request {
    /// Scroll to this position, from 0 to 1 (the knob was dragged).
    To(f64),
    /// One page back (-1) or on (1): a press on the track beside the knob.
    Page(f64),
}

/// One overlay scroller. Its scroll view sets what it shows (in compose) and takes its
/// requests after pointer events.
#[derive(Debug)]
pub(crate) struct ScrollerWidget {
    spec: Scroller,
    expansion: Animated,
    /// While the knob is dragged: the pointer's position along the axis when the press
    /// began, and the scroll position then.
    drag: Option<(f64, f64)>,
    request: Option<Request>,
    /// The pointer is over it or dragging it: it stays on screen.
    held: bool,
    replayer: ImagingReplayer,
}

impl ScrollerWidget {
    fn new(vertical: bool) -> Self {
        Self {
            spec: Scroller {
                vertical,
                ..Scroller::default()
            },
            expansion: Animated::new(0.0, Spring::SNAPPY),
            drag: None,
            request: None,
            held: false,
            replayer: ImagingReplayer::new(),
        }
    }

    fn along(&self, p: Point) -> f64 {
        if self.spec.vertical { p.y } else { p.x }
    }
}

impl AllowRawMut for ScrollerWidget {}
impl UsesProperty<ThemeProp> for ScrollerWidget {}

impl Widget for ScrollerWidget {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let rect = ctx.content_box();
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let p = ctx.local_position(state.position);
                ctx.capture_pointer();
                let knob = scroller_knob(rect, &self.spec).rect();
                if knob.contains(p) {
                    self.drag = Some((self.along(p), self.spec.position));
                } else {
                    let before = self.along(p) < self.along(knob.origin());
                    self.request = Some(Request::Page(if before { -1.0 } else { 1.0 }));
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                if let Some((from, start)) = self.drag {
                    let travel = scroller_travel(rect, &self.spec);
                    if travel > 0.0 {
                        let moved = self.along(ctx.local_position(current.position)) - from;
                        self.request = Some(Request::To((start + moved / travel).clamp(0.0, 1.0)));
                    }
                }
            }
            PointerEvent::Up(..) | PointerEvent::Cancel(..) => self.drag = None,
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::HoveredChanged(_) | Update::ActiveChanged(_) = event {
            let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
            self.held = ctx.is_hovered() || ctx.is_active();
            let wide = if self.held { 1.0 } else { 0.0 };
            self.expansion.animate_to(ctx, &theme, wide);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.expansion.on_anim_frame(ctx, &theme, interval);
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            self.expansion.theme_changed(ctx);
            ctx.request_render();
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // The scroll view sizes its scrollers itself.
        Length::px(SCROLLER_WIDTH)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 amount")]
        let spec = Scroller {
            expanded: self.expansion.value() as f32,
            ..self.spec
        };
        let mut scene = Scene::new(size.width, size.height);
        scroller(&mut scene, size.to_rect(), spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::ScrollBar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_orientation(if self.spec.vertical {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        });
        node.set_numeric_value(self.spec.position);
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(1.0);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// The Masonry widget behind [`crate::ScrollView`] and [`crate::VirtualList`].
/// What a scroll view draws around its viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Frame {
    /// Nothing: the view is a Tab stop with its own focus ring.
    #[default]
    None,
    /// A text field's well (a text view, ADR 0044), with the focus ring for focus inside
    /// it; the view itself is no Tab stop.
    Well,
    /// A hairline border (a table, ADR 0048), with the focus ring for focus inside it.
    Border,
    /// A source list's sidebar (ADR 0050): no border and no focus ring, since the selection
    /// shows the focus; the view itself is no Tab stop.
    Sidebar,
}

#[derive(Debug)]
pub(crate) struct ScrollWidget {
    content: WidgetPod<dyn Widget>,
    /// A table's header: above the viewport, scrolled sideways with the content.
    header: Option<WidgetPod<dyn Widget>>,
    header_height: f64,
    /// Vertical, then horizontal; last in z-order, so above the content.
    scrollers: [WidgetPod<ScrollerWidget>; 2],
    pub(super) axes: ScrollAxes,
    /// Preferred viewport size; `None` follows the space it's given.
    pub(super) width: Option<f64>,
    pub(super) height: Option<f64>,
    /// How far the content is scrolled: the content point at the viewport's top left.
    offset: Vec2,
    content_size: Size,
    viewport: Size,
    /// The content is a [`RowsWidget`], told which part of it is on screen.
    rows: bool,
    /// The part of the content last told to the rows.
    told: Option<Rect>,
    /// Where the viewport's top left is: inside the border, below the header.
    origin: Point,
    pub(super) frame: Frame,
    /// A scroll-into-view request the content's size at the time couldn't satisfy yet
    /// (the content grew in the same event): tried again after the next layout.
    pending: Option<Rect>,
    /// The scrollers are on screen (perhaps fading out).
    shown: bool,
    opacity: Animated,
    /// Time since the last scroll while the scrollers show and wait to fade.
    idle: Option<Duration>,
    replayer: ImagingReplayer,
}

impl ScrollWidget {
    pub(super) fn new(content: NewWidget<dyn Widget>, axes: ScrollAxes, rows: bool) -> Self {
        Self {
            content: content.to_pod(),
            header: None,
            header_height: 0.0,
            scrollers: [
                NewWidget::new(ScrollerWidget::new(true)).to_pod(),
                NewWidget::new(ScrollerWidget::new(false)).to_pod(),
            ],
            axes,
            width: None,
            height: None,
            offset: Vec2::ZERO,
            content_size: Size::ZERO,
            viewport: Size::ZERO,
            rows,
            told: None,
            origin: Point::ORIGIN,
            frame: Frame::None,
            pending: None,
            shown: false,
            opacity: Animated::new(0.0, Spring::STANDARD),
            idle: None,
            replayer: ImagingReplayer::new(),
        }
    }

    /// Width of the border around the viewport.
    fn border(&self) -> f64 {
        if self.frame == Frame::Border {
            BORDER
        } else {
            0.0
        }
    }

    /// Puts `header`, `height` tall, above the viewport (a table's column titles).
    pub(super) fn set_header(&mut self, header: NewWidget<dyn Widget>, height: f64) {
        self.header = Some(header.to_pod());
        self.header_height = height;
    }

    /// The header's widget, if there is one.
    #[cfg(test)]
    pub(crate) fn header_id(&self) -> Option<masonry_core::core::WidgetId> {
        self.header.as_ref().map(WidgetPod::id)
    }

    /// Runs `f` on the header (nothing without one).
    pub(super) fn with_header<R>(
        this: &mut WidgetMut<'_, Self>,
        f: impl FnOnce(WidgetMut<'_, dyn Widget>) -> R,
    ) -> Option<R> {
        let header = this.widget.header.as_mut()?;
        Some(f(this.ctx.get_mut(header)))
    }

    /// How far the content can scroll along each axis.
    fn range(&self) -> Vec2 {
        let r = (self.content_size - self.viewport).max(Size::ZERO);
        Vec2::new(
            if self.axes.scrolls(Axis::Horizontal) {
                r.width
            } else {
                0.0
            },
            if self.axes.scrolls(Axis::Vertical) {
                r.height
            } else {
                0.0
            },
        )
    }

    /// Moves to `offset` (kept in range). Returns whether it moved; the caller requests
    /// compose and an accessibility update.
    fn move_to(&mut self, offset: Vec2) -> bool {
        let range = self.range();
        let offset = Vec2::new(offset.x.clamp(0.0, range.x), offset.y.clamp(0.0, range.y));
        if offset == self.offset {
            return false;
        }
        self.offset = offset;
        true
    }

    /// Scrolls by `delta`; on a move, shows the scrollers and marks the event handled
    /// (so an enclosing scroll view doesn't scroll too). At an end the event goes on.
    fn scroll_by(&mut self, ctx: &mut EventCtx<'_>, delta: Vec2) {
        if self.move_to(self.offset + delta) {
            self.moved(ctx);
            ctx.set_handled();
        }
    }

    fn moved(&mut self, ctx: &mut EventCtx<'_>) {
        ctx.request_compose();
        ctx.request_accessibility_update();
        self.show(ctx);
    }

    /// One page along `axis`: the viewport less a line, so a line stays in view.
    fn page(&self, axis: Axis) -> f64 {
        let len = match axis {
            Axis::Horizontal => self.viewport.width,
            Axis::Vertical => self.viewport.height,
        };
        (len - SCROLL_LINE).max(SCROLL_LINE.min(len))
    }

    /// Puts the scrollers on screen and restarts the wait before they fade, if there's
    /// anything to scroll. Returns whether they weren't on screen (they need a layout).
    fn start_showing(&mut self) -> bool {
        if self.range() == Vec2::ZERO {
            return false;
        }
        self.opacity = Animated::new(1.0, Spring::STANDARD);
        self.idle = Some(Duration::ZERO);
        !std::mem::replace(&mut self.shown, true)
    }

    fn show(&mut self, ctx: &mut EventCtx<'_>) {
        if self.start_showing() {
            ctx.request_layout();
        }
        ctx.request_compose();
        ctx.request_anim_frame();
    }

    /// Applies what the scrollers asked for during this event.
    fn take_requests(&mut self, ctx: &mut EventCtx<'_>) {
        let range = self.range();
        for (i, axis) in [(0, Axis::Vertical), (1, Axis::Horizontal)] {
            let (scroller, _) = ctx.get_raw_mut(&mut self.scrollers[i]);
            let Some(request) = scroller.request.take() else {
                continue;
            };
            let target = match request {
                Request::To(position) => position * get(range, axis),
                Request::Page(sign) => get(self.offset, axis) + sign * self.page(axis),
            };
            let offset = match axis {
                Axis::Vertical => Vec2::new(self.offset.x, target),
                Axis::Horizontal => Vec2::new(target, self.offset.y),
            };
            if self.move_to(offset) {
                ctx.request_compose();
                ctx.request_accessibility_update();
            }
            self.show(ctx);
            ctx.set_handled();
        }
    }

    /// Whether the pointer is on a scroller or dragging one: the scrollers stay.
    fn scroller_held(&mut self, ctx: &mut UpdateCtx<'_>) -> bool {
        self.scrollers.iter_mut().any(|pod| ctx.get_raw(pod).0.held)
    }

    /// Scrolls so `target` (content coordinates) shows. Returns whether it moved.
    fn reveal(&mut self, target: Rect) -> bool {
        self.pending = Some(target);
        self.reveal_now(target)
    }

    fn reveal_now(&mut self, target: Rect) -> bool {
        let x = reveal(self.offset.x, self.viewport.width, (target.x0, target.x1));
        let y = reveal(self.offset.y, self.viewport.height, (target.y0, target.y1));
        self.move_to(Vec2::new(x, y))
    }

    pub(super) fn edit_offset(this: &mut WidgetMut<'_, Self>, offset: Vec2) {
        if this.widget.move_to(offset) {
            this.ctx.request_compose();
            this.ctx.request_accessibility_update();
        }
    }

    pub(super) fn edit_reveal(this: &mut WidgetMut<'_, Self>, target: Rect) {
        if this.widget.reveal(target) {
            this.ctx.request_compose();
            this.ctx.request_accessibility_update();
        }
    }

    pub(crate) fn offset(&self) -> Vec2 {
        self.offset
    }

    /// Whether the scrollers are on screen, and their opacity.
    #[cfg(test)]
    pub(crate) fn scrollers(&self) -> (bool, f64) {
        (self.shown, self.opacity.value())
    }

    #[cfg(test)]
    pub(crate) fn content_id(&self) -> masonry_core::core::WidgetId {
        self.content.id()
    }

    /// Runs `f` on the rows of a virtual list (nothing for other content).
    pub(super) fn with_rows<R>(
        this: &mut WidgetMut<'_, Self>,
        f: impl FnOnce(&mut WidgetMut<'_, RowsWidget>) -> R,
    ) -> Option<R> {
        if !this.widget.rows {
            return None;
        }
        // The rows' window must be told again after they change.
        this.widget.told = None;
        this.ctx.request_compose();
        let mut child = this.ctx.get_mut(&mut this.widget.content);
        Some(f(&mut child.downcast()))
    }
}

impl UsesProperty<ThemeProp> for ScrollWidget {}

impl Widget for ScrollWidget {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Scroll(PointerScrollEvent { delta, .. }) = event {
            let scale = ctx.scale_factor();
            let line = PhysicalPosition::new(SCROLL_LINE * scale, SCROLL_LINE * scale);
            let page =
                PhysicalPosition::new(self.viewport.width * scale, self.viewport.height * scale);
            let LogicalPosition { x, y } =
                delta.to_pixel_delta(line, page).to_logical::<f64>(scale);
            let mut delta = -Vec2::new(x, y);
            // A wheel that only turns up and down scrolls a horizontal view sideways.
            if self.axes == ScrollAxes::Horizontal && delta.x == 0.0 {
                delta.x = delta.y;
            }
            self.scroll_by(ctx, delta);
        }
        self.take_requests(ctx);
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if key.state.is_down() => {
                let own = ctx.target() == ctx.widget_id();
                let shift = key.modifiers.shift();
                let (v, h) = (Axis::Vertical, Axis::Horizontal);
                let delta = match &key.key {
                    Key::Named(NamedKey::ArrowUp) => Vec2::new(0.0, -SCROLL_LINE),
                    Key::Named(NamedKey::ArrowDown) => Vec2::new(0.0, SCROLL_LINE),
                    Key::Named(NamedKey::ArrowLeft) => Vec2::new(-SCROLL_LINE, 0.0),
                    Key::Named(NamedKey::ArrowRight) => Vec2::new(SCROLL_LINE, 0.0),
                    Key::Named(NamedKey::PageUp) => Vec2::new(0.0, -self.page(v)),
                    Key::Named(NamedKey::PageDown) => Vec2::new(0.0, self.page(v)),
                    Key::Character(c) if own && c == " " => {
                        let page = if self.axes == ScrollAxes::Horizontal {
                            Vec2::new(self.page(h), 0.0)
                        } else {
                            Vec2::new(0.0, self.page(v))
                        };
                        if shift { -page } else { page }
                    }
                    Key::Named(NamedKey::Home) => -self.offset,
                    Key::Named(NamedKey::End) => self.range() - self.offset,
                    _ => return,
                };
                self.scroll_by(ctx, delta);
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
        if ctx.target() != ctx.widget_id() {
            return;
        }
        let page = matches!(event.data, Some(ActionData::ScrollUnit(ScrollUnit::Page)));
        let step = |axis| if page { self.page(axis) } else { SCROLL_LINE };
        let delta = match event.action {
            Action::ScrollUp => Vec2::new(0.0, -step(Axis::Vertical)),
            Action::ScrollDown => Vec2::new(0.0, step(Axis::Vertical)),
            Action::ScrollLeft => Vec2::new(-step(Axis::Horizontal), 0.0),
            Action::ScrollRight => Vec2::new(step(Axis::Horizontal), 0.0),
            Action::SetScrollOffset => match event.data {
                Some(ActionData::SetScrollOffset(p)) => Vec2::new(p.x, p.y) - self.offset,
                _ => return,
            },
            _ => return,
        };
        self.scroll_by(ctx, delta);
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        // Step a fade already under way first, so one that starts in this frame starts
        // from full opacity.
        self.opacity.on_anim_frame(ctx, &theme, interval);
        if let Some(idle) = self.idle {
            let idle = if self.scroller_held(ctx) {
                Duration::ZERO
            } else {
                idle + Duration::from_nanos(interval)
            };
            if idle >= SCROLLER_HIDE_DELAY {
                self.idle = None;
                self.opacity.animate_to(ctx, &theme, 0.0);
            } else {
                self.idle = Some(idle);
                ctx.request_anim_frame();
            }
        }
        if self.idle.is_none() && self.opacity.value() <= 0.0 && self.shown {
            self.shown = false;
            ctx.request_layout();
        }
        ctx.request_compose();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.content);
        if let Some(header) = &mut self.header {
            ctx.register_child(header);
        }
        for scroller in &mut self.scrollers {
            ctx.register_child(scroller);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::RequestPanToChild(target) => {
                // In this view's coordinates: the content sits at the viewport's origin.
                if self.reveal(*target - self.origin.to_vec2()) {
                    ctx.request_compose();
                    ctx.request_accessibility_update();
                }
            }
            Update::FocusChanged(_) | Update::DisabledChanged(_) => ctx.request_post_paint(),
            Update::ChildFocusChanged(_) => ctx.request_post_paint(),
            // The pointer coming in flashes the scrollers, to show there's more to see.
            Update::ChildHoveredChanged(true) => {
                if self.start_showing() {
                    ctx.request_layout();
                }
                ctx.request_compose();
                ctx.request_anim_frame();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            self.opacity.theme_changed(ctx);
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
        let preferred = match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        };
        if let Some(length) = preferred {
            return Length::px(length);
        }
        let border = 2.0 * self.border();
        let header = match axis {
            Axis::Vertical => self.header_height,
            Axis::Horizontal => 0.0,
        };
        if self.axes.scrolls(axis) {
            match len_req {
                LenReq::MinContent => return Length::ZERO,
                LenReq::FitContent(space) => return space,
                LenReq::MaxContent => {}
            }
        }
        // The content's length; across a scrolling axis it isn't limited.
        let cross = cross_length.filter(|_| !self.axes.scrolls(axis.cross()));
        let context = LayoutSize::maybe(axis.cross(), cross);
        let content = ctx.compute_length(&mut self.content, len_req.into(), context, axis, cross);
        Length::px(content.get() + border + header)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, outer: Size) {
        let border = self.border();
        let header_height = self.header_height;
        let inner = outer.to_rect().inset(-border);
        self.origin = Point::new(inner.x0, inner.y0 + header_height);
        let size = Size::new(inner.width(), inner.height() - header_height).max(Size::ZERO);
        let along = |axis: Axis, length: f64| {
            if self.axes.scrolls(axis) {
                LenDef::MaxContent
            } else {
                LenDef::Fixed(Length::px(length))
            }
        };
        let auto = SizeDef::new(
            along(Axis::Horizontal, size.width),
            along(Axis::Vertical, size.height),
        );
        // The content fills the view at least, so its background does.
        let content = ctx
            .compute_size(&mut self.content, auto, size.into())
            .max(size);
        ctx.run_layout(&mut self.content, content);
        ctx.place_child(&mut self.content, self.origin);
        if let Some(header) = &mut self.header {
            ctx.run_layout(header, Size::new(content.width, header_height));
            ctx.place_child(header, inner.origin());
        }
        self.content_size = content;
        self.viewport = size;
        let _ = self.move_to(self.offset);
        // Compose follows layout, and takes the new offset.
        if let Some(target) = self.pending.take() {
            self.reveal_now(target);
        }
        // A framed view keeps its content inside the well's or the table's border.
        let clip = match self.frame {
            Frame::Well => text_field_inside(outer.to_rect(), FieldShape::Square).rect(),
            Frame::Border | Frame::None | Frame::Sidebar => inner,
        };
        ctx.set_clip_path(clip);
        ctx.set_paint_insets(Insets::uniform(FOCUS_RING));

        let range = self.range();
        let both = range.x > 0.0 && range.y > 0.0;
        let corner = if both { SCROLLER_WIDTH } else { 0.0 };
        let view = Rect::from_origin_size(self.origin, size);
        let rects = [
            Rect::new(view.x1 - SCROLLER_WIDTH, view.y0, view.x1, view.y1 - corner),
            Rect::new(view.x0, view.y1 - SCROLLER_WIDTH, view.x1 - corner, view.y1),
        ];
        for (i, axis) in [(0, Axis::Vertical), (1, Axis::Horizontal)] {
            let pod = &mut self.scrollers[i];
            let visible = self.shown && get(range, axis) > 0.0;
            ctx.set_stashed(pod, !visible);
            if visible {
                ctx.run_layout(pod, rects[i].size());
                ctx.place_child(pod, rects[i].origin());
            }
        }
    }

    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        // Layout comes first in a frame, so a request not settled by now never will be.
        self.pending = None;
        ctx.set_child_scroll_translation(&mut self.content, -self.offset);
        if let Some(header) = &mut self.header {
            ctx.set_child_scroll_translation(header, Vec2::new(-self.offset.x, 0.0));
        }
        let range = self.range();
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 opacity")]
        let opacity = self.opacity.value() as f32;
        for (i, axis) in [(0, Axis::Vertical), (1, Axis::Horizontal)] {
            let content = get(self.content_size.to_vec2(), axis);
            let len = get(self.viewport.to_vec2(), axis);
            let max = get(range, axis);
            let (scroller, mut raw) = ctx.get_raw_mut(&mut self.scrollers[i]);
            let spec = Scroller {
                visible: if content > 0.0 { len / content } else { 1.0 },
                position: if max > 0.0 {
                    get(self.offset, axis) / max
                } else {
                    0.0
                },
                opacity,
                ..scroller.spec
            };
            if spec != scroller.spec {
                scroller.spec = spec;
                raw.request_paint_only();
                raw.request_accessibility_update();
            }
        }
        if self.rows {
            let window = Rect::from_origin_size(self.offset.to_point(), self.viewport);
            if self.told != Some(window) {
                self.told = Some(window);
                ctx.mutate_child_later(&mut self.content, move |mut content| {
                    RowsWidget::set_window(&mut content.downcast(), window);
                });
            }
        }
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if self.frame != Frame::Well {
            return;
        }
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        let spec = WellSpec {
            shape: FieldShape::Square,
            focused: false,
            disabled: ctx.is_disabled(),
        };
        text_field(&mut scene, size.to_rect(), spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn post_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let focused = if self.frame != Frame::None {
            ctx.has_focus_target()
        } else {
            ctx.is_focus_target()
        };
        let ring = focused
            && self.frame != Frame::Sidebar
            && ctx.is_window_focused()
            && !ctx.is_disabled();
        let border = self.frame == Frame::Border;
        if !ring && !border {
            return;
        }
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        // The border goes here, not in `paint`: the clip (inside the border) applies there.
        if border {
            table_border(&mut scene, size.to_rect(), theme.variant);
        }
        if !ring {
            self.replayer.replay(&scene, ctx.scale_factor(), painter);
            return;
        }
        focus_ring(
            &mut scene,
            RoundedRect::from_rect(size.to_rect(), 0.0),
            theme.variant,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::ScrollView
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_clips_children();
        let range = self.range();
        if range.x > 0.0 {
            node.set_scroll_x_min(0.0);
            node.set_scroll_x_max(range.x);
            node.set_scroll_x(self.offset.x);
            node.add_action(Action::ScrollLeft);
            node.add_action(Action::ScrollRight);
        }
        if range.y > 0.0 {
            node.set_scroll_y_min(0.0);
            node.set_scroll_y_max(range.y);
            node.set_scroll_y(self.offset.y);
            node.add_action(Action::ScrollUp);
            node.add_action(Action::ScrollDown);
        }
        if range != Vec2::ZERO {
            node.add_action(Action::SetScrollOffset);
        }
        node.add_child_action(Action::ScrollIntoView);
    }

    fn children_ids(&self) -> ChildrenIds {
        let mut ids = ChildrenIds::new();
        ids.push(self.content.id());
        if let Some(header) = &self.header {
            ids.push(header.id());
        }
        for scroller in &self.scrollers {
            ids.push(scroller.id());
        }
        ids
    }

    fn accepts_focus(&self) -> bool {
        self.frame == Frame::None
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("{:?}", self.offset))
    }
}

/// A view onto content larger than itself: it scrolls by mouse wheel or trackpad, by its
/// overlay scrollers, by keyboard and by assistive technology, and keeps the focused
/// widget in view.
///
/// It scrolls vertically unless [`axes`](Self::axes) says otherwise, and takes the space
/// its container gives it; in a [`crate::Stack`], give it a [`height`](Self::height)
/// (a stack gives its children their whole content height, so it would never scroll).
///
/// The scrollers appear while it scrolls and when the pointer comes in, and fade out a
/// second later. Over a scroller the knob widens; drag it, or press beside it to move a
/// page. While the view has keyboard focus (Tab), the arrow keys move a line, Page Up and
/// Page Down (and Space, Shift-Space) a page, Home and End to the ends; keys a focused
/// widget inside doesn't use reach the view too.
pub struct ScrollView<M> {
    inner: NewWidget<ScrollWidget>,
    wiring: Wiring<M>,
}

impl<M> std::fmt::Debug for ScrollView<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollView")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M> ScrollView<M> {
    /// `content`, scrolling vertically.
    pub fn new(content: impl Into<Element<M>>) -> Self {
        let mut wiring = Wiring::default();
        let widget = wiring.adopt(content.into());
        Self {
            inner: NewWidget::new(ScrollWidget::new(widget, ScrollAxes::Vertical, false)),
            wiring,
        }
    }

    /// Draws the view in a text field's well (a text view's frame).
    #[must_use]
    pub(super) fn framed(mut self) -> Self {
        self.inner.widget.frame = Frame::Well;
        self
    }

    /// The directions it scrolls in.
    #[must_use]
    pub fn axes(mut self, axes: ScrollAxes) -> Self {
        self.inner.widget.axes = axes;
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

    /// Points `handle` at this scroll view.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<ScrollView<M>> for Element<M> {
    fn from(view: ScrollView<M>) -> Self {
        Element::with_wiring(view.inner.erased(), view.wiring)
    }
}

impl<M> Handle<ScrollView<M>> {
    /// Scrolls so the content point `(x, y)` is at the view's top left (as near as the
    /// content's size allows).
    pub fn scroll_to<N>(&self, cx: &mut Cx<'_, N>, x: f64, y: f64) {
        cx.edit::<ScrollWidget, _>(self, |mut w| {
            ScrollWidget::edit_offset(&mut w, Vec2::new(x, y));
        });
    }

    /// The content point at the view's top left, if it's in an open window.
    pub fn offset<N>(&self, cx: &mut Cx<'_, N>) -> Option<(f64, f64)> {
        cx.edit::<ScrollWidget, _>(self, |w| {
            let o = w.widget.offset();
            (o.x, o.y)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::reveal;

    #[test]
    fn reveal_moves_as_little_as_it_can() {
        // A 100-long viewport at 50 shows 50..150.
        assert_eq!(reveal(50.0, 100.0, (60.0, 80.0)), 50.0, "already in view");
        assert_eq!(
            reveal(50.0, 100.0, (20.0, 40.0)),
            20.0,
            "above: to its start"
        );
        assert_eq!(
            reveal(50.0, 100.0, (160.0, 180.0)),
            80.0,
            "below: to its end"
        );
        assert_eq!(
            reveal(50.0, 100.0, (0.0, 300.0)),
            0.0,
            "too long: its start"
        );
    }
}
