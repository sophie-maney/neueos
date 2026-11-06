//! The combo box (task 2.17b, ADR 0046): a text field with an arrow button inside its right
//! end. The arrow, or Down in the field, opens a menu of suggestions as wide as the field;
//! choosing one puts its title in the field.

use std::sync::Arc;

use masonry_core::core::{Widget, WidgetMut};

use super::{Combo, FieldAction, FieldWidget, TextField};
use crate::element::{Element, Handle};
use crate::menu::{ItemSpec, MenuSpec, SpecKind};
use crate::{Cx, menu::MenuToggle};

/// The owner's menu closed (`chosen`: the item, if one was chosen).
pub(super) fn closed(mut owner: WidgetMut<'_, dyn Widget>, chosen: Option<usize>) {
    let mut this = owner.downcast::<FieldWidget>();
    let title = this.widget.combo.as_mut().and_then(|combo| {
        combo.open = false;
        chosen.and_then(|i| combo.labels.get(i).cloned())
    });
    if let Some(title) = title {
        FieldWidget::edit_text(&mut this, &title);
    }
    if let Some(i) = chosen {
        this.ctx
            .submit_action::<FieldAction>(FieldAction::Chosen(i));
    }
    this.ctx.request_paint_only();
    this.ctx.request_accessibility_update();
}

/// A text field with a menu of suggestions: type a value of your own, or open the list
/// (the arrow button, or Down) and choose one. Sends what a [`TextField`] sends as the
/// user types, and `on_select` (the index and the title) when they choose from the list;
/// choosing sends no text change, so an app that follows the text listens to both.
///
/// ```no_run
/// # use neue_kit::ComboBox;
/// # #[derive(Clone)] enum Msg { Size(String), Picked(usize) }
/// let size = ComboBox::new(["Small", "Medium", "Large"])
///     .on_change(Msg::Size)
///     .on_select(|index, _title| Msg::Picked(index));
/// ```
pub struct ComboBox<M> {
    field: TextField<M>,
    labels: Vec<String>,
    on_select: Option<Box<dyn FnMut(usize, String) -> M>>,
}

impl<M> std::fmt::Debug for ComboBox<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComboBox")
            .field("field", &self.field)
            .field("items", &self.labels.len())
            .finish_non_exhaustive()
    }
}

/// A plain list of `labels`.
fn suggestions(labels: &[String]) -> MenuSpec {
    MenuSpec(
        labels
            .iter()
            .enumerate()
            .map(|(id, label)| ItemSpec {
                id,
                label: label.clone(),
                shortcut: None,
                signal: None,
                enabled: true,
                toggle: MenuToggle::None,
                kind: SpecKind::Action,
            })
            .collect::<Arc<[_]>>(),
    )
}

impl<M: 'static> ComboBox<M> {
    /// A combo box suggesting `labels`, with an empty field.
    pub fn new<S: Into<String>>(labels: impl IntoIterator<Item = S>) -> Self {
        let labels: Vec<String> = labels.into_iter().map(Into::into).collect();
        let mut field = TextField::new();
        field.inner.widget.combo = Some(Combo {
            spec: suggestions(&labels),
            labels: labels.clone(),
            open: false,
            return_focus: None,
        });
        Self {
            field,
            labels,
            on_select: None,
        }
    }

    /// Starts with `text`, the caret at its end.
    #[must_use]
    pub fn text(mut self, text: &str) -> Self {
        self.field = self.field.text(text);
        self
    }

    /// Grey text shown while the field is empty.
    #[must_use]
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.field = self.field.placeholder(text);
        self
    }

    /// Width in logical pixels, the arrow button included
    /// ([`neue_theme::metrics::control::TEXT_FIELD_WIDTH`] by default).
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.field = self.field.width(width);
        self
    }

    /// Sends the message `make` returns, given the whole text, each time the user types.
    #[must_use]
    pub fn on_change(mut self, make: impl FnMut(String) -> M + 'static) -> Self {
        self.field = self.field.on_change(make);
        self
    }

    /// Sends the message `make` returns, given the text, when the user presses Return.
    #[must_use]
    pub fn on_submit(mut self, make: impl FnMut(String) -> M + 'static) -> Self {
        self.field = self.field.on_submit(make);
        self
    }

    /// Sends the message `make` returns, given the index and the title of the suggestion
    /// the user chose. The field then holds that title.
    #[must_use]
    pub fn on_select(mut self, make: impl FnMut(usize, String) -> M + 'static) -> Self {
        self.on_select = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.field = self.field.disabled(disabled);
        self
    }

    /// Points `handle` at this combo box.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.field.inner.id());
        self
    }
}

impl<M: 'static> From<ComboBox<M>> for Element<M> {
    fn from(combo: ComboBox<M>) -> Self {
        let ComboBox {
            field,
            labels,
            mut on_select,
        } = combo;
        let TextField {
            inner,
            mut on_change,
            mut on_submit,
        } = field;
        let id = inner.id();
        let mut element = Element::new(inner.erased());
        // One handler per widget: the field's own messages and the choice go through it.
        element.on_action(id, move |action| {
            match action.downcast_ref::<FieldAction>()? {
                FieldAction::Changed(text) => on_change.as_mut().map(|f| f(text.clone())),
                FieldAction::Submitted(text) => on_submit.as_mut().map(|f| f(text.clone())),
                FieldAction::Chosen(i) => {
                    let title = labels.get(*i)?.clone();
                    on_select.as_mut().map(|f| f(*i, title))
                }
                FieldAction::Selected(_) => None,
            }
        });
        element
    }
}

impl<M> Handle<ComboBox<M>> {
    /// Replaces the text, caret at the end. Sends no message.
    pub fn set_text<N>(&self, cx: &mut Cx<'_, N>, text: &str) {
        cx.edit::<FieldWidget, _>(self, |mut w| FieldWidget::edit_text(&mut w, text));
    }

    /// The field's text, if it's in an open window.
    pub fn text<N>(&self, cx: &mut Cx<'_, N>) -> Option<String> {
        cx.edit::<FieldWidget, _>(self, |w| w.widget.text())
    }

    /// Enables or disables the combo box.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<FieldWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
