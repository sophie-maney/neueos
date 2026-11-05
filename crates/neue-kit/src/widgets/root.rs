//! Every window's root: fills the window with its theme's background, draws its chrome if
//! it has some (ADR 0055), and turns window-wide keys into actions neue-kit's runner handles
//! (ADR 0037).
//!
//! While a sheet is open on the window (ADR 0057), the root lays a scrim over its content,
//! under the chrome, that takes the pointer's events so the content gets none. Where the
//! sheet is not a window of its own, the root also draws it, centred under the chrome.
//!
//! It is the window's focus fallback, so it sees keys while nothing has focus; keys a
//! focused widget leaves unhandled bubble up to it as well.

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, UsesProperty, Widget, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, Size};
use masonry_core::layout::{LayoutSize, LenReq, Length};
use neue_platform::ChromeHeight;

use super::chrome::Chrome;
use super::sheet::SheetWidget;
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
    /// Where the chrome ends, for the driver to tell the compositor (ADR 0064).
    chrome_height: Option<ChromeHeight>,
    sheet: Option<Attached>,
}

/// A sheet open on the window: the scrim over the content, and the sheet itself unless it
/// is a window of its own.
#[derive(Debug)]
struct Attached {
    scrim: WidgetPod<Scrim>,
    panel: Option<(WidgetPod<SheetWidget>, Size)>,
}

impl RootWidget {
    pub(crate) fn new(child: NewWidget<dyn Widget>) -> Self {
        Self {
            child: child.to_pod(),
            chrome: None,
            chrome_height: None,
            sheet: None,
        }
    }

    /// A root that draws its own chrome above `child`.
    pub(crate) fn with_chrome(child: NewWidget<dyn Widget>, chrome: Chrome) -> Self {
        Self {
            child: child.to_pod(),
            chrome: Some(chrome),
            chrome_height: Some(ChromeHeight::new()),
            sheet: None,
        }
    }

    /// Where the chrome ends, as of the last layout, shared with the driver; `None` without
    /// chrome. The Wayland driver tells the compositor, which hangs sheets under it.
    pub(crate) fn chrome_height(&self) -> Option<ChromeHeight> {
        self.chrome_height.clone()
    }

    /// Blocks pointer input to the content for a sheet, and draws `panel` (a sheet that
    /// isn't a window of its own) under the chrome. Replaces a sheet already there.
    pub(crate) fn attach_sheet(
        this: &mut WidgetMut<'_, Self>,
        panel: Option<NewWidget<SheetWidget>>,
    ) {
        Self::detach_sheet(this);
        let panel = panel.map(|mut panel| {
            panel.widget.set_attached(true);
            let size = panel.widget.size();
            (panel.to_pod(), size)
        });
        this.widget.sheet = Some(Attached {
            scrim: NewWidget::new(Scrim).to_pod(),
            panel,
        });
        this.ctx.children_changed();
    }

    /// Removes the sheet's scrim and panel, if there are any.
    pub(crate) fn detach_sheet(this: &mut WidgetMut<'_, Self>) {
        if let Some(sheet) = this.widget.sheet.take() {
            this.ctx.remove_child(sheet.scrim);
            if let Some((panel, _)) = sheet.panel {
                this.ctx.remove_child(panel);
            }
        }
    }

    /// Paints the focused look while a HUD window of this one has the keyboard, or stops.
    pub(crate) fn set_keep_active(this: &mut WidgetMut<'_, Self>, keep: bool) {
        if let Some(chrome) = &mut this.widget.chrome
            && chrome.set_keep_active(keep)
        {
            this.ctx.request_paint_only();
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
        if let Some(sheet) = &mut self.sheet {
            ctx.register_child(&mut sheet.scrim);
            if let Some((panel, _)) = &mut sheet.panel {
                ctx.register_child(panel);
            }
        }
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
        if let Some(height) = &self.chrome_height {
            height.set(top);
        }
        let content = Size::new(size.width, (size.height - top).max(0.0));
        ctx.run_layout(&mut self.child, content);
        ctx.place_child(&mut self.child, Point::new(0.0, top));
        if let Some(sheet) = &mut self.sheet {
            ctx.run_layout(&mut sheet.scrim, content);
            ctx.place_child(&mut sheet.scrim, Point::new(0.0, top));
            if let Some((panel, wanted)) = &mut sheet.panel {
                // As asked, but no wider or taller than the content area; centred, its top
                // edge under the chrome.
                let fit = Size::new(
                    wanted.width.min(content.width),
                    wanted.height.min(content.height),
                );
                ctx.run_layout(panel, fit);
                let x = ((size.width - fit.width) / 2.0).round();
                ctx.place_child(panel, Point::new(x, top));
            }
        }
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
        // A HUD window's panel is the chrome's, over the whole window.
        if self.chrome.as_ref().is_none_or(|chrome| !chrome.is_hud()) {
            painter.fill_rect(rect, theme.colors().window_background);
        }
        if let Some(chrome) = &mut self.chrome {
            let focused = ctx.is_window_focused();
            chrome.paint(painter, theme, focused, size, ctx.scale_factor());
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
        // The sheet last: it is on top.
        self.chrome
            .iter()
            .flat_map(Chrome::children_ids)
            .chain(std::iter::once(self.child.id()))
            .chain(self.sheet.iter().flat_map(|sheet| {
                std::iter::once(sheet.scrim.id())
                    .chain(sheet.panel.as_ref().map(|(panel, _)| panel.id()))
            }))
            .collect()
    }
}

/// An invisible widget over a window's content while a sheet is open: the pointer's events
/// land on it instead of on the content.
#[derive(Debug)]
pub(crate) struct Scrim;

impl Widget for Scrim {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::ZERO
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

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
        ChildrenIds::new()
    }
}
