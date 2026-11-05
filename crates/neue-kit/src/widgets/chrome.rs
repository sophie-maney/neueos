//! Client-side window chrome (ADR 0055): the title bar, or a unified title bar and toolbar
//! under one gradient, that a window with [`crate::Window::toolbar`] or
//! [`crate::Window::chrome`] draws itself.
//!
//! The window root owns a [`Chrome`] and hands it pointer, access and focus events; the
//! chrome draws with `neue_paint::materials::window_chrome`, the same code as the
//! compositor's server-side bar. The traffic lights show their glyphs while the pointer is
//! over any of them, darken while pressed, and act when released over the button they were
//! pressed on. Dragging the bar moves the window, and a double-click zooms it. While the
//! window is inactive, the bar, the lights and the title take their unfocused look.
//!
//! A HUD window ([`crate::Window::hud`]) has the other kind of chrome: the dark HUD panel
//! fills the whole window, with a small title bar that has a close button and a title
//! and nothing else. It looks the same whether the window is focused or not.

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesRef,
    RegisterCtx, Widget, WidgetId, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Point, Rect, Size};
use masonry_core::layout::{LenDef, LenReq, Length, SizeDef};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::hud::{CloseState, HUD_TITLE_HEIGHT, close_rect, hud, hud_close_button};
use neue_paint::materials::title_bar::{
    MIN_WIDTH, TITLE_BAR_HEIGHT, TitleBarStyle, TrafficLight, title_max_width, title_x,
    window_chrome,
};
use neue_paint::{Backdrop, Scene};
use neue_theme::materials::hud::{
    Palette as HudPalette, TITLE_BASELINE as HUD_TITLE_BASELINE, TITLE_INSET as HUD_TITLE_INSET,
};
use neue_theme::materials::title_bar::{
    BarPalette, TITLE_BASELINE, TITLE_EMBOSS_OFFSET, TITLE_FONT_SIZE, TOOLBAR_BOTTOM, TOOLBAR_INSET,
};
use neue_theme::{Theme, typography};

use super::root::RootAction;
use super::text::{TextLine, TextStyle};

// The kit draws the title with Masonry text in the body style; it must match the size the
// compositor's bar uses.
const _: () = assert!(TITLE_FONT_SIZE == typography::BODY);

/// What the chrome asks of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowRequest {
    /// Follow the pointer until the button is released (the bar was pressed).
    Move,
    /// Maximise or restore (zoom button, or a double-click on the bar).
    Zoom,
    Minimize,
    /// The app's close request (the close button).
    Close,
}

impl From<TrafficLight> for WindowRequest {
    fn from(light: TrafficLight) -> Self {
        match light {
            TrafficLight::Close => Self::Close,
            TrafficLight::Minimize => Self::Minimize,
            TrafficLight::Zoom => Self::Zoom,
        }
    }
}

/// Which chrome a window draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChromeKind {
    /// The title bar, with the three traffic lights and perhaps a toolbar.
    #[default]
    Standard,
    /// The dark HUD panel with a small title bar and a close button.
    Hud,
}

/// The narrowest a HUD window gets: room for the close button and a short title.
const HUD_MIN_WIDTH: f64 = 80.0;

impl ChromeKind {
    /// The buttons in the title bar.
    fn buttons(self) -> &'static [TrafficLight] {
        match self {
            Self::Standard => &TrafficLight::ALL,
            Self::Hud => &[TrafficLight::Close],
        }
    }

    fn rect(self, light: TrafficLight) -> Rect {
        match self {
            Self::Standard => light.rect(),
            Self::Hud => close_rect(),
        }
    }

    /// Height of the title row.
    fn title_height(self) -> f64 {
        match self {
            Self::Standard => TITLE_BAR_HEIGHT,
            Self::Hud => HUD_TITLE_HEIGHT,
        }
    }

    fn min_width(self) -> f64 {
        match self {
            Self::Standard => MIN_WIDTH,
            Self::Hud => HUD_MIN_WIDTH,
        }
    }

    fn title_style(self) -> TextStyle {
        match self {
            Self::Standard => TextStyle::Body,
            Self::Hud => TextStyle::Small,
        }
    }

    /// The buttons together: hovering anywhere in here shows every glyph.
    fn buttons_rect(self) -> Rect {
        self.buttons()
            .iter()
            .map(|&light| self.rect(light))
            .reduce(|a, b| a.union(b))
            .unwrap_or_default()
    }

    fn button_at(self, pos: Point) -> Option<TrafficLight> {
        self.buttons()
            .iter()
            .copied()
            .find(|&light| self.rect(light).contains(pos))
    }
}

/// A window's chrome, owned by its root widget.
#[derive(Debug)]
pub(crate) struct Chrome {
    kind: ChromeKind,
    title: TextLine,
    toolbar: Option<WidgetPod<dyn Widget>>,
    /// AccessKit buttons over the lights, which the chrome paints itself.
    lights: Vec<(TrafficLight, WidgetPod<TrafficLightWidget>)>,
    /// Paint the focused look even when the window isn't focused: a HUD window that belongs
    /// to this one has the keyboard.
    keep_active: bool,
    /// What the HUD material draws: translucent when the compositor blurs behind the window.
    backdrop: Backdrop,
    /// Height of the bar, from the last layout.
    height: f64,
    /// The pointer is over the traffic lights.
    hover: bool,
    /// The light the pointer went down on, and whether the pointer is still over it.
    pressed: Option<(TrafficLight, bool)>,
    replayer: ImagingReplayer,
    /// Every request made, for tests: the test harness drops window signals.
    #[cfg(test)]
    pub(crate) requests: Vec<WindowRequest>,
}

impl Chrome {
    /// A HUD window has no toolbar: `toolbar` is dropped.
    pub(crate) fn new(
        kind: ChromeKind,
        title: String,
        toolbar: Option<NewWidget<dyn Widget>>,
    ) -> Self {
        let toolbar = toolbar.filter(|_| kind == ChromeKind::Standard);
        Self {
            kind,
            title: TextLine::new(title, kind.title_style()),
            toolbar: toolbar.map(NewWidget::to_pod),
            lights: kind
                .buttons()
                .iter()
                .map(|&light| {
                    let size = kind.rect(light).size();
                    (
                        light,
                        NewWidget::new(TrafficLightWidget(light, size)).to_pod(),
                    )
                })
                .collect(),
            keep_active: false,
            backdrop: Backdrop::Opaque,
            height: kind.title_height(),
            hover: false,
            pressed: None,
            replayer: ImagingReplayer::new(),
            #[cfg(test)]
            requests: Vec::new(),
        }
    }

    /// A HUD paints `backdrop` (the translucent tint when the compositor blurs behind it).
    #[must_use]
    pub(crate) fn with_backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = backdrop;
        self
    }

    pub(crate) fn is_hud(&self) -> bool {
        self.kind == ChromeKind::Hud
    }

    pub(crate) fn set_keep_active(&mut self, keep: bool) -> bool {
        std::mem::replace(&mut self.keep_active, keep) != keep
    }

    /// Height of the bar (title row plus toolbar), as last laid out.
    pub(crate) fn height(&self) -> f64 {
        self.height
    }

    /// How the bar looks in a window that is `focused` or not.
    pub(crate) fn style(&self, theme: Theme, focused: bool) -> TitleBarStyle {
        TitleBarStyle {
            variant: theme.variant,
            focused,
            hover: self.hover || self.pressed.is_some(),
            pressed: self.pressed.and_then(|(light, over)| over.then_some(light)),
        }
    }

    fn request(&mut self, ctx: &mut EventCtx<'_>, request: WindowRequest) {
        tracing::trace!(?request, "window chrome");
        #[cfg(test)]
        self.requests.push(request);
        match request {
            WindowRequest::Move => ctx.drag_window(),
            WindowRequest::Zoom => ctx.toggle_maximized(),
            WindowRequest::Minimize => ctx.minimize(),
            WindowRequest::Close => ctx.submit_action::<RootAction>(RootAction::Close),
        }
    }

    fn set_hover(&mut self, ctx: &mut EventCtx<'_>, hover: bool) {
        if self.hover != hover {
            self.hover = hover;
            ctx.request_paint_only();
        }
    }

    /// A pointer event that reached the window root: on the root itself, or bubbled up from
    /// a widget that left it unhandled.
    pub(crate) fn on_pointer_event(&mut self, ctx: &mut EventCtx<'_>, event: &PointerEvent) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                // A toolbar control that captured the pointer owns this press; many don't
                // mark it handled.
                let taken = ctx
                    .pointer_capture_target_id()
                    .is_some_and(|id| id != ctx.widget_id());
                if pos.y >= self.height || taken {
                    return;
                }
                ctx.set_handled();
                if let Some(light) = self.kind.button_at(pos) {
                    self.pressed = Some((light, true));
                    ctx.capture_pointer();
                    ctx.request_paint_only();
                } else if state.count == 2 && self.kind == ChromeKind::Standard {
                    // A HUD panel has no zoom button, so double-clicking its bar doesn't zoom.
                    self.request(ctx, WindowRequest::Zoom);
                } else {
                    self.request(ctx, WindowRequest::Move);
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let pos = ctx.local_position(current.position);
                if let Some((light, over)) = self.pressed {
                    let now = self.kind.rect(light).contains(pos);
                    if now != over {
                        self.pressed = Some((light, now));
                        ctx.request_paint_only();
                    }
                }
                self.set_hover(ctx, self.kind.buttons_rect().contains(pos));
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                if let Some((light, _)) = self.pressed.take() {
                    ctx.set_handled();
                    ctx.request_paint_only();
                    let pos = ctx.local_position(state.position);
                    if self.kind.rect(light).contains(pos) {
                        self.request(ctx, light.into());
                    }
                    self.set_hover(ctx, self.kind.buttons_rect().contains(pos));
                }
            }
            PointerEvent::Cancel(..) | PointerEvent::Leave(..) => {
                if self.pressed.take().is_some() {
                    ctx.request_paint_only();
                }
                self.set_hover(ctx, false);
            }
            _ => {}
        }
    }

    /// An access event that reached the window root: a click on a traffic light acts.
    pub(crate) fn on_access_event(&mut self, ctx: &mut EventCtx<'_>, event: &AccessEvent) {
        if event.action != Action::Click {
            return;
        }
        let target = ctx.target();
        let light = self
            .lights
            .iter()
            .find_map(|(light, pod)| (pod.id() == target).then_some(*light));
        if let Some(light) = light {
            ctx.set_handled();
            self.request(ctx, light.into());
        }
    }

    /// The pointer left the window root and its children.
    pub(crate) fn unhover(&mut self) -> bool {
        std::mem::replace(&mut self.hover, false)
    }

    pub(crate) fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for (_, light) in &mut self.lights {
            ctx.register_child(light);
        }
        if let Some(toolbar) = &mut self.toolbar {
            ctx.register_child(toolbar);
        }
    }

    pub(crate) fn children_ids(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.lights
            .iter()
            .map(|(_, pod)| pod.id())
            .chain(self.toolbar.as_ref().map(WidgetPod::id))
    }

    /// The chrome's length along `axis`: at least [`MIN_WIDTH`] wide, and as tall as the
    /// title row plus the toolbar.
    pub(crate) fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let Some(toolbar) = &mut self.toolbar else {
            return Length::px(match axis {
                Axis::Horizontal => self.kind.min_width(),
                Axis::Vertical => self.kind.title_height(),
            });
        };
        let inset = Length::px(2.0 * TOOLBAR_INSET);
        let cross = match axis {
            Axis::Horizontal => cross_length,
            Axis::Vertical => cross_length.map(|c| c.saturating_sub(inset)),
        };
        let context = masonry_core::layout::LayoutSize::maybe(axis.cross(), cross);
        let length = ctx.compute_length(toolbar, len_req.into(), context, axis, cross);
        match axis {
            Axis::Horizontal => length.saturating_add(inset).max(Length::px(MIN_WIDTH)),
            Axis::Vertical => length.saturating_add(Length::px(TITLE_BAR_HEIGHT + TOOLBAR_BOTTOM)),
        }
    }

    /// Lays the chrome out across a window `width` wide; returns its height.
    pub(crate) fn layout(&mut self, ctx: &mut LayoutCtx<'_>, width: f64, height: f64) -> f64 {
        self.title.size(ctx.text_contexts());
        for (light, pod) in &mut self.lights {
            let rect = self.kind.rect(*light);
            ctx.run_layout(pod, rect.size());
            ctx.place_child(pod, rect.origin());
        }
        self.height = self.kind.title_height();
        if let Some(toolbar) = &mut self.toolbar {
            let inner = Size::new((width - 2.0 * TOOLBAR_INSET).max(0.0), height);
            let def = SizeDef::new(LenDef::Fixed(Length::px(inner.width)), LenDef::MaxContent);
            let size = ctx.compute_size(toolbar, def, inner.into());
            let size = Size::new(inner.width, size.height);
            ctx.run_layout(toolbar, size);
            ctx.place_child(toolbar, Point::new(TOOLBAR_INSET, TITLE_BAR_HEIGHT));
            self.height += size.height + TOOLBAR_BOTTOM;
        }
        self.height = self.height.min(height.max(self.kind.title_height()));
        self.height
    }

    /// Paints the chrome across a window `size` big: the bar and the title, or for a HUD
    /// window the whole panel. `focused` is whether the window has the keyboard.
    pub(crate) fn paint(
        &mut self,
        painter: &mut Painter<'_>,
        theme: Theme,
        focused: bool,
        size: Size,
        scale: f64,
    ) {
        match self.kind {
            ChromeKind::Standard => self.paint_standard(painter, theme, focused, size.width, scale),
            ChromeKind::Hud => self.paint_hud(painter, theme, size, scale),
        }
    }

    fn paint_standard(
        &mut self,
        painter: &mut Painter<'_>,
        theme: Theme,
        focused: bool,
        width: f64,
        scale: f64,
    ) {
        let focused = focused || self.keep_active;
        let style = self.style(theme, focused);
        let mut scene = Scene::new(width.max(1.0), self.height);
        window_chrome(&mut scene, width, self.height, style);
        self.replayer.replay(&scene, scale, painter);

        let palette = BarPalette::new(theme.variant, focused);
        let text_width = self.title.width();
        let x = title_x(width.max(MIN_WIDTH), text_width);
        let top = TITLE_BASELINE - self.title.baseline();
        let room = title_max_width(width.max(MIN_WIDTH));
        let clip = Rect::new(x, 0.0, x + room + TITLE_EMBOSS_OFFSET, TITLE_BAR_HEIGHT);
        painter.push_fill_clip(clip);
        let shadow = Affine::translate((x, top + TITLE_EMBOSS_OFFSET));
        self.title.paint(painter, shadow, palette.text_shadow);
        self.title
            .paint(painter, Affine::translate((x, top)), palette.text);
        painter.pop_clip();
    }

    /// The HUD panel over the whole window, its close button and its title: the
    /// translucent tint with a blur behind it, else the opaque one (ADR 0039).
    fn paint_hud(&mut self, painter: &mut Painter<'_>, theme: Theme, size: Size, scale: f64) {
        let mut scene = Scene::new(size.width.max(1.0), size.height.max(1.0));
        hud(
            &mut scene,
            Rect::new(0.0, 0.0, size.width, size.height),
            theme.variant,
            self.backdrop,
        );
        hud_close_button(
            &mut scene,
            theme.variant,
            CloseState {
                hover: self.hover || self.pressed.is_some(),
                pressed: self.pressed.is_some_and(|(_, over)| over),
            },
        );
        self.replayer.replay(&scene, scale, painter);

        let palette = HudPalette::new(theme.variant, self.backdrop);
        let room = (size.width - 2.0 * HUD_TITLE_INSET).max(0.0);
        let x = HUD_TITLE_INSET + (room - self.title.width()).max(0.0) / 2.0;
        let top = HUD_TITLE_BASELINE - self.title.baseline();
        painter.push_fill_clip(Rect::new(
            HUD_TITLE_INSET,
            0.0,
            HUD_TITLE_INSET + room,
            HUD_TITLE_HEIGHT,
        ));
        self.title
            .paint(painter, Affine::translate((x, top)), palette.label);
        painter.pop_clip();
    }

    /// The window's title, for its accessibility node.
    pub(crate) fn title(&self) -> &str {
        self.title.text()
    }
}

/// A traffic light as AccessKit sees it: a button the chrome paints and handles. It takes
/// no pointer input, so the pointer reaches the chrome under it.
#[derive(Debug)]
pub(crate) struct TrafficLightWidget(TrafficLight, Size);

impl Widget for TrafficLightWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let size = self.1;
        Length::px(match axis {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        })
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(match self.0 {
            TrafficLight::Close => "Close",
            TrafficLight::Minimize => "Minimise",
            TrafficLight::Zoom => "Zoom",
        });
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("{:?}", self.0))
    }
}
