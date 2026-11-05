//! The Aqua push button: neutral gel that takes the accent while held down; the default
//! button (Return presses it) shows the accent at rest and pulses while its window is
//! active. Space presses a focused button; Tab focuses it and shows the focus ring.

use std::any::TypeId;
use std::f64::consts::TAU;
use std::time::Duration;

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::keyboard::Key;
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget,
    WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{ButtonState, PushButton, push_button};
use neue_theme::Theme;
use neue_theme::metrics::control;
use neue_theme::motion::DEFAULT_BUTTON_PULSE;

use super::text::{TextLine, TextStyle};
use crate::anim::AnimCtx;
use crate::theme::ThemeProp;

/// What a button emits when it's pressed (pointer, Space when focused, Return for the
/// default button, or an assistive technology).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pressed;

/// The default button's pulse: time into the current cycle, and whether animation frames
/// are coming.
#[derive(Debug, Default)]
struct Pulse {
    elapsed: Duration,
    running: bool,
}

impl Pulse {
    /// 0 at rest, rising to 1 half a period in, and back.
    fn amount(&self) -> f64 {
        let t = self.elapsed.as_secs_f64() / DEFAULT_BUTTON_PULSE.as_secs_f64();
        0.5 - 0.5 * (TAU * t).cos()
    }

    fn advance(&mut self, by: Duration) {
        let period = DEFAULT_BUTTON_PULSE.as_nanos();
        let nanos = (self.elapsed.as_nanos() + by.as_nanos()) % period;
        // The remainder is below the period, which fits in a u64 of nanoseconds.
        self.elapsed = Duration::from_nanos(u64::try_from(nanos).unwrap_or(0));
    }
}

/// The Masonry widget behind [`crate::Button`].
#[derive(Debug)]
pub(crate) struct ButtonWidget {
    line: TextLine,
    replayer: ImagingReplayer,
    is_default: bool,
    /// Space went down while focused: drawn pressed until it comes up.
    key_down: bool,
    pulse: Pulse,
}

impl ButtonWidget {
    pub(crate) fn new(label: String) -> Self {
        Self {
            line: TextLine::new(label, TextStyle::Body),
            replayer: ImagingReplayer::new(),
            is_default: false,
            key_down: false,
            pulse: Pulse::default(),
        }
    }

    pub(crate) fn set_default(&mut self, is_default: bool) {
        self.is_default = is_default;
    }

    pub(crate) fn edit_label(this: &mut WidgetMut<'_, Self>, label: String) {
        if this.widget.line.set_text(label) {
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }

    /// Gives it the default look and pulse, or takes them away (while a sheet's default
    /// button is in charge of Return).
    pub(crate) fn edit_default(this: &mut WidgetMut<'_, Self>, is_default: bool) {
        if this.widget.is_default == is_default {
            return;
        }
        this.widget.is_default = is_default;
        this.ctx.request_render();
        this.widget.start_pulse(&mut this.ctx);
    }

    /// The window gained or lost focus: the default look and the pulse follow it.
    pub(crate) fn edit_window_focus(this: &mut WidgetMut<'_, Self>) {
        this.ctx.request_render();
        this.widget.start_pulse(&mut this.ctx);
    }

    /// How far into its pulse the button is, from 0 to 1.
    #[cfg(test)]
    pub(crate) fn pulse(&self) -> f64 {
        self.pulse.amount()
    }

    /// Asks for animation frames if this is the default button; `on_anim_frame` decides
    /// whether it actually pulses.
    fn start_pulse(&mut self, ctx: &mut impl AnimCtx) {
        if self.is_default && !self.pulse.running {
            self.pulse.running = true;
            ctx.request_anim_frame();
        }
    }

    fn should_pulse(&self, ctx: &UpdateCtx<'_>, theme: &Theme) -> bool {
        self.is_default && theme.motion().ambient() && ctx.is_window_focused() && !ctx.is_disabled()
    }
}

/// Space presses a focused button. Return is the window's: it presses the default button.
fn is_press_key(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ")
}

impl UsesProperty<ThemeProp> for ButtonWidget {}

impl Widget for ButtonWidget {
    type Action = Pressed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            // Clicking doesn't take keyboard focus, as on the desktop this follows: focus
            // moves with Tab.
            PointerEvent::Down(..) => {
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(..) => {
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<Pressed>(Pressed);
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
            TextEvent::Keyboard(key) if is_press_key(&key.key) => {
                if key.state.is_down() {
                    self.key_down = true;
                } else if std::mem::take(&mut self.key_down) {
                    ctx.submit_action::<Pressed>(Pressed);
                }
                ctx.request_paint_only();
                ctx.set_handled();
            }
            // The focus ring shows only in the active window.
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
            ctx.submit_action::<Pressed>(Pressed);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => self.start_pulse(ctx),
            // Masonry's disabled pass doesn't repaint on its own.
            Update::DisabledChanged(_) => {
                ctx.request_paint_only();
                self.start_pulse(ctx);
            }
            Update::FocusChanged(focused) => {
                if !focused {
                    self.key_down = false;
                }
                ctx.request_paint_only();
            }
            // Hover and active change the pressed look.
            Update::HoveredChanged(_) | Update::ActiveChanged(_) => ctx.request_paint_only(),
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
            // Reduced motion may have been switched off.
            self.start_pulse(ctx);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        if self.should_pulse(ctx, &theme) {
            self.pulse.advance(Duration::from_nanos(interval));
            ctx.request_anim_frame();
        } else {
            // Paused (inactive window, disabled, reduced motion) at rest; `start_pulse`
            // resumes it.
            self.pulse = Pulse::default();
        }
        ctx.request_paint_only();
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
                (text.width + 2.0 * control::BUTTON_PADDING).max(control::BUTTON_MIN_WIDTH)
            }
            Axis::Vertical => control::REGULAR_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let text = self.line.size(ctx.text_contexts());
        let baseline = (size.height - text.height) / 2.0 + self.line.baseline();
        ctx.set_baselines(baseline, baseline);
        // The focus ring, shadow and etch draw outside the button.
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
        let state = if (ctx.is_active() && ctx.is_hovered()) || self.key_down {
            ButtonState::Pressed
        } else {
            ButtonState::Normal
        };
        let window_focused = ctx.is_window_focused();
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 blend amount")]
        let button = PushButton {
            state,
            default: self.is_default && window_focused,
            pulse: self.pulse.amount() as f32,
            disabled: ctx.is_disabled(),
            focused: ctx.is_focus_target() && window_focused,
        };
        let mut scene = Scene::new(size.width, size.height);
        push_button(
            &mut scene,
            Rect::from_origin_size((0.0, 0.0), size),
            button,
            theme.variant,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let colors = theme.colors();
        let color = if ctx.is_disabled() {
            colors.disabled_label
        } else {
            colors.label
        };
        let text = self.line.size(ctx.text_contexts());
        let origin = Vec2::new(
            ((size.width - text.width) / 2.0).round(),
            ((size.height - text.height) / 2.0).round(),
        );
        self.line.paint(painter, Affine::translate(origin), color);
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
        node.set_label(self.line.text());
        node.add_action(Action::Click);
        if self.is_default {
            node.set_keyboard_shortcut("Return");
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
