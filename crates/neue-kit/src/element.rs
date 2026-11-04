//! Elements (built widgets with their message handlers) and handles to them.

use std::marker::PhantomData;

use masonry_core::core::{ErasedAction, NewWidget, Widget, WidgetId};

/// Turns a widget's action into the app's message, or ignores it.
pub(crate) type Handler<M> = Box<dyn FnMut(ErasedAction) -> Option<M>>;

/// Handlers keyed by the widget whose actions they map.
pub(crate) type Handlers<M> = Vec<(WidgetId, Handler<M>)>;

/// A built widget, ready to go into a container or a window, together with the handlers
/// that turn its (and its children's) actions into messages of type `M`. Every widget
/// builder converts into one.
pub struct Element<M> {
    widget: NewWidget<dyn Widget>,
    handlers: Handlers<M>,
}

impl<M> std::fmt::Debug for Element<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Element")
            .field("widget", &self.widget)
            .field("handlers", &self.handlers.len())
            .finish()
    }
}

impl<M> Element<M> {
    pub(crate) fn new(widget: NewWidget<dyn Widget>) -> Self {
        Self {
            widget,
            handlers: Vec::new(),
        }
    }

    /// Maps actions of the widget `id` (this one or a descendant) to messages.
    pub(crate) fn on_action(
        &mut self,
        id: WidgetId,
        handler: impl FnMut(ErasedAction) -> Option<M> + 'static,
    ) {
        self.handlers.push((id, Box::new(handler)));
    }

    pub(crate) fn extend_handlers(&mut self, handlers: Handlers<M>) {
        self.handlers.extend(handlers);
    }

    pub(crate) fn into_parts(self) -> (NewWidget<dyn Widget>, Handlers<M>) {
        (self.widget, self.handlers)
    }
}

/// Refers to a widget after it went into a window, so `update` can change it. Starts
/// unbound; a builder's `bind` points it at that widget. Methods on an unbound handle, or
/// on one whose widget isn't in an open window (yet, or any more), do nothing.
///
/// `W` is the builder type: `Handle<Label>` has `set_text`, `Handle<Button<M>>` has
/// `set_label` and `set_enabled`, and so on.
pub struct Handle<W> {
    id: Option<WidgetId>,
    kind: PhantomData<fn() -> W>,
}

impl<W> Handle<W> {
    pub(crate) fn new(id: WidgetId) -> Self {
        Self {
            id: Some(id),
            kind: PhantomData,
        }
    }

    pub(crate) fn id(&self) -> Option<WidgetId> {
        self.id
    }

    /// Whether a builder's `bind` pointed this handle at a widget.
    pub fn is_bound(&self) -> bool {
        self.id.is_some()
    }
}

impl<W> Default for Handle<W> {
    fn default() -> Self {
        Self {
            id: None,
            kind: PhantomData,
        }
    }
}

impl<W> Clone for Handle<W> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<W> Copy for Handle<W> {}

impl<W> PartialEq for Handle<W> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<W> Eq for Handle<W> {}

impl<W> std::fmt::Debug for Handle<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = std::any::type_name::<W>();
        let kind = kind.rsplit("::").next().unwrap_or(kind);
        match self.id {
            Some(id) => write!(f, "Handle<{kind}>({id})"),
            None => write!(f, "Handle<{kind}>(unbound)"),
        }
    }
}
