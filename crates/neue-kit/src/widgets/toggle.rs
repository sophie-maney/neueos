//! Checkbox and radio button: one widget, two looks. A click on the box or the label (or
//! Space while focused) toggles it; the check mark fades in on a spring.

use std::any::TypeId;

use masonry_core::accesskit::{Action, Node, Role, Toggled as AccessToggled};
use masonry_core::core::keyboard::Key;
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
    UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{Mark, Toggle, ToggleKind, toggle};
use neue_theme::metrics::control;
use neue_theme::{Spring, Theme};

use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::anim::Animated;
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// The state of a checkbox.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CheckState {
    /// Unchecked.
    #[default]
    Off,
    /// Checked.
    On,
    /// Neither: some of what the checkbox stands for is on, some is off. Only the app sets
    /// it; a click makes it [`CheckState::On`].
    Mixed,
}

impl CheckState {
    /// How far the box fills with accent gel: 0 for off, 1 for on and mixed.
    pub(crate) fn amount(self) -> f64 {
        f64::from(u8::from(self != Self::Off))
    }

    /// What a click does to a checkbox in this state.
    fn clicked(self) -> Self {
        if self == Self::On {
            Self::Off
        } else {
            Self::On
        }
    }
}

impl From<bool> for CheckState {
    fn from(checked: bool) -> Self {
        if checked { Self::On } else { Self::Off }
    }
}

/// What a checkbox, radio button or toggle button emits when the user changes it: the new
/// state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Toggled(pub(crate) CheckState);

/// Space toggles a focused control.
pub(super) fn is_toggle_key(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ")
}

/// The Masonry widget behind [`Checkbox`] and [`Radio`].
#[derive(Debug)]
pub(crate) struct ToggleWidget {
    kind: ToggleKind,
    line: TextLine,
    replayer: ImagingReplayer,
    state: CheckState,
    /// Fill amount of the box, springing between 0 and 1.
    on: Animated,
    /// Space went down while focused: drawn pressed until it comes up.
    key_down: bool,
}

impl ToggleWidget {
    fn new(kind: ToggleKind, label: String, state: CheckState) -> Self {
        Self {
            kind,
            line: TextLine::new(label, TextStyle::Body),
            replayer: ImagingReplayer::new(),
            state,
            on: Animated::new(state.amount(), Spring::SNAPPY),
            key_down: false,
        }
    }

    pub(crate) fn edit_label(this: &mut WidgetMut<'_, Self>, label: String) {
        if this.widget.line.set_text(label) {
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets the state from the app: no message goes back.
    pub(crate) fn edit_state(this: &mut WidgetMut<'_, Self>, state: CheckState) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        this.widget.set_state(&mut this.ctx, &theme, state);
        this.ctx.request_accessibility_update();
    }

    fn set_state(&mut self, ctx: &mut impl crate::anim::AnimCtx, theme: &Theme, state: CheckState) {
        self.state = state;
        self.on.animate_to(ctx, theme, state.amount());
    }

    /// The user toggled it: change state and tell the app.
    fn activate(&mut self, ctx: &mut EventCtx<'_>, props: &mut PropertiesMut<'_>) {
        let next = match self.kind {
            ToggleKind::Checkbox => self.state.clicked(),
            // A radio button only turns on; the app turns the others off.
            ToggleKind::Radio if self.state == CheckState::On => return,
            ToggleKind::Radio => CheckState::On,
        };
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.set_state(ctx, &theme, next);
        ctx.submit_action::<Toggled>(Toggled(next));
        ctx.request_accessibility_update();
    }
}

impl UsesProperty<ThemeProp> for ToggleWidget {}

impl Widget for ToggleWidget {
    type Action = Toggled;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(..) => {
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(..) => {
                if ctx.is_active() && ctx.is_hovered() {
                    self.activate(ctx, props);
                }
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if is_toggle_key(&key.key) => {
                if key.state.is_down() {
                    self.key_down = true;
                } else if std::mem::take(&mut self.key_down) {
                    self.activate(ctx, props);
                }
                ctx.request_paint_only();
                ctx.set_handled();
            }
            TextEvent::WindowFocusChange(_) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            self.activate(ctx, props);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) | Update::HoveredChanged(_) | Update::ActiveChanged(_) => {
                ctx.request_paint_only();
            }
            Update::FocusChanged(focused) => {
                if !focused {
                    self.key_down = false;
                }
                ctx.request_paint_only();
            }
            Update::FontsChanged => {
                self.line.invalidate();
                ctx.request_layout();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
            self.on.theme_changed(ctx);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.on.on_anim_frame(ctx, &theme, interval);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let text = self.line.size(ctx.text_contexts());
        Length::px(match axis {
            Axis::Horizontal => control::CHECKBOX_SIZE + control::CHECKBOX_GAP + text.width,
            Axis::Vertical => control::SMALL_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let text = self.line.size(ctx.text_contexts());
        let baseline = (size.height - text.height) / 2.0 + self.line.baseline();
        ctx.set_baselines(baseline, baseline);
        // The focus ring, shadow and etch draw outside the box.
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
        let window_focused = ctx.is_window_focused();
        let side = control::CHECKBOX_SIZE;
        let rect =
            Rect::from_origin_size((0.0, ((size.height - side) / 2.0).round()), (side, side));
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 blend amount")]
        let spec = Toggle {
            kind: self.kind,
            mark: if self.state == CheckState::Mixed {
                Mark::Dash
            } else {
                Mark::Check
            },
            on: self.on.value() as f32,
            pressed: (ctx.is_active() && ctx.is_hovered()) || self.key_down,
            disabled: ctx.is_disabled(),
            focused: ctx.is_focus_target() && window_focused,
        };
        let mut scene = Scene::new(size.width, size.height);
        toggle(&mut scene, rect, spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let colors = theme.colors();
        let color = if ctx.is_disabled() {
            colors.disabled_label
        } else {
            colors.label
        };
        let text = self.line.size(ctx.text_contexts());
        let origin = Vec2::new(
            side + control::CHECKBOX_GAP,
            ((size.height - text.height) / 2.0).round(),
        );
        self.line.paint(painter, Affine::translate(origin), color);
    }

    fn accessibility_role(&self) -> Role {
        match self.kind {
            ToggleKind::Checkbox => Role::CheckBox,
            ToggleKind::Radio => Role::RadioButton,
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.line.text());
        node.add_action(Action::Click);
        node.set_toggled(match self.state {
            CheckState::Off => AccessToggled::False,
            CheckState::On => AccessToggled::True,
            CheckState::Mixed => AccessToggled::Mixed,
        });
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.line.text().to_owned())
    }
}

/// A checkbox: toggles when clicked, or by Space while it has keyboard focus (Tab moves
/// focus), and sends a message with its new state. It can also show
/// [`CheckState::Mixed`], which only the app sets.
pub struct Checkbox<M> {
    inner: NewWidget<ToggleWidget>,
    on_toggle: Option<Box<dyn FnMut(bool) -> M>>,
}

impl<M> std::fmt::Debug for Checkbox<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkbox")
            .field("inner", &self.inner)
            .field("on_toggle", &self.on_toggle.is_some())
            .finish()
    }
}

impl<M: 'static> Checkbox<M> {
    /// An unchecked checkbox with `label` beside it.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            inner: NewWidget::new(ToggleWidget::new(
                ToggleKind::Checkbox,
                label.into(),
                CheckState::Off,
            )),
            on_toggle: None,
        }
    }

    /// Starts checked or unchecked.
    #[must_use]
    pub fn checked(self, checked: bool) -> Self {
        self.state(checked.into())
    }

    /// Starts in `state` (use [`CheckState::Mixed`] for a partly-on group).
    #[must_use]
    pub fn state(mut self, state: CheckState) -> Self {
        self.inner.widget.state = state;
        self.inner.widget.on = Animated::new(state.amount(), Spring::SNAPPY);
        self
    }

    /// Sends the message `make` returns, given whether the checkbox is now checked, each
    /// time the user toggles it. Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_toggle(mut self, make: impl FnMut(bool) -> M + 'static) -> Self {
        self.on_toggle = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this checkbox.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<Checkbox<M>> for Element<M> {
    fn from(checkbox: Checkbox<M>) -> Self {
        let id = checkbox.inner.id();
        let mut element = Element::new(checkbox.inner.erased());
        if let Some(mut make) = checkbox.on_toggle {
            element.on_action(id, move |action| {
                let Toggled(state) = *action.downcast_ref::<Toggled>()?;
                Some(make(state == CheckState::On))
            });
        }
        element
    }
}

impl<M> Handle<Checkbox<M>> {
    /// Checks or unchecks the checkbox.
    pub fn set_checked<N>(&self, cx: &mut Cx<'_, N>, checked: bool) {
        self.set_state(cx, checked.into());
    }

    /// Sets the checkbox's state, including [`CheckState::Mixed`].
    pub fn set_state<N>(&self, cx: &mut Cx<'_, N>, state: CheckState) {
        cx.edit::<ToggleWidget, _>(self, |mut w| ToggleWidget::edit_state(&mut w, state));
    }

    /// Replaces the label.
    pub fn set_label<N>(&self, cx: &mut Cx<'_, N>, label: impl Into<String>) {
        let label = label.into();
        cx.edit::<ToggleWidget, _>(self, |mut w| ToggleWidget::edit_label(&mut w, label));
    }

    /// Enables or disables the checkbox.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ToggleWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

/// A radio button: selects when clicked, or by Space while it has keyboard focus, and
/// sends a message. It never deselects itself: the app owns the choice and turns the
/// others off through their [`Handle`]s when one reports it was selected.
pub struct Radio<M> {
    inner: NewWidget<ToggleWidget>,
    on_select: Option<Box<dyn FnMut() -> M>>,
}

impl<M> std::fmt::Debug for Radio<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Radio")
            .field("inner", &self.inner)
            .field("on_select", &self.on_select.is_some())
            .finish()
    }
}

impl<M: 'static> Radio<M> {
    /// An unselected radio button with `label` beside it.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            inner: NewWidget::new(ToggleWidget::new(
                ToggleKind::Radio,
                label.into(),
                CheckState::Off,
            )),
            on_select: None,
        }
    }

    /// Starts selected or not.
    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        let state = CheckState::from(selected);
        self.inner.widget.state = state;
        self.inner.widget.on = Animated::new(state.amount(), Spring::SNAPPY);
        self
    }

    /// Sends `message` when the user selects this radio button.
    #[must_use]
    pub fn on_select(self, message: M) -> Self
    where
        M: Clone,
    {
        self.on_select_with(move || message.clone())
    }

    /// Sends the message `make` returns when the user selects this radio button.
    #[must_use]
    pub fn on_select_with(mut self, make: impl FnMut() -> M + 'static) -> Self {
        self.on_select = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this radio button.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<Radio<M>> for Element<M> {
    fn from(radio: Radio<M>) -> Self {
        let id = radio.inner.id();
        let mut element = Element::new(radio.inner.erased());
        if let Some(mut make) = radio.on_select {
            element.on_action(id, move |action| action.is::<Toggled>().then(&mut make));
        }
        element
    }
}

impl<M> Handle<Radio<M>> {
    /// Selects or deselects the radio button.
    pub fn set_selected<N>(&self, cx: &mut Cx<'_, N>, selected: bool) {
        cx.edit::<ToggleWidget, _>(self, |mut w| {
            ToggleWidget::edit_state(&mut w, selected.into());
        });
    }

    /// Replaces the label.
    pub fn set_label<N>(&self, cx: &mut Cx<'_, N>, label: impl Into<String>) {
        let label = label.into();
        cx.edit::<ToggleWidget, _>(self, |mut w| ToggleWidget::edit_label(&mut w, label));
    }

    /// Enables or disables the radio button.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ToggleWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
