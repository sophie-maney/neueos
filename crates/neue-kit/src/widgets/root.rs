//! Every window's root: fills the window with its theme's background, draws its chrome if
//! it has some (ADR 0055), and turns window-wide keys into actions neue-kit's runner handles
//! (ADR 0037).
//!
//! It is the window's focus fallback, so it sees keys while nothing has focus; keys a
//! focused widget leaves unhandled bubble up to it as well.

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
    UsesProperty, Widget, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, Size};
use masonry_core::layout::{LayoutSize, LenReq, Length};

use super::chrome::Chrome;
use crate::theme::ThemeProp;

/// What a window root reports to the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootAction {
    /// Return (or keypad Enter) went unhandled: press the window's default button.
    DefaultKey,
    /// The window gained or lost keyboard focus: its default button changes look.
    WindowFocus(bool),
    /// The chrome's close button was clicked: the app's close request.
    Close,
}

/// The Masonry widget at the root of every neue-kit window.
#[derive(Debug)]
pub(crate) struct RootWidget {
    child: WidgetPod<dyn Widget>,
    chrome: Option<Chrome>,
}

impl RootWidget {
    pub(crate) fn new(child: NewWidget<dyn Widget>) -> Self {
        Self {
            child: child.to_pod(),
            chrome: None,
        }
    }

    /// A root that draws its own chrome above `child`.
    pub(crate) fn with_chrome(child: NewWidget<dyn Widget>, chrome: Chrome) -> Self {
        Self {
            child: child.to_pod(),
            chrome: Some(chrome),
        }
    }

    #[cfg(test)]
    pub(crate) fn chrome(&self) -> Option<&Chrome> {
        self.chrome.as_ref()
    }
}

impl UsesProperty<ThemeProp> for RootWidget {}

impl Widget for RootWidget {
    type Action = RootAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let Some(chrome) = &mut self.chrome {
            chrome.on_pointer_event(ctx, event);
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if let Some(chrome) = &mut self.chrome {
            chrome.on_access_event(ctx, event);
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
                if self.chrome.is_some() {
                    ctx.request_paint_only();
                }
            }
            _ => {}
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        if let Some(chrome) = &mut self.chrome {
            chrome.register_children(ctx);
        }
        ctx.register_child(&mut self.child);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let (Update::HoveredChanged(false), Some(chrome)) = (event, &mut self.chrome)
            && !ctx.has_hovered()
            && chrome.unhover()
        {
            ctx.request_paint_only();
        }
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
        let content =
            ctx.compute_length(&mut self.child, len_req.into(), context, axis, cross_length);
        let Some(chrome) = &mut self.chrome else {
            return content;
        };
        let bar = chrome.measure(ctx, axis, len_req, cross_length);
        match axis {
            Axis::Horizontal => content.max(bar),
            Axis::Vertical => content.saturating_add(bar),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let top = match &mut self.chrome {
            Some(chrome) => chrome.layout(ctx, size.width, size.height),
            None => 0.0,
        };
        let content = Size::new(size.width, (size.height - top).max(0.0));
        ctx.run_layout(&mut self.child, content);
        ctx.place_child(&mut self.child, Point::new(0.0, top));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let top = self.chrome.as_ref().map_or(0.0, Chrome::height);
        let rect = Rect::new(0.0, top, size.width, size.height);
        painter.fill_rect(rect, theme.colors().window_background);
        if let Some(chrome) = &mut self.chrome {
            let focused = ctx.is_window_focused();
            chrome.paint(painter, theme, focused, size.width, ctx.scale_factor());
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if let Some(chrome) = &self.chrome {
            node.set_label(chrome.title());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        // The chrome first: reading order is the window controls, the toolbar, the content.
        self.chrome
            .iter()
            .flat_map(Chrome::children_ids)
            .chain(std::iter::once(self.child.id()))
            .collect()
    }
}
