//! The stepper: small up and down arrows that raise or lower a number by one step. It
//! shows no number itself; the app puts one beside it (a text field, from 2.14, or a label).

use std::any::TypeId;
use std::ops::RangeInclusive;

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Rect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{Stepper as StepperSpec, StepperPart, stepper, stepper_part_at};
use neue_theme::metrics::control;

use super::slider::{Changed, Range};
use crate::Cx;
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// The Masonry widget behind [`Stepper`].
#[derive(Debug)]
pub(crate) struct StepperWidget {
    range: Range,
    value: f64,
    replayer: ImagingReplayer,
    /// The half the pointer went down on, until it comes up.
    pressed: Option<StepperPart>,
    /// Size from the last layout.
    size: Size,
}

impl StepperWidget {
    fn new() -> Self {
        Self {
            range: Range::new(0.0, 100.0, 1.0),
            value: 0.0,
            replayer: ImagingReplayer::new(),
            pressed: None,
            size: Size::ZERO,
        }
    }

    fn rect(&self) -> Rect {
        Rect::from_origin_size((0.0, 0.0), self.size)
    }

    /// Sets the value from the app: no message goes back.
    pub(crate) fn edit_value(this: &mut WidgetMut<'_, Self>, value: f64) {
        this.widget.value = this.widget.range.clamp(value);
        this.ctx.request_accessibility_update();
    }

    /// The user stepped: show the new value and tell the app, unless it is at the end.
    fn step(&mut self, ctx: &mut EventCtx<'_>, part: StepperPart) {
        let step = self.range.key_step();
        let target = match part {
            StepperPart::Up => self.value + step,
            StepperPart::Down => self.value - step,
        };
        let value = self.range.clamp(target);
        if value != self.value {
            self.value = value;
            ctx.submit_action::<Changed>(Changed(value));
            ctx.request_accessibility_update();
        }
    }
}

impl UsesProperty<ThemeProp> for StepperWidget {}

impl Widget for StepperWidget {
    type Action = Changed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let y = ctx.local_position(state.position).y;
                self.pressed = Some(stepper_part_at(self.rect(), y));
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let y = ctx.local_position(state.position).y;
                let pressed = self.pressed.take();
                if ctx.is_active()
                    && ctx.is_hovered()
                    && let Some(part) = pressed.filter(|&p| stepper_part_at(self.rect(), y) == p)
                {
                    self.step(ctx, part);
                }
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) => {
                self.pressed = None;
                ctx.request_paint_only();
            }
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
            TextEvent::Keyboard(key) if key.state.is_down() => {
                match key.key {
                    Key::Named(NamedKey::ArrowUp) => self.step(ctx, StepperPart::Up),
                    Key::Named(NamedKey::ArrowDown) => self.step(ctx, StepperPart::Down),
                    _ => return,
                }
                ctx.set_handled();
            }
            TextEvent::WindowFocusChange(_) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match event.action {
            Action::Increment => self.step(ctx, StepperPart::Up),
            Action::Decrement => self.step(ctx, StepperPart::Down),
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_)
            | Update::HoveredChanged(_)
            | Update::ActiveChanged(_)
            | Update::FocusChanged(_) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(match axis {
            Axis::Horizontal => control::STEPPER_WIDTH,
            Axis::Vertical => control::STEPPER_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        ctx.set_paint_insets(Insets::uniform(control::FOCUS_RING + control::RIM));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let spec = StepperSpec {
            pressed: self.pressed.filter(|_| ctx.is_active() && ctx.is_hovered()),
            disabled: ctx.is_disabled(),
            focused: ctx.is_focus_target() && ctx.is_window_focused(),
        };
        let mut scene = Scene::new(size.width, size.height);
        stepper(
            &mut scene,
            Rect::from_origin_size((0.0, 0.0), size),
            spec,
            theme.variant,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::SpinButton
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.range.min);
        node.set_max_numeric_value(self.range.max);
        node.set_numeric_value_step(self.range.key_step());
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.value.to_string())
    }
}

/// A stepper: the up half adds one step to its value, the down half subtracts one, and
/// Up and Down do the same while it has keyboard focus. Sends the new value each time it
/// changes; at either end of the range it sends nothing. It shows no number: the app
/// shows `value` beside it.
///
/// The range is 0 to 100 in steps of 1, the value 0, until set otherwise. Holding a half
/// doesn't repeat.
pub struct Stepper<M> {
    inner: NewWidget<StepperWidget>,
    on_change: Option<Box<dyn FnMut(f64) -> M>>,
}

impl<M> std::fmt::Debug for Stepper<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stepper")
            .field("inner", &self.inner)
            .field("on_change", &self.on_change.is_some())
            .finish()
    }
}

impl<M: 'static> Default for Stepper<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: 'static> Stepper<M> {
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(StepperWidget::new()),
            on_change: None,
        }
    }

    /// The values the stepper covers, `min` to `max` (the start and end of `range`).
    #[must_use]
    pub fn range(mut self, range: RangeInclusive<f64>) -> Self {
        let w = &mut self.inner.widget;
        w.range = Range::new(*range.start(), *range.end(), w.range.step);
        w.value = w.range.clamp(w.value);
        self
    }

    /// How much each step adds or subtracts. 1 by default; a step of 0 or less counts as
    /// 1.
    #[must_use]
    pub fn step(mut self, step: f64) -> Self {
        let w = &mut self.inner.widget;
        let step = if step > 0.0 { step } else { 1.0 };
        w.range = Range::new(w.range.min, w.range.max, step);
        w.value = w.range.clamp(w.value);
        self
    }

    /// Starts at `value` (kept inside the range, on a step).
    #[must_use]
    pub fn value(mut self, value: f64) -> Self {
        let w = &mut self.inner.widget;
        w.value = w.range.clamp(value);
        self
    }

    /// Sends the message `make` returns, given the new value, each time the user steps.
    /// Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_change(mut self, make: impl FnMut(f64) -> M + 'static) -> Self {
        self.on_change = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this stepper.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<Stepper<M>> for Element<M> {
    fn from(stepper: Stepper<M>) -> Self {
        let id = stepper.inner.id();
        let mut element = Element::new(stepper.inner.erased());
        if let Some(mut make) = stepper.on_change {
            element.on_action(id, move |action| {
                let Changed(value) = *action.downcast_ref::<Changed>()?;
                Some(make(value))
            });
        }
        element
    }
}

impl<M> Handle<Stepper<M>> {
    /// Sets the value the stepper steps from (kept inside the range, on a step), for when
    /// the app changes the number some other way, for example by typing.
    pub fn set_value<N>(&self, cx: &mut Cx<'_, N>, value: f64) {
        cx.edit::<StepperWidget, _>(self, |mut w| StepperWidget::edit_value(&mut w, value));
    }

    /// Enables or disables the stepper.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<StepperWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
