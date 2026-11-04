//! The widget facade: builders that apps use, over Masonry widgets they never see.
//!
//! A builder turns into an [`Element`] when it goes into a container or a window. Bind a
//! [`Handle`] to it to change it later from [`crate::Application::update`].

mod button;
mod label;
mod root;
mod stack;
mod text;
mod zoom;

use masonry_core::core::NewWidget;
use masonry_core::kurbo::Axis;
use neue_theme::metrics::spacing;

pub(crate) use button::{ButtonWidget, Pressed};
pub(crate) use label::LabelWidget;
pub(crate) use root::{RootAction, RootWidget};
pub use stack::Align;
pub(crate) use stack::StackWidget;
pub use text::TextStyle;
pub(crate) use zoom::ZoomWidget;

use crate::Cx;
use crate::element::{Element, Handle, Wiring};

/// A line of text.
///
/// ```no_run
/// # use neue_kit::{Handle, Label, TextStyle};
/// let mut value = Handle::default();
/// let label = Label::new("0").style(TextStyle::Display).bind(&mut value);
/// ```
#[derive(Debug)]
pub struct Label {
    inner: NewWidget<LabelWidget>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            inner: NewWidget::new(LabelWidget::new(text.into())),
        }
    }

    /// The size from the type scale. Body by default.
    #[must_use]
    pub fn style(mut self, style: TextStyle) -> Self {
        self.inner.widget.set_style(style);
        self
    }

    /// Secondary text colour (captions, status lines).
    #[must_use]
    pub fn secondary(mut self) -> Self {
        self.inner.widget.set_secondary(true);
        self
    }

    /// Points `handle` at this label.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<Label> for Element<M> {
    fn from(label: Label) -> Self {
        Element::new(label.inner.erased())
    }
}

impl Handle<Label> {
    /// Replaces the label's text.
    pub fn set_text<M>(&self, cx: &mut Cx<'_, M>, text: impl Into<String>) {
        let text = text.into();
        cx.edit::<LabelWidget, _>(self, |mut label| LabelWidget::edit_text(&mut label, text));
    }

    /// The label's text, if it's in an open window.
    pub fn text<M>(&self, cx: &mut Cx<'_, M>) -> Option<String> {
        cx.edit::<LabelWidget, _>(self, |label| label.widget.text().to_owned())
    }
}

/// A push button that sends a message when pressed: by pointer, by Space while it has
/// keyboard focus (Tab moves focus), by Return if it is the window's default button, or by
/// an assistive technology.
pub struct Button<M> {
    inner: NewWidget<ButtonWidget>,
    on_press: Option<Box<dyn FnMut() -> M>>,
    is_default: bool,
}

impl<M> std::fmt::Debug for Button<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("inner", &self.inner)
            .field("on_press", &self.on_press.is_some())
            .field("is_default", &self.is_default)
            .finish()
    }
}

impl<M: 'static> Button<M> {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            inner: NewWidget::new(ButtonWidget::new(label.into())),
            on_press: None,
            is_default: false,
        }
    }

    /// Makes this the window's default button: it shows the accent colour and pulses while
    /// the window is active, and Return presses it from anywhere in the window. Use it for
    /// the action that completes a dialog ("Save", "OK"); a window has at most one.
    #[must_use]
    pub fn as_default(mut self) -> Self {
        self.inner.widget.set_default(true);
        self.is_default = true;
        self
    }

    /// Sends `message` on every press.
    #[must_use]
    pub fn on_press(self, message: M) -> Self
    where
        M: Clone,
    {
        self.on_press_with(move || message.clone())
    }

    /// Sends the message `make` returns on every press.
    #[must_use]
    pub fn on_press_with(mut self, make: impl FnMut() -> M + 'static) -> Self {
        self.on_press = Some(Box::new(make));
        self
    }

    /// Starts disabled: greyed out and ignoring input.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this button.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<Button<M>> for Element<M> {
    fn from(button: Button<M>) -> Self {
        let id = button.inner.id();
        let mut element = Element::new(button.inner.erased());
        if let Some(mut make) = button.on_press {
            element.on_action(id, move |action| action.is::<Pressed>().then(&mut make));
        }
        if button.is_default {
            element.set_default_button(id);
        }
        element
    }
}

impl<M> Handle<Button<M>> {
    /// Replaces the button's title.
    pub fn set_label<N>(&self, cx: &mut Cx<'_, N>, label: impl Into<String>) {
        let label = label.into();
        cx.edit::<ButtonWidget, _>(self, |mut button| {
            ButtonWidget::edit_label(&mut button, label);
        });
    }

    /// Enables or disables the button.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ButtonWidget, _>(self, |mut button| button.ctx.set_disabled(!enabled));
    }
}

/// Children in a column or a row. Spacing defaults to the theme's small spacing, padding
/// to none.
pub struct Stack<M> {
    inner: NewWidget<StackWidget>,
    children: Vec<Element<M>>,
}

impl<M> std::fmt::Debug for Stack<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stack")
            .field("inner", &self.inner)
            .field("children", &self.children.len())
            .finish()
    }
}

impl<M> Stack<M> {
    /// Children top to bottom.
    pub fn column() -> Self {
        Self::new(Axis::Vertical)
    }

    /// Children left to right.
    pub fn row() -> Self {
        Self::new(Axis::Horizontal)
    }

    fn new(axis: Axis) -> Self {
        Self {
            inner: NewWidget::new(StackWidget::new(axis, spacing::S)),
            children: Vec::new(),
        }
    }

    /// Adds a child at the end.
    #[must_use]
    pub fn with(mut self, child: impl Into<Element<M>>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Space between children, in logical pixels (use `neue_kit::tokens::metrics::spacing`).
    #[must_use]
    pub fn spacing(mut self, spacing: f64) -> Self {
        self.inner.widget.spacing = spacing;
        self
    }

    /// Space around the children on every side, in logical pixels.
    #[must_use]
    pub fn padding(mut self, padding: f64) -> Self {
        self.inner.widget.padding = padding;
        self
    }

    /// Where children sit across the stack.
    #[must_use]
    pub fn align(mut self, align: Align) -> Self {
        self.inner.widget.align = align;
        self
    }

    /// Points `handle` at this stack.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<Stack<M>> for Element<M> {
    fn from(stack: Stack<M>) -> Self {
        let Stack {
            mut inner,
            children,
        } = stack;
        let mut wiring = Wiring::default();
        for child in children {
            inner.widget.push(wiring.adopt(child));
        }
        Element::with_wiring(inner.erased(), wiring)
    }
}

impl<M> Handle<Stack<M>> {
    /// Adds a child at the end.
    pub fn push(&self, cx: &mut Cx<'_, M>, child: impl Into<Element<M>>) {
        let (widget, wiring) = child.into().into_parts();
        let Some(window) = cx.window_of(self) else {
            tracing::debug!("pushing into a stack that isn't in an open window");
            return;
        };
        cx.edit::<StackWidget, _>(self, |mut stack| StackWidget::edit_push(&mut stack, widget));
        cx.register_wiring(window, wiring);
    }

    /// Removes every child.
    pub fn clear(&self, cx: &mut Cx<'_, M>) {
        if let Some(window) = cx.window_of(self) {
            cx.edit::<StackWidget, _>(self, |mut stack| StackWidget::edit_clear(&mut stack));
            cx.prune_handlers(window);
        }
    }
}

/// Draws its child at a multiple of its size (2.0: twice as large), with input still
/// landing where it's drawn. Vector content stays sharp at any factor.
pub struct Zoom<M> {
    inner: NewWidget<ZoomWidget>,
    wiring: Wiring<M>,
}

impl<M> std::fmt::Debug for Zoom<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Zoom")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M> Zoom<M> {
    /// `child` drawn at `factor` times its size.
    pub fn new(factor: f64, child: impl Into<Element<M>>) -> Self {
        let mut wiring = Wiring::default();
        let widget = wiring.adopt(child.into());
        Self {
            inner: NewWidget::new(ZoomWidget::new(widget, factor)),
            wiring,
        }
    }

    /// Points `handle` at this zoom.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<Zoom<M>> for Element<M> {
    fn from(zoom: Zoom<M>) -> Self {
        Element::with_wiring(zoom.inner.erased(), zoom.wiring)
    }
}

impl<M> Handle<Zoom<M>> {
    /// Changes the zoom factor.
    pub fn set_factor<N>(&self, cx: &mut Cx<'_, N>, factor: f64) {
        cx.edit::<ZoomWidget, _>(self, |mut zoom| ZoomWidget::edit_factor(&mut zoom, factor));
    }
}
