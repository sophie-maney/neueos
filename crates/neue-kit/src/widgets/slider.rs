//! The slider: a knob on a groove. Click or drag to set the value; the arrow keys, Page Up
//! and Page Down, Home and End move it while the slider has keyboard focus.

use std::any::TypeId;
use std::ops::RangeInclusive;

use masonry_core::accesskit::{Action, ActionData, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, UsesProperty, Widget,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Rect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{Slider as SliderSpec, slider, slider_value_at};
use neue_theme::metrics::control;

use crate::Cx;
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// What a slider or stepper emits when the user changes its value: the new value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Changed(pub(crate) f64);

/// The values a slider or stepper can take: `min` to `max`, in multiples of `step` from
/// `min` (continuous if `step` is 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Range {
    pub(super) min: f64,
    pub(super) max: f64,
    pub(super) step: f64,
}

impl Range {
    pub(super) fn new(min: f64, max: f64, step: f64) -> Self {
        // NaN and backwards ranges collapse to a single value instead of panicking in
        // `clamp`.
        let max = if max >= min { max } else { min };
        let step = if step.is_finite() && step > 0.0 {
            step
        } else {
            0.0
        };
        Self {
            min: if min.is_finite() { min } else { 0.0 },
            max: if max.is_finite() { max } else { 0.0 },
            step,
        }
    }

    /// `value` snapped to the step and kept inside the range.
    pub(super) fn clamp(self, value: f64) -> f64 {
        let value = if value.is_nan() { self.min } else { value };
        let snapped = if self.step > 0.0 {
            self.min + ((value - self.min) / self.step).round() * self.step
        } else {
            value
        };
        snapped.clamp(self.min, self.max)
    }

    /// Where `value` sits in the range, from 0 to 1.
    pub(super) fn fraction(self, value: f64) -> f64 {
        if self.max > self.min {
            ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// The value `fraction` (0..1) of the way along.
    pub(super) fn at(self, fraction: f64) -> f64 {
        self.clamp(self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min))
    }

    /// How far an arrow key or an assistive technology's Increment moves the value.
    pub(super) fn key_step(self) -> f64 {
        if self.step > 0.0 {
            self.step
        } else {
            (self.max - self.min) / 100.0
        }
    }
}

/// The Masonry widget behind [`Slider`].
#[derive(Debug)]
pub(crate) struct SliderWidget {
    range: Range,
    value: f64,
    width: f64,
    /// What assistive technology announces.
    label: Option<String>,
    replayer: ImagingReplayer,
    /// Size from the last layout.
    size: Size,
}

impl SliderWidget {
    fn new() -> Self {
        Self {
            range: Range::new(0.0, 1.0, 0.0),
            value: 0.0,
            width: control::SLIDER_WIDTH,
            label: None,
            replayer: ImagingReplayer::new(),
            size: Size::ZERO,
        }
    }

    fn rect(&self) -> Rect {
        Rect::from_origin_size((0.0, 0.0), self.size)
    }

    /// Sets the value from the app: no message goes back.
    pub(crate) fn edit_value(this: &mut masonry_core::core::WidgetMut<'_, Self>, value: f64) {
        this.widget.value = this.widget.range.clamp(value);
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// The user moved it to `value`: show it and tell the app, if it changed.
    fn change(&mut self, ctx: &mut EventCtx<'_>, value: f64) {
        let value = self.range.clamp(value);
        if value == self.value {
            return;
        }
        self.value = value;
        ctx.submit_action::<Changed>(Changed(value));
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    fn change_from_x(&mut self, ctx: &mut EventCtx<'_>, x: f64) {
        let fraction = slider_value_at(self.rect(), x);
        self.change(ctx, self.range.at(fraction));
    }
}

impl UsesProperty<ThemeProp> for SliderWidget {}

impl Widget for SliderWidget {
    type Action = Changed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                ctx.capture_pointer();
                self.change_from_x(ctx, ctx.local_position(state.position).x);
                ctx.request_paint_only();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                self.change_from_x(ctx, ctx.local_position(current.position).x);
            }
            PointerEvent::Up(..) | PointerEvent::Cancel(..) => ctx.request_paint_only(),
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
                let Range { min, max, .. } = self.range;
                let step = self.range.key_step();
                let target = match key.key {
                    Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => self.value - step,
                    Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => self.value + step,
                    Key::Named(NamedKey::PageDown) => self.value - (max - min) / 10.0,
                    Key::Named(NamedKey::PageUp) => self.value + (max - min) / 10.0,
                    Key::Named(NamedKey::Home) => min,
                    Key::Named(NamedKey::End) => max,
                    _ => return,
                };
                self.change(ctx, target);
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
        let step = self.range.key_step();
        match (event.action, &event.data) {
            (Action::Increment, _) => self.change(ctx, self.value + step),
            (Action::Decrement, _) => self.change(ctx, self.value - step),
            (Action::SetValue, Some(ActionData::NumericValue(v))) => self.change(ctx, *v),
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
            Axis::Horizontal => self.width,
            Axis::Vertical => control::SLIDER_HEIGHT,
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
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 position")]
        let spec = SliderSpec {
            value: self.range.fraction(self.value) as f32,
            pressed: ctx.is_active(),
            disabled: ctx.is_disabled(),
            focused: ctx.is_focus_target() && ctx.is_window_focused(),
        };
        let mut scene = Scene::new(size.width, size.height);
        slider(
            &mut scene,
            Rect::from_origin_size((0.0, 0.0), size),
            spec,
            theme.variant,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Slider
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if let Some(label) = &self.label {
            node.set_label(label.clone());
        }
        node.set_numeric_value(self.value);
        node.set_min_numeric_value(self.range.min);
        node.set_max_numeric_value(self.range.max);
        node.set_numeric_value_step(self.range.key_step());
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
        node.add_action(Action::SetValue);
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

/// A horizontal slider. A click or drag sets the value; while it has keyboard focus (Tab
/// moves focus) the arrow keys move it by one step, Page Up and Page Down by a tenth of
/// the range, Home and End to the ends. Sends the new value each time it changes.
///
/// The range is 0 to 1 and the value 0 until set otherwise. Without a step the value is
/// continuous and the arrow keys move it by a hundredth of the range.
pub struct Slider<M> {
    inner: NewWidget<SliderWidget>,
    on_change: Option<Box<dyn FnMut(f64) -> M>>,
}

impl<M> std::fmt::Debug for Slider<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slider")
            .field("inner", &self.inner)
            .field("on_change", &self.on_change.is_some())
            .finish()
    }
}

impl<M: 'static> Default for Slider<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: 'static> Slider<M> {
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(SliderWidget::new()),
            on_change: None,
        }
    }

    /// The values the slider covers, `min` to `max` (the start and end of `range`).
    #[must_use]
    pub fn range(mut self, range: RangeInclusive<f64>) -> Self {
        let w = &mut self.inner.widget;
        w.range = Range::new(*range.start(), *range.end(), w.range.step);
        w.value = w.range.clamp(w.value);
        self
    }

    /// Snaps the value to multiples of `step` from the range's start. 0 (the default)
    /// is continuous.
    #[must_use]
    pub fn step(mut self, step: f64) -> Self {
        let w = &mut self.inner.widget;
        w.range = Range::new(w.range.min, w.range.max, step);
        w.value = w.range.clamp(w.value);
        self
    }

    /// Starts at `value` (kept inside the range).
    #[must_use]
    pub fn value(mut self, value: f64) -> Self {
        let w = &mut self.inner.widget;
        w.value = w.range.clamp(value);
        self
    }

    /// The width, in logical pixels. 120 by default.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.inner.widget.width = width.max(control::SLIDER_HEIGHT);
        self
    }

    /// What assistive technology announces for the slider.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.widget.label = Some(label.into());
        self
    }

    /// Sends the message `make` returns, given the new value, each time the user changes
    /// it. Changes through a [`Handle`] send nothing.
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

    /// Points `handle` at this slider.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<Slider<M>> for Element<M> {
    fn from(slider: Slider<M>) -> Self {
        let id = slider.inner.id();
        let mut element = Element::new(slider.inner.erased());
        if let Some(mut make) = slider.on_change {
            element.on_action(id, move |action| {
                let Changed(value) = *action.downcast_ref::<Changed>()?;
                Some(make(value))
            });
        }
        element
    }
}

impl<M> Handle<Slider<M>> {
    /// Moves the knob to `value` (kept inside the range).
    pub fn set_value<N>(&self, cx: &mut Cx<'_, N>, value: f64) {
        cx.edit::<SliderWidget, _>(self, |mut w| SliderWidget::edit_value(&mut w, value));
    }

    /// Enables or disables the slider.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<SliderWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_snaps_to_the_step_and_stays_in_range() {
        let r = Range::new(10.0, 20.0, 2.5);
        assert_eq!(r.clamp(11.0), 10.0);
        assert_eq!(r.clamp(11.5), 12.5);
        assert_eq!(r.clamp(100.0), 20.0);
        assert_eq!(r.clamp(-5.0), 10.0);
        assert_eq!(r.clamp(f64::NAN), 10.0);
    }

    #[test]
    fn continuous_ranges_keep_the_value() {
        let r = Range::new(0.0, 1.0, 0.0);
        assert_eq!(r.clamp(0.123), 0.123);
        assert_eq!(r.key_step(), 0.01);
    }

    #[test]
    fn a_backwards_range_collapses() {
        let r = Range::new(5.0, 1.0, 1.0);
        assert_eq!((r.min, r.max), (5.0, 5.0));
        assert_eq!(r.clamp(3.0), 5.0);
        assert_eq!(r.fraction(5.0), 0.0);
    }

    #[test]
    fn fraction_and_at_round_trip() {
        let r = Range::new(-10.0, 10.0, 0.0);
        assert_eq!(r.fraction(0.0), 0.5);
        assert_eq!(r.at(0.75), 5.0);
    }
}
