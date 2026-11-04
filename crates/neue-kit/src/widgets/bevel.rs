//! Bevel and recessed buttons: the toolbar and inspector buttons. Momentary, they send a
//! press like a push button; as toggles they stay down (bevel) or fill with a dark pill
//! (recessed) until pressed again.

use std::any::TypeId;

use masonry_core::accesskit::{Action, Node, Role, Toggled as AccessToggled};
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
use neue_paint::materials::{
    BevelButton as BevelSpec, RecessedButton, bevel_button, recessed_button,
};
use neue_theme::materials::bevel_button::recessed;
use neue_theme::metrics::control;
use neue_theme::{Spring, Theme};

use super::text::{TextLine, TextStyle};
use super::toggle::{CheckState, Toggled, is_toggle_key};
use crate::Cx;
use crate::anim::{AnimCtx, Animated};
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// Which of the two looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Bevel,
    Recessed,
}

/// The Masonry widget behind [`crate::BevelButton`].
#[derive(Debug)]
pub(crate) struct BevelWidget {
    look: Look,
    line: TextLine,
    replayer: ImagingReplayer,
    /// A toggle stays on after a press; otherwise a press sends `Toggled(On)`, which the
    /// builder maps to `on_press`.
    toggle: bool,
    on: bool,
    /// How far on it is (a toggle's state), springing between 0 and 1.
    amount: Animated,
    /// Space went down while focused: drawn pressed until it comes up.
    key_down: bool,
}

impl BevelWidget {
    fn new(look: Look, label: String) -> Self {
        Self {
            look,
            line: TextLine::new(label, TextStyle::Body),
            replayer: ImagingReplayer::new(),
            toggle: false,
            on: false,
            amount: Animated::new(0.0, Spring::SNAPPY),
            key_down: false,
        }
    }

    pub(crate) fn edit_label(this: &mut WidgetMut<'_, Self>, label: String) {
        if this.widget.line.set_text(label) {
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }

    /// Sets a toggle's state from the app: no message goes back.
    pub(crate) fn edit_on(this: &mut WidgetMut<'_, Self>, on: bool) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        this.widget.set_on(&mut this.ctx, &theme, on);
        this.ctx.request_accessibility_update();
    }

    fn set_on(&mut self, ctx: &mut impl AnimCtx, theme: &Theme, on: bool) {
        self.on = on;
        self.amount.animate_to(ctx, theme, f64::from(u8::from(on)));
    }

    fn activate(&mut self, ctx: &mut EventCtx<'_>, props: &mut PropertiesMut<'_>) {
        if self.toggle {
            let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
            self.set_on(ctx, &theme, !self.on);
            ctx.submit_action::<Toggled>(Toggled(CheckState::from(self.on)));
            ctx.request_accessibility_update();
        } else {
            ctx.submit_action::<Toggled>(Toggled(CheckState::On));
        }
    }
}

impl UsesProperty<ThemeProp> for BevelWidget {}

impl Widget for BevelWidget {
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
            self.amount.theme_changed(ctx);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.amount.on_anim_frame(ctx, &theme, interval);
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
            Axis::Horizontal => {
                (text.width + 2.0 * control::BUTTON_PADDING).max(control::BEVEL_MIN_WIDTH)
            }
            Axis::Vertical => match self.look {
                Look::Bevel => control::REGULAR_HEIGHT,
                Look::Recessed => control::SMALL_HEIGHT,
            },
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let text = self.line.size(ctx.text_contexts());
        let baseline = (size.height - text.height) / 2.0 + self.line.baseline();
        ctx.set_baselines(baseline, baseline);
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
        let pressed = (ctx.is_active() && ctx.is_hovered()) || self.key_down;
        // Held down or on: sunk in as far as the toggle's spring has got.
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 blend amount")]
        let depth = if pressed {
            1.0
        } else {
            self.amount.value() as f32
        };
        let (disabled, focused) = (
            ctx.is_disabled(),
            ctx.is_focus_target() && ctx.is_window_focused(),
        );
        let rect = Rect::from_origin_size((0.0, 0.0), size);
        let mut scene = Scene::new(size.width, size.height);
        let colors = theme.colors();
        let mut color = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        match self.look {
            Look::Bevel => bevel_button(
                &mut scene,
                rect,
                BevelSpec {
                    depth,
                    disabled,
                    focused,
                },
                theme.variant,
            ),
            Look::Recessed => {
                recessed_button(
                    &mut scene,
                    rect,
                    RecessedButton {
                        amount: depth,
                        disabled,
                        focused,
                    },
                    theme.variant,
                );
                if !disabled {
                    color = recessed::Palette::new(theme.variant, depth).label(color, depth);
                }
            }
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let text = self.line.size(ctx.text_contexts());
        let origin = Vec2::new(
            ((size.width - text.width) / 2.0).round(),
            ((size.height - text.height) / 2.0).round(),
        );
        self.line.paint(painter, Affine::translate(origin), color);
    }

    fn accessibility_role(&self) -> Role {
        // AccessKit has no toggle-button role: a button with a toggled state is one.
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.line.text());
        node.add_action(Action::Click);
        if self.toggle {
            node.set_toggled(AccessToggled::from(self.on));
        }
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

/// A button for toolbars and inspectors, in one of two looks: the **bevel** button (grey,
/// sinks in while pressed) and the **recessed** button (plain text that fills with a dark
/// pill while pressed).
///
/// By default it is momentary: it sends [`on_press`](Self::on_press) when pressed (pointer,
/// Space while it has keyboard focus, or an assistive technology). With
/// [`on_toggle`](Self::on_toggle) it stays down until pressed again.
pub struct BevelButton<M> {
    inner: NewWidget<BevelWidget>,
    on_press: Option<Box<dyn FnMut() -> M>>,
    on_toggle: Option<Box<dyn FnMut(bool) -> M>>,
}

impl<M> std::fmt::Debug for BevelButton<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BevelButton")
            .field("inner", &self.inner)
            .field("on_press", &self.on_press.is_some())
            .field("on_toggle", &self.on_toggle.is_some())
            .finish()
    }
}

impl<M: 'static> BevelButton<M> {
    fn with_look(look: Look, label: String) -> Self {
        Self {
            inner: NewWidget::new(BevelWidget::new(look, label)),
            on_press: None,
            on_toggle: None,
        }
    }

    /// A bevel button.
    pub fn new(label: impl Into<String>) -> Self {
        Self::with_look(Look::Bevel, label.into())
    }

    /// A recessed button.
    pub fn recessed(label: impl Into<String>) -> Self {
        Self::with_look(Look::Recessed, label.into())
    }

    /// Sends `message` on every press (momentary buttons).
    #[must_use]
    pub fn on_press(self, message: M) -> Self
    where
        M: Clone,
    {
        self.on_press_with(move || message.clone())
    }

    /// Sends the message `make` returns on every press (momentary buttons).
    #[must_use]
    pub fn on_press_with(mut self, make: impl FnMut() -> M + 'static) -> Self {
        self.inner.widget.toggle = false;
        self.on_toggle = None;
        self.on_press = Some(Box::new(make));
        self
    }

    /// Makes this a toggle: it stays down after a press, and sends the message `make`
    /// returns, given whether it is now on. Changes through a [`Handle`] send nothing.
    /// Replaces [`on_press`](Self::on_press).
    #[must_use]
    pub fn on_toggle(mut self, make: impl FnMut(bool) -> M + 'static) -> Self {
        self.inner.widget.toggle = true;
        self.on_press = None;
        self.on_toggle = Some(Box::new(make));
        self
    }

    /// A toggle that starts on.
    #[must_use]
    pub fn on(mut self, on: bool) -> Self {
        self.inner.widget.toggle = true;
        self.inner.widget.on = on;
        self.inner.widget.amount = Animated::new(f64::from(u8::from(on)), Spring::SNAPPY);
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this button.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<BevelButton<M>> for Element<M> {
    fn from(button: BevelButton<M>) -> Self {
        let id = button.inner.id();
        let mut element = Element::new(button.inner.erased());
        // A button is a toggle or momentary, not both: only one handler is set.
        if let Some(mut make) = button.on_press {
            element.on_action(id, move |action| action.is::<Toggled>().then(&mut make));
        }
        if let Some(mut make) = button.on_toggle {
            element.on_action(id, move |action| {
                let Toggled(state) = *action.downcast_ref::<Toggled>()?;
                Some(make(state == CheckState::On))
            });
        }
        element
    }
}

impl<M> Handle<BevelButton<M>> {
    /// Replaces the button's title.
    pub fn set_label<N>(&self, cx: &mut Cx<'_, N>, label: impl Into<String>) {
        let label = label.into();
        cx.edit::<BevelWidget, _>(self, |mut w| BevelWidget::edit_label(&mut w, label));
    }

    /// Turns a toggle on or off.
    pub fn set_on<N>(&self, cx: &mut Cx<'_, N>, on: bool) {
        cx.edit::<BevelWidget, _>(self, |mut w| BevelWidget::edit_on(&mut w, on));
    }

    /// Enables or disables the button.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<BevelWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
