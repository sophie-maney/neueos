//! Elements (built widgets with their message handlers) and handles to them.

use std::marker::PhantomData;

use masonry_core::core::{ErasedAction, NewWidget, Widget, WidgetId};

/// Turns a widget's action into the app's message, or ignores it.
pub(crate) type Handler<M> = Box<dyn FnMut(ErasedAction) -> Option<M>>;

/// Handlers keyed by the widget whose actions they map.
pub(crate) type Handlers<M> = Vec<(WidgetId, Handler<M>)>;

/// A built widget, ready to go into a container or a window, together with its
/// [`Wiring`]: the handlers that turn its (and its children's) actions into messages of
/// type `M`, and the default button among them. Every widget builder converts into one.
pub struct Element<M> {
    widget: NewWidget<dyn Widget>,
    wiring: Wiring<M>,
}

impl<M> std::fmt::Debug for Element<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Element")
            .field("widget", &self.widget)
            .field("wiring", &self.wiring)
            .finish()
    }
}

impl<M> Element<M> {
    pub(crate) fn new(widget: NewWidget<dyn Widget>) -> Self {
        Self::with_wiring(widget, Wiring::default())
    }

    /// A container `widget` whose children's wiring was gathered into `wiring`.
    pub(crate) fn with_wiring(widget: NewWidget<dyn Widget>, wiring: Wiring<M>) -> Self {
        Self { widget, wiring }
    }

    /// Maps actions of the widget `id` (this one or a descendant) to messages.
    pub(crate) fn on_action(
        &mut self,
        id: WidgetId,
        handler: impl FnMut(ErasedAction) -> Option<M> + 'static,
    ) {
        self.wiring.handlers.push((id, Box::new(handler)));
    }

    /// Marks the widget `id` (this one or a descendant) as the default button.
    pub(crate) fn set_default_button(&mut self, id: WidgetId) {
        self.wiring.default_button = Some(id);
    }

    pub(crate) fn into_parts(self) -> (NewWidget<dyn Widget>, Wiring<M>) {
        (self.widget, self.wiring)
    }
}

/// What an [`Element`] carries besides its widget: message handlers keyed by widget, and
/// the default button (Return presses it) in its subtree.
pub(crate) struct Wiring<M> {
    pub(crate) handlers: Handlers<M>,
    pub(crate) default_button: Option<WidgetId>,
}

impl<M> Default for Wiring<M> {
    fn default() -> Self {
        Self {
            handlers: Vec::new(),
            default_button: None,
        }
    }
}

impl<M> std::fmt::Debug for Wiring<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wiring")
            .field("handlers", &self.handlers.len())
            .field("default_button", &self.default_button)
            .finish()
    }
}

impl<M> Wiring<M> {
    /// Takes over `child`'s wiring and returns its widget, for a container to hold. A
    /// window has one default button; the first one wins.
    pub(crate) fn adopt(&mut self, child: Element<M>) -> NewWidget<dyn Widget> {
        self.handlers.extend(child.wiring.handlers);
        if let Some(id) = child.wiring.default_button {
            if self.default_button.is_some() {
                tracing::warn!(%id, "a second default button in one window is ignored");
            } else {
                self.default_button = Some(id);
            }
        }
        child.widget
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
