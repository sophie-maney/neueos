//! Every window's root: fills the window with its theme's background, and turns window-wide
//! keys into actions neue-kit's runner handles (ADR 0037).
//!
//! It is the window's focus fallback, so it sees keys while nothing has focus; keys a
//! focused widget leaves unhandled bubble up to it as well.

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, UpdateCtx, UsesProperty, Widget, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, Size};
use masonry_core::layout::{LayoutSize, LenReq, Length};

use crate::theme::ThemeProp;

/// What a window root reports to the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootAction {
    /// Return (or keypad Enter) went unhandled: press the window's default button.
    DefaultKey,
    /// The window gained or lost keyboard focus: its default button changes look.
    WindowFocus(bool),
}

/// The Masonry widget at the root of every neue-kit window.
#[derive(Debug)]
pub(crate) struct RootWidget {
    child: WidgetPod<dyn Widget>,
}

impl RootWidget {
    pub(crate) fn new(child: NewWidget<dyn Widget>) -> Self {
        Self {
            child: child.to_pod(),
        }
    }
}

impl UsesProperty<ThemeProp> for RootWidget {}

impl Widget for RootWidget {
    type Action = RootAction;

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key)
                if key.state.is_down()
                    && !key.repeat
                    && key.key == Key::Named(NamedKey::Enter)
                    && !key.modifiers.ctrl()
                    && !key.modifiers.alt()
                    && !key.modifiers.meta() =>
            {
                ctx.submit_action::<RootAction>(RootAction::DefaultKey);
                ctx.set_handled();
            }
            TextEvent::WindowFocusChange(focused) => {
                ctx.submit_action::<RootAction>(RootAction::WindowFocus(*focused));
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_paint_only();
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
        let context = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(&mut self.child, len_req.into(), context, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let rect = Rect::from_origin_size(Point::ORIGIN, ctx.content_box().size());
        painter.fill_rect(rect, theme.colors().window_background);
    }

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
        std::iter::once(self.child.id()).collect()
    }
}
