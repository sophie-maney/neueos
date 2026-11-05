//! The colour well (task 2.24b, ADR 0061): a bevel button holding a swatch. A press opens a
//! popover with a colour panel: a hue and saturation area, brightness and opacity sliders,
//! and a hex field. The app hears the new colour continuously while the user drags.
//!
//! - **The panel owns the colour while it is open.** It keeps it as HSB, which a colour
//!   alone can't (a grey has no hue), and tells the well after each change; the well
//!   updates its swatch and sends [`Changed`].
//! - **The panel is built from kit widgets.** The sliders and the field are the ordinary
//!   ones; their actions reach the panel because Masonry offers a widget's actions to its
//!   ancestors before the app.
//! - **Closing** is the popover's: Escape, a press outside, or pressing the well again.

use std::any::TypeId;

use masonry_core::accesskit::{Action, ActionData, HasPopup, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ActionCtx, ChildrenIds, ErasedAction, EventCtx, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId,
    WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Point, Rect, Size};
use masonry_core::layout::{LenDef, LenReq, Length, SizeDef};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{
    ColorArea, ColorWell as WellSpec, Hsb, color_area, color_area_value_at, color_hex, color_well,
    parse_hex,
};
use neue_theme::Color;
use neue_theme::materials::color_well::{
    AREA_HEIGHT, KEY_STEP, KNOB_RADIUS, LABEL_WIDTH, PANEL_WIDTH, ROW_HEIGHT, WIDTH,
};
use neue_theme::metrics::{control, spacing};

use super::field::FieldAction;
use super::popover::{self, PopoverWidget};
use super::slider::Changed as SliderChanged;
use super::toggle::is_toggle_key;
use super::{FieldWidget, SliderWidget};
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;
use crate::{Cx, Label, Popover, Slider, TextField};

/// What a colour well emits when the user changes its colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Changed(pub(crate) Color);

/// What the area emits: the hue (degrees) and saturation under the pointer or the keys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AreaChanged {
    hue: f32,
    saturation: f32,
}

/// The Masonry widget behind [`ColorWell`].
#[derive(Debug)]
pub(crate) struct ColorWellWidget {
    color: Color,
    /// What assistive technology announces.
    label: Option<String>,
    /// Space or Return went down while focused: drawn pressed until it comes up.
    key_down: bool,
    replayer: ImagingReplayer,
}

impl ColorWellWidget {
    fn new(color: Color) -> Self {
        Self {
            color,
            label: None,
            key_down: false,
            replayer: ImagingReplayer::new(),
        }
    }

    /// Sets the colour from the app: no message goes back.
    pub(crate) fn edit_color(this: &mut WidgetMut<'_, Self>, color: Color) {
        this.widget.color = color;
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// The panel changed the colour: show it and tell the app.
    fn panel_changed(this: &mut WidgetMut<'_, Self>, color: Color) {
        Self::edit_color(this, color);
        this.ctx.submit_action::<Changed>(Changed(color));
    }

    /// Opens the panel, or closes it if it is open.
    fn toggle(ctx: &mut EventCtx<'_>, color: Color) {
        if ctx.is_disabled() {
            return;
        }
        let me = ctx.widget_id();
        ctx.mutate_later(me, move |mut owner| {
            if let Some(open) = popover::shown(&owner) {
                owner.ctx.mutate_later(open, |mut panel| {
                    PopoverWidget::close(&mut panel.downcast::<PopoverWidget>(), false);
                });
                return;
            }
            let panel = NewWidget::new(ColorPanelWidget::new(me, color));
            let (popover, _wiring) = Popover::<()>::new(Element::new(panel.erased())).into_parts();
            popover::show(&mut owner, popover);
        });
    }
}

impl UsesProperty<ThemeProp> for ColorWellWidget {}

impl Widget for ColorWellWidget {
    type Action = Changed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(..) => {
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(..) => {
                if ctx.is_active() && ctx.is_hovered() {
                    Self::toggle(ctx, self.color);
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
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key)
                if is_toggle_key(&key.key) || key.key == Key::Named(NamedKey::Enter) =>
            {
                if key.state.is_down() {
                    self.key_down = true;
                } else if std::mem::take(&mut self.key_down) {
                    Self::toggle(ctx, self.color);
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
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            Self::toggle(ctx, self.color);
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
            Axis::Horizontal => WIDTH,
            Axis::Vertical => control::REGULAR_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
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
        let well = WellSpec {
            color: self.color,
            depth: if pressed { 1.0 } else { 0.0 },
            disabled: ctx.is_disabled(),
            focused: ctx.is_focus_target() && ctx.is_window_focused(),
        };
        let mut scene = Scene::new(size.width, size.height);
        color_well(
            &mut scene,
            Rect::from_origin_size((0.0, 0.0), size),
            well,
            theme.variant,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::ColorWell
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
        node.set_value(color_hex(self.color));
        node.set_has_popup(HasPopup::Dialog);
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(color_hex(self.color))
    }
}

/// The Masonry widget of the hue and saturation area.
#[derive(Debug)]
pub(crate) struct HsAreaWidget {
    hsb: Hsb,
    replayer: ImagingReplayer,
    /// Size from the last layout.
    size: Size,
}

impl HsAreaWidget {
    fn new(hsb: Hsb) -> Self {
        Self {
            hsb,
            replayer: ImagingReplayer::new(),
            size: Size::ZERO,
        }
    }

    fn rect(&self) -> Rect {
        Rect::from_origin_size((0.0, 0.0), self.size)
    }

    /// Shows `hsb` (the panel changed it from elsewhere): no message goes back.
    fn edit_hsb(this: &mut WidgetMut<'_, Self>, hsb: Hsb) {
        this.widget.hsb = hsb;
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// The user moved to `hue` and `saturation`: show it and tell the panel.
    fn change(&mut self, ctx: &mut EventCtx<'_>, hue: f32, saturation: f32) {
        let hue = hue.clamp(0.0, 359.99);
        let saturation = saturation.clamp(0.0, 1.0);
        if hue == self.hsb.hue && saturation == self.hsb.saturation {
            return;
        }
        self.hsb.hue = hue;
        self.hsb.saturation = saturation;
        ctx.submit_action::<AreaChanged>(AreaChanged { hue, saturation });
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    fn change_to(&mut self, ctx: &mut EventCtx<'_>, at: Point) {
        let (hue, saturation) = color_area_value_at(self.rect(), at);
        self.change(ctx, hue, saturation);
    }
}

impl UsesProperty<ThemeProp> for HsAreaWidget {}

impl Widget for HsAreaWidget {
    type Action = AreaChanged;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                ctx.capture_pointer();
                self.change_to(ctx, ctx.local_position(state.position));
                ctx.request_paint_only();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                self.change_to(ctx, ctx.local_position(current.position));
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
                #[expect(clippy::cast_possible_truncation, reason = "a small fraction")]
                let step = KEY_STEP as f32;
                let Hsb {
                    hue, saturation, ..
                } = self.hsb;
                let (hue, saturation) = match key.key {
                    Key::Named(NamedKey::ArrowLeft) => (hue - step * 360.0, saturation),
                    Key::Named(NamedKey::ArrowRight) => (hue + step * 360.0, saturation),
                    Key::Named(NamedKey::ArrowUp) => (hue, saturation + step),
                    Key::Named(NamedKey::ArrowDown) => (hue, saturation - step),
                    _ => return,
                };
                self.change(ctx, hue, saturation);
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
        #[expect(clippy::cast_possible_truncation, reason = "a small fraction")]
        let step = KEY_STEP as f32 * 360.0;
        let hue = self.hsb.hue;
        match (event.action, &event.data) {
            (Action::Increment, _) => self.change(ctx, hue + step, self.hsb.saturation),
            (Action::Decrement, _) => self.change(ctx, hue - step, self.hsb.saturation),
            (Action::SetValue, Some(ActionData::NumericValue(v))) => {
                #[expect(clippy::cast_possible_truncation, reason = "a hue in degrees")]
                self.change(ctx, *v as f32, self.hsb.saturation);
            }
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
            Axis::Horizontal => PANEL_WIDTH,
            Axis::Vertical => AREA_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        // The knob sits on the edges at the extremes: it and its focus ring overhang.
        ctx.set_paint_insets(Insets::uniform(
            KNOB_RADIUS + 1.5 + control::FOCUS_RING + control::RIM,
        ));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let area = ColorArea {
            hsb: self.hsb,
            disabled: ctx.is_disabled(),
            focused: ctx.is_focus_target() && ctx.is_window_focused(),
        };
        let mut scene = Scene::new(size.width, size.height);
        color_area(
            &mut scene,
            Rect::from_origin_size((0.0, 0.0), size),
            area,
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
        node.set_label("Hue and saturation");
        node.set_value(format!(
            "Hue {:.0}°, saturation {:.0}%",
            self.hsb.hue,
            self.hsb.saturation * 100.0
        ));
        node.set_numeric_value(f64::from(self.hsb.hue));
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(360.0);
        node.set_numeric_value_step(KEY_STEP * 360.0);
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
}

/// The colour panel in the well's popover: the area, two sliders and the hex field, each
/// in its row, with labels.
pub(crate) struct ColorPanelWidget {
    /// The well it belongs to.
    owner: WidgetId,
    /// The colour, as the panel's controls see it.
    hsb: Hsb,
    area: WidgetPod<dyn Widget>,
    brightness: WidgetPod<dyn Widget>,
    opacity: WidgetPod<dyn Widget>,
    hex: WidgetPod<dyn Widget>,
    /// The row labels: brightness, opacity, hex.
    labels: [WidgetPod<dyn Widget>; 3],
}

impl std::fmt::Debug for ColorPanelWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorPanelWidget")
            .field("owner", &self.owner)
            .field("hsb", &self.hsb)
            .finish_non_exhaustive()
    }
}

/// A widget out of a kit builder that sends no messages of its own.
fn plain(element: impl Into<Element<()>>) -> NewWidget<dyn Widget> {
    element.into().into_parts().0
}

impl ColorPanelWidget {
    fn new(owner: WidgetId, color: Color) -> Self {
        let hsb = Hsb::from_color(color, Hsb::default());
        let area = NewWidget::new(HsAreaWidget::new(hsb)).erased();
        let brightness = plain(
            Slider::new()
                .value(f64::from(hsb.brightness))
                .label("Brightness"),
        );
        let opacity = plain(Slider::new().value(f64::from(hsb.alpha)).label("Opacity"));
        // The handlers only make the field send its actions (Return too, so that it keeps
        // Return from the window); the panel listens to them as they pass.
        let hex = plain(
            TextField::new()
                .text(&color_hex(color))
                .label("Hex colour")
                .on_change(|_| ())
                .on_submit(|_| ()),
        );
        let label = |text: &str| Label::new(text).secondary();
        Self {
            owner,
            hsb,
            area: area.to_pod(),
            brightness: brightness.to_pod(),
            opacity: opacity.to_pod(),
            hex: hex.to_pod(),
            labels: [
                plain(label("Brightness")).to_pod(),
                plain(label("Opacity")).to_pod(),
                plain(label("Hex")).to_pod(),
            ],
        }
    }

    /// Shows the panel's colour in the controls other than `except` (the hex field only if
    /// `hex_too`), and tells the well if `notify`.
    fn show(&self, ctx: &mut ActionCtx<'_>, except: WidgetId, hex_too: bool, notify: bool) {
        let hsb = self.hsb;
        if self.area.id() != except {
            ctx.mutate_later(self.area.id(), move |mut w| {
                HsAreaWidget::edit_hsb(&mut w.downcast::<HsAreaWidget>(), hsb);
            });
        }
        for (slider, value) in [
            (self.brightness.id(), hsb.brightness),
            (self.opacity.id(), hsb.alpha),
        ] {
            if slider != except {
                ctx.mutate_later(slider, move |mut w| {
                    SliderWidget::edit_value(&mut w.downcast::<SliderWidget>(), f64::from(value));
                });
            }
        }
        let color = hsb.to_color();
        if hex_too {
            ctx.mutate_later(self.hex.id(), move |mut w| {
                FieldWidget::edit_text(&mut w.downcast::<FieldWidget>(), &color_hex(color));
            });
        }
        if notify {
            ctx.mutate_later(self.owner, move |mut w| {
                ColorWellWidget::panel_changed(&mut w.downcast::<ColorWellWidget>(), color);
            });
        }
    }
}

impl Widget for ColorPanelWidget {
    type Action = masonry_core::core::NoAction;

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        if source == self.area.id() {
            let Some(&AreaChanged { hue, saturation }) = action.downcast_ref::<AreaChanged>()
            else {
                return;
            };
            self.hsb.hue = hue;
            self.hsb.saturation = saturation;
            self.show(ctx, source, true, true);
        } else if source == self.brightness.id() || source == self.opacity.id() {
            let Some(&SliderChanged(value)) = action.downcast_ref::<SliderChanged>() else {
                return;
            };
            #[expect(clippy::cast_possible_truncation, reason = "a 0..1 slider")]
            let value = value as f32;
            if source == self.brightness.id() {
                self.hsb.brightness = value;
            } else {
                self.hsb.alpha = value;
            }
            self.show(ctx, source, true, true);
        } else if source == self.hex.id() {
            let (text, submitted) = match action.downcast_ref::<FieldAction>() {
                Some(FieldAction::Changed(text)) => (text, false),
                Some(FieldAction::Submitted(text)) => (text, true),
                _ => return,
            };
            match parse_hex(text, self.hsb.alpha) {
                Some(color) => {
                    self.hsb = Hsb::from_color(color, self.hsb);
                    // While typing the field keeps what was typed; Return tidies it (the
                    // colour went out when it was typed).
                    self.show(ctx, source, submitted, !submitted);
                }
                // Return on something that isn't a colour puts the colour back.
                None if submitted => self.show(ctx, source, true, false),
                None => {}
            }
        } else {
            return;
        }
        ctx.set_handled();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.area);
        for label in &mut self.labels {
            ctx.register_child(label);
        }
        ctx.register_child(&mut self.brightness);
        ctx.register_child(&mut self.opacity);
        ctx.register_child(&mut self.hex);
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
            Axis::Horizontal => PANEL_WIDTH,
            Axis::Vertical => AREA_HEIGHT + 3.0 * (spacing::S + ROW_HEIGHT),
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.area, Size::new(size.width, AREA_HEIGHT));
        ctx.place_child(&mut self.area, Point::ORIGIN);

        let control_x = LABEL_WIDTH + spacing::S;
        let control_width = (size.width - control_x).max(0.0);
        let controls = [&mut self.brightness, &mut self.opacity, &mut self.hex];
        for (i, (label, control)) in self.labels.iter_mut().zip(controls).enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "a row of three")]
            let y = AREA_HEIGHT + (spacing::S + ROW_HEIGHT) * (i + 1) as f64 - ROW_HEIGHT;
            let natural = ctx.compute_size(
                label,
                SizeDef::new(LenDef::MaxContent, LenDef::MaxContent),
                size.into(),
            );
            ctx.run_layout(label, natural);
            // Labels sit against their controls, right-aligned in the label column.
            ctx.place_child(
                label,
                Point::new(
                    (LABEL_WIDTH - natural.width).max(0.0),
                    y + ((ROW_HEIGHT - natural.height) / 2.0).round(),
                ),
            );
            ctx.run_layout(control, Size::new(control_width, ROW_HEIGHT));
            ctx.place_child(control, Point::new(control_x, y));
        }
        ctx.clear_baselines();
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label("Colour");
    }

    fn children_ids(&self) -> ChildrenIds {
        let mut ids = vec![self.area.id()];
        ids.extend(self.labels.iter().map(WidgetPod::id));
        ids.extend([self.brightness.id(), self.opacity.id(), self.hex.id()]);
        ids.into_iter().collect()
    }
}

/// A colour well: a swatch in a bevelled button. A press, or Space or Return while it has
/// keyboard focus, opens a panel to choose the colour with (hue and saturation, brightness,
/// opacity, or a hex value); the app hears each change as it happens, while the user drags
/// too. A colour that isn't opaque shows its opacity over a checkerboard.
///
/// ```no_run
/// # use neue_kit::{ColorWell, tokens::Color};
/// # #[derive(Clone)] enum Msg { Background(Color) }
/// let well = ColorWell::new(Color::new([0.2, 0.5, 0.9, 1.0]))
///     .label("Background")
///     .on_change(Msg::Background);
/// ```
pub struct ColorWell<M> {
    inner: NewWidget<ColorWellWidget>,
    on_change: Option<Box<dyn FnMut(Color) -> M>>,
}

impl<M> std::fmt::Debug for ColorWell<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColorWell")
            .field("inner", &self.inner)
            .field("on_change", &self.on_change.is_some())
            .finish()
    }
}

impl<M: 'static> ColorWell<M> {
    /// A well showing `color`.
    pub fn new(color: Color) -> Self {
        Self {
            inner: NewWidget::new(ColorWellWidget::new(color)),
            on_change: None,
        }
    }

    /// What assistive technology announces for the well.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.widget.label = Some(label.into());
        self
    }

    /// Sends the message `make` returns, given the new colour, each time the user changes
    /// it. Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_change(mut self, make: impl FnMut(Color) -> M + 'static) -> Self {
        self.on_change = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this well.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<ColorWell<M>> for Element<M> {
    fn from(well: ColorWell<M>) -> Self {
        let id = well.inner.id();
        let mut element = Element::new(well.inner.erased());
        if let Some(mut make) = well.on_change {
            element.on_action(id, move |action| {
                let Changed(color) = *action.downcast_ref::<Changed>()?;
                Some(make(color))
            });
        }
        element
    }
}

impl<M> Handle<ColorWell<M>> {
    /// Shows `color` in the swatch. A panel that is open keeps showing the old one.
    pub fn set_color<N>(&self, cx: &mut Cx<'_, N>, color: Color) {
        cx.edit::<ColorWellWidget, _>(self, |mut w| ColorWellWidget::edit_color(&mut w, color));
    }

    /// Enables or disables the well.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ColorWellWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
