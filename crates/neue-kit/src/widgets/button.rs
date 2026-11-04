//! A push button: the Aqua gel with a text label. Interim look and behaviour; task 2.9
//! makes it the full push button (default pulse, focus ring, goldens).

use std::any::TypeId;

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget,
    WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{ButtonState, aqua_button};
use neue_theme::metrics::control;

use super::text::{TextLine, TextStyle};
use crate::theme::ThemeProp;

/// What a button emits when it's pressed (pointer, Space or Return, or an assistive
/// technology's click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pressed;

/// The Masonry widget behind [`crate::Button`].
#[derive(Debug)]
pub(crate) struct ButtonWidget {
    line: TextLine,
    replayer: ImagingReplayer,
}

impl ButtonWidget {
    pub(crate) fn new(label: String) -> Self {
        Self {
            line: TextLine::new(label, TextStyle::Body),
            replayer: ImagingReplayer::new(),
        }
    }

    pub(crate) fn edit_label(this: &mut WidgetMut<'_, Self>, label: String) {
        if this.widget.line.set_text(label) {
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }
}

/// Space and Return press a focused button.
fn is_press_key(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ") || *key == Key::Named(NamedKey::Enter)
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
            PointerEvent::Down(..) => {
                ctx.request_focus();
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
        if let TextEvent::Keyboard(key) = event
            && key.state.is_up()
            && is_press_key(&key.key)
        {
            ctx.submit_action::<Pressed>(Pressed);
            ctx.set_handled();
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
            // Masonry's disabled pass doesn't repaint on its own; hover and active change
            // the pressed look.
            Update::DisabledChanged(_)
            | Update::HoveredChanged(_)
            | Update::ActiveChanged(_)
            | Update::FocusChanged(_) => ctx.request_paint_only(),
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
        }
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
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let state = if ctx.is_active() && ctx.is_hovered() {
            ButtonState::Pressed
        } else {
            ButtonState::Normal
        };
        let mut scene = Scene::new(size.width, size.height);
        aqua_button(
            &mut scene,
            Rect::from_origin_size((0.0, 0.0), size),
            state,
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
