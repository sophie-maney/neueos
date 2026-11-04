//! A context menu area (task 2.17a, ADR 0045): wraps a child and opens a menu on a
//! secondary click inside it, on the Menu key or Shift-F10 while focus is inside it, or
//! when an assistive technology asks.

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Widget, WidgetId, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Size};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length};

use super::menu::{self, Chosen};
use crate::Cx;
use crate::element::{Element, Handle, Handler, Wiring};
use crate::menu::{ItemHandler, Menu, MenuSpec};

/// The Masonry widget behind [`crate::ContextMenu`].
#[derive(Debug)]
pub(crate) struct ContextMenuWidget {
    child: WidgetPod<dyn Widget>,
    spec: MenuSpec,
    return_focus: Option<WidgetId>,
}

impl ContextMenuWidget {
    fn new(child: NewWidget<dyn Widget>, spec: MenuSpec) -> Self {
        Self {
            child: child.to_pod(),
            spec,
            return_focus: None,
        }
    }

    fn open(&mut self, ctx: &mut EventCtx<'_>, at: Point, keyboard: bool) {
        menu::open(ctx, self.spec.clone(), at, keyboard, &mut self.return_focus);
    }

    /// Opened without a pointer: at the middle of the area.
    fn open_centred(&mut self, ctx: &mut EventCtx<'_>) {
        let at = ctx.to_window(ctx.border_box().center());
        self.open(ctx, at, true);
    }
}

impl Widget for ContextMenuWidget {
    type Action = Chosen;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Secondary),
            state,
            ..
        }) = event
        {
            let at = ctx.to_window(ctx.local_position(state.position));
            self.open(ctx, at, false);
            ctx.set_handled();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if !key.state.is_down() {
            return;
        }
        let menu_key = match &key.key {
            Key::Named(NamedKey::ContextMenu) => true,
            Key::Named(NamedKey::F10) => key.modifiers == Modifiers::SHIFT,
            _ => false,
        };
        if menu_key {
            self.open_centred(ctx);
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::ShowContextMenu {
            self.open_centred(ctx);
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let auto = match len_req {
            LenReq::MinContent => LenDef::MinContent,
            LenReq::MaxContent => LenDef::MaxContent,
            LenReq::FitContent(space) => LenDef::FitContent(space),
        };
        let context = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(&mut self.child, auto, context, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        let (first, last) = ctx.child_aligned_baselines(&self.child);
        ctx.set_baselines(first, last);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.add_action(Action::ShowContextMenu);
    }

    fn children_ids(&self) -> ChildrenIds {
        std::iter::once(self.child.id()).collect()
    }
}

/// Turns the chosen item's id into its message.
fn handler<M: 'static>(mut items: Vec<Option<ItemHandler<M>>>) -> Handler<M> {
    Box::new(move |action| {
        let Chosen(id) = *action.downcast_ref::<Chosen>()?;
        items.get_mut(id)?.as_mut().map(|make| make())
    })
}

/// `child` with a context menu: a secondary click inside it opens `menu` at the pointer;
/// the Menu key or Shift-F10, while keyboard focus is inside it, opens it with the first
/// item highlighted. Choosing an item sends its message.
///
/// ```no_run
/// # use neue_kit::{ContextMenu, Label, Menu, MenuItem, Shortcut};
/// # #[derive(Clone)] enum Msg { Copy }
/// let area = ContextMenu::new(
///     Label::new("Right-click me"),
///     Menu::new().with(MenuItem::new("Copy").shortcut(Shortcut::command("C")).on_choose(Msg::Copy)),
/// );
/// ```
pub struct ContextMenu<M> {
    inner: NewWidget<ContextMenuWidget>,
    wiring: Wiring<M>,
    items: Vec<Option<ItemHandler<M>>>,
}

impl<M> std::fmt::Debug for ContextMenu<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextMenu")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M: 'static> ContextMenu<M> {
    /// `child`, opening `menu` on a secondary click.
    pub fn new(child: impl Into<Element<M>>, menu: Menu<M>) -> Self {
        let mut wiring = Wiring::default();
        let child = wiring.adopt(child.into());
        let (spec, items) = menu.into_parts();
        Self {
            inner: NewWidget::new(ContextMenuWidget::new(child, spec)),
            wiring,
            items,
        }
    }

    /// Points `handle` at this area.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<ContextMenu<M>> for Element<M> {
    fn from(area: ContextMenu<M>) -> Self {
        let id = area.inner.id();
        let mut element = Element::with_wiring(area.inner.erased(), area.wiring);
        element.on_action(id, handler(area.items));
        element
    }
}

impl<M: 'static> Handle<ContextMenu<M>> {
    /// Replaces the menu (for the next time it opens): update check marks, enable or
    /// disable items.
    pub fn set_menu(&self, cx: &mut Cx<'_, M>, menu: Menu<M>) {
        let (Some(id), Some(window)) = (self.id(), cx.window_of(self)) else {
            return;
        };
        let (spec, items) = menu.into_parts();
        cx.edit::<ContextMenuWidget, _>(self, |w| w.widget.spec = spec);
        let mut wiring = Wiring::default();
        wiring.handlers.push((id, handler(items)));
        cx.register_wiring(window, wiring);
    }
}
