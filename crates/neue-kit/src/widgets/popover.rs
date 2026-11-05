//! Popovers (task 2.23a, ADR 0056): a panel of content with an arrow pointing at the widget
//! it opened from. A Masonry layer attached to that widget (an xdg-popup on Wayland, an
//! in-window overlay on macOS).
//!
//! - **Placement.** The popover asks for one side of its anchor and lays itself out there,
//!   centred on the anchor. On Wayland the compositor may flip it to the other side or
//!   slide it along at the screen's edge; the driver then tells it where it landed
//!   ([`PopoverLanded`]) and the arrow follows, pointing back at the anchor.
//! - **Closing.** Escape, a press outside it, the window losing focus, or the compositor
//!   dismissing it close it, and the app hears about it (`on_close`). Pressing the anchor
//!   again closes it too: on Wayland the compositor dismisses it and the press never
//!   reaches the anchor; in a window, the popover remembers the press and the anchor's
//!   next [`Cx::show_popover`](crate::Cx::show_popover) closes it instead of reopening.
//! - **Focus.** The popover takes keyboard focus when it opens (Tab moves into its
//!   content) and gives it back to whatever had it when it closes.

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, ActionCtx, ChildrenIds, ErasedAction, EventCtx, Layer, LayerType, LayoutCtx,
    MeasureCtx, NewWidget, PaintCtx, PointerButtonEvent, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId,
    WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Point, Rect, Size};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{Edge, popover, popover_body, popover_landing, popover_origin};
use neue_platform::{PopoverAnchor, PopoverLanded, PopoverRequest, Side};
use neue_theme::materials::popover::{ARROW_HEIGHT, GAP, PADDING, SHADOW_REACH};

use crate::element::{Element, Wiring};
use crate::theme::ThemeProp;

/// A panel of content shown next to a widget, with an arrow pointing at it. Show it with
/// [`Cx::show_popover`](crate::Cx::show_popover), anchored to a widget you hold a
/// [`Handle`](crate::Handle) to.
///
/// ```no_run
/// # use neue_kit::{Button, Cx, Edge, Handle, Label, Popover};
/// # #[derive(Clone)] enum Msg { Info, Closed }
/// # fn update(cx: &mut Cx<'_, Msg>, button: &Handle<Button<Msg>>) {
/// cx.show_popover(
///     button,
///     Popover::new(Label::new("Saved 2 minutes ago"))
///         .edge(Edge::Bottom)
///         .on_close(Msg::Closed),
/// );
/// # }
/// ```
pub struct Popover<M> {
    content: Element<M>,
    edge: Edge,
    on_close: Option<M>,
}

impl<M> std::fmt::Debug for Popover<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Popover")
            .field("edge", &self.edge)
            .field("on_close", &self.on_close.is_some())
            .finish_non_exhaustive()
    }
}

impl<M: Clone + 'static> Popover<M> {
    /// A popover showing `content`, below its anchor unless told otherwise.
    pub fn new(content: impl Into<Element<M>>) -> Self {
        Self {
            content: content.into(),
            edge: Edge::Bottom,
            on_close: None,
        }
    }

    /// The side of the anchor it goes on when there is room (on Wayland the compositor
    /// puts it on the other side when there isn't).
    #[must_use]
    pub fn edge(mut self, edge: Edge) -> Self {
        self.edge = edge;
        self
    }

    /// Sent when the popover closes by itself: Escape, a press outside it, the window
    /// losing focus. Not sent for [`Cx::close_popover`](crate::Cx::close_popover).
    #[must_use]
    pub fn on_close(mut self, message: M) -> Self {
        self.on_close = Some(message);
        self
    }

    /// The popover's widget, still to be placed by [`show`], and the wiring of its
    /// content and of its close message.
    pub(crate) fn into_parts(self) -> (NewWidget<PopoverWidget>, Wiring<M>) {
        let mut wiring = Wiring::default();
        let content = wiring.adopt(self.content);
        if let Some(button) = wiring.default_button.take() {
            tracing::debug!(%button, "a popover's default button is not the window's");
        }
        let popover = NewWidget::new(PopoverWidget::new(content, self.edge));
        if let Some(message) = self.on_close {
            wiring.handlers.push((
                popover.id(),
                Box::new(move |action| action.is::<Closed>().then(|| message.clone())),
            ));
        }
        (popover, wiring)
    }
}

/// A popover's action: it closed by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Closed;

/// The side the driver is asked for.
fn side(edge: Edge) -> Side {
    match edge {
        Edge::Top => Side::Top,
        Edge::Bottom => Side::Bottom,
        Edge::Left => Side::Left,
        Edge::Right => Side::Right,
    }
}

/// Opens `popover` next to the widget `owner`, replacing one it already has.
pub(crate) fn show(owner: &mut WidgetMut<'_, dyn Widget>, mut popover: NewWidget<PopoverWidget>) {
    let ctx = &mut owner.ctx;
    let anchor = ctx.window_transform().transform_rect_bbox(ctx.border_box());
    let w = &mut popover.widget;
    w.anchor = anchor;
    w.owner = Some(ctx.widget_id());
    w.return_focus = ctx.focus_target_id();
    w.origin = popover_origin(anchor, w.edge, Size::ZERO);
    let origin = w.origin;
    let request = PopoverRequest {
        anchor: anchor.inflate(GAP, GAP),
        side: side(w.edge),
        reach: SHADOW_REACH,
    };
    let popover = popover.with_props(PopoverAnchor(Some(request)));
    ctx.create_attached_layer(LayerType::Other, popover, origin);
}

/// The popover open on the widget `owner`, if any.
pub(crate) fn shown(owner: &WidgetMut<'_, dyn Widget>) -> Option<WidgetId> {
    owner.ctx.get_attached_layer::<PopoverWidget>()
}

/// The Masonry widget of an open popover: the panel, its content, and a probe that tells
/// it when it is in the tree.
pub(crate) struct PopoverWidget {
    content: WidgetPod<dyn Widget>,
    probe: WidgetPod<ReadyProbe>,
    /// The widget it points at, in window coordinates, when it opened.
    anchor: Rect,
    /// The side of the anchor it asked for.
    edge: Edge,
    /// The widget it is attached to.
    owner: Option<WidgetId>,
    /// What had keyboard focus when it opened.
    return_focus: Option<WidgetId>,
    /// The layer's position, as last asked for or as landed.
    origin: Point,
    /// The driver placed it ([`PopoverLanded`]): it stays where it is.
    landed: bool,
    /// The side of the anchor it is on, and where its arrow's tip is along the facing
    /// side.
    placed: (Edge, f64),
    /// The last press outside it was on its anchor (see the module docs).
    anchor_pressed: bool,
    /// On its way out: close only once.
    closing: bool,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for PopoverWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopoverWidget")
            .field("edge", &self.edge)
            .field("placed", &self.placed)
            .field("landed", &self.landed)
            .finish_non_exhaustive()
    }
}

impl PopoverWidget {
    fn new(content: NewWidget<dyn Widget>, edge: Edge) -> Self {
        Self {
            content: content.to_pod(),
            probe: NewWidget::new(ReadyProbe).to_pod(),
            anchor: Rect::ZERO,
            edge,
            owner: None,
            return_focus: None,
            origin: Point::ORIGIN,
            landed: false,
            placed: (edge, 0.0),
            anchor_pressed: false,
            closing: false,
            replayer: ImagingReplayer::new(),
        }
    }

    /// The side of the anchor it is on.
    #[cfg(test)]
    pub(crate) fn placed_edge(&self) -> Edge {
        self.placed.0
    }

    /// Where its arrow's tip is along the side facing the anchor.
    #[cfg(test)]
    pub(crate) fn arrow_at(&self) -> f64 {
        self.placed.1
    }

    /// Whether the press that closed it, if any, was on the anchor; clears that.
    pub(crate) fn take_anchor_press(this: &mut WidgetMut<'_, Self>) -> bool {
        std::mem::take(&mut this.widget.anchor_pressed)
    }

    /// Closes it from outside an event (the app, or its anchor pressed again). `notify`:
    /// the app hears about it. Returns where keyboard focus goes back to, if it was in the
    /// popover.
    pub(crate) fn close(this: &mut WidgetMut<'_, Self>, notify: bool) -> Option<WidgetId> {
        if std::mem::replace(&mut this.widget.closing, true) {
            return None;
        }
        if notify {
            this.ctx.submit_action::<Closed>(Closed);
        }
        if let Some(owner) = this.widget.owner {
            let me = this.ctx.widget_id();
            this.ctx.mutate_later(owner, move |owner| remove(owner, me));
        }
        this.ctx
            .has_focus_target()
            .then_some(this.widget.return_focus)
            .flatten()
    }

    /// Closes it from one of its events, telling the app.
    fn dismiss(&mut self, ctx: &mut EventCtx<'_>) {
        if std::mem::replace(&mut self.closing, true) {
            return;
        }
        if ctx.has_focus_target()
            && let Some(focus) = self.return_focus
        {
            ctx.set_focus(focus);
        }
        ctx.submit_action::<Closed>(Closed);
        if let Some(owner) = self.owner {
            let me = ctx.widget_id();
            ctx.mutate_later(owner, move |owner| remove(owner, me));
        }
    }

    /// The side and arrow for a box of `size` at the layer's position.
    fn place(&mut self, size: Size) {
        self.placed = popover_landing(self.anchor, self.edge, size, self.origin);
    }

    /// The content's extra room on `axis`: padding, and the arrow's strip across it.
    fn chrome(&self, axis: Axis) -> f64 {
        let arrow = match (axis, self.edge.is_horizontal()) {
            (Axis::Vertical, true) | (Axis::Horizontal, false) => ARROW_HEIGHT,
            _ => 0.0,
        };
        2.0 * PADDING + arrow
    }
}

/// Removes the popover `me` from `owner`, unless a newer one replaced it.
fn remove(mut owner: WidgetMut<'_, dyn Widget>, me: WidgetId) {
    if owner.ctx.get_attached_layer::<PopoverWidget>() == Some(me) {
        owner.ctx.remove_layer(me);
    }
}

impl Layer for PopoverWidget {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let p = ctx.local_position(state.position);
                let inside =
                    Rect::from_origin_size(Point::ORIGIN, ctx.border_box().size()).contains(p);
                self.anchor_pressed = false;
                if inside {
                    return;
                }
                if self.anchor.contains(ctx.to_window(p)) {
                    // The anchor's own press decides: showing it again closes it.
                    self.anchor_pressed = true;
                } else if ctx.has_focus_target() || ctx.is_focus_target() {
                    // With focus elsewhere (a menu opened from the content), a press
                    // outside is that menu's business.
                    self.dismiss(ctx);
                }
            }
            PointerEvent::Cancel(..) => self.dismiss(ctx),
            _ => {}
        }
    }
}

impl UsesProperty<ThemeProp> for PopoverWidget {}
impl UsesProperty<PopoverLanded> for PopoverWidget {}

impl Widget for PopoverWidget {
    type Action = Closed;

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key)
                if key.state.is_down() && key.key == Key::Named(NamedKey::Escape) =>
            {
                ctx.set_handled();
                self.dismiss(ctx);
            }
            TextEvent::WindowFocusChange(false) => self.dismiss(ctx),
            _ => {}
        }
    }

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if source == self.probe.id() && action.is::<Ready>() {
            ctx.set_handled();
            ctx.request_focus();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.content);
        ctx.register_child(&mut self.probe);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::FocusChanged(_) | Update::ChildFocusChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_paint_only();
        } else if property_type == TypeId::of::<PopoverLanded>() {
            ctx.request_layout();
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
        let inward = |length: Length, axis| Length::px((length.get() - self.chrome(axis)).max(0.0));
        let auto = match len_req {
            LenReq::MinContent => LenDef::MinContent,
            LenReq::MaxContent => LenDef::MaxContent,
            LenReq::FitContent(space) => LenDef::FitContent(inward(space, axis)),
        };
        let cross = cross_length.map(|c| inward(c, axis.cross()));
        let context = LayoutSize::maybe(axis.cross(), cross);
        let inner = ctx.compute_length(&mut self.content, auto, context, axis, cross);
        Length::px(inner.get() + self.chrome(axis))
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, props: &PropertiesRef<'_>, size: Size) {
        if let PopoverLanded(Some(at)) = *props.get::<PopoverLanded>(ctx.property_cache()) {
            self.landed = true;
            self.origin = at;
        }
        if !self.landed {
            // Centred on the anchor, now that the size is known. A driver that places the
            // popover itself moves it again once the compositor has.
            let origin = popover_origin(self.anchor, self.edge, size);
            if origin != self.origin {
                self.origin = origin;
                ctx.reposition_layer(ctx.widget_id(), origin);
            }
        }
        self.place(size);

        let body = popover_body(size, self.placed.0.opposite());
        let content = body.inflate(-PADDING, -PADDING);
        let content_size = Size::new(content.width().max(0.0), content.height().max(0.0));
        ctx.run_layout(&mut self.content, content_size);
        ctx.place_child(&mut self.content, content.origin());
        ctx.run_layout(&mut self.probe, Size::ZERO);
        ctx.place_child(&mut self.probe, Point::ORIGIN);
        ctx.clear_baselines();
        ctx.set_paint_insets(Insets::uniform(SHADOW_REACH));
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
        let (edge, at) = self.placed;
        popover(&mut scene, size, edge.opposite(), at, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Dialog
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        [self.content.id(), self.probe.id()].into_iter().collect()
    }

    fn accepts_focus(&self) -> bool {
        // Focus comes by request only (on opening); Tab never lands here.
        false
    }

    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}

/// The probe's action: it is in the tree, so the popover can take focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ready;

/// An empty child that reports [`Ready`] once it is in the tree (a layer's root can't take
/// focus from `update`, only from an action or event).
#[derive(Debug)]
pub(crate) struct ReadyProbe;

impl Widget for ReadyProbe {
    type Action = Ready;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::WidgetAdded = event {
            ctx.submit_action::<Ready>(Ready);
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
        Length::ZERO
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
