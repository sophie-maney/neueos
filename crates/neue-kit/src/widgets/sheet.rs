//! Sheets (task 2.23b, ADR 0057): a modal panel attached to a window, with no title bar of
//! its own, that comes out from under the window's title bar. It holds the app's content
//! and a row of buttons in the bottom right corner: Return presses the default button,
//! Escape the cancel button.
//!
//! The sheet widget is the same everywhere; only its host differs. On Wayland it is the
//! root of a modal child window (xdg-dialog), which the compositor places on its parent.
//! Where the driver has no modal windows (winit on macOS, and the kit's tests), the parent
//! window's root draws it over its content, under its chrome, and blocks pointer input to
//! the content while it is open ([`super::root`]).
//!
//! Tab and Shift-Tab cycle through the sheet's controls without leaving it: the sheet hands
//! them to the runner ([`SheetKey::Tab`]), which can see the whole subtree.

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId,
    WidgetPod, WidgetRef,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Point, Size};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{sheet, sheet_shadow};
use neue_paint::{Backdrop, Scene};
use neue_theme::materials::sheet::{BUTTON_GAP, BUTTON_ROW_GAP, PADDING, SHADOW_REACH};

use crate::element::{Element, Handle, Wiring};
use crate::theme::ThemeProp;
use crate::{Button, Stack};

/// A modal panel attached to a window: the app's content above a row of buttons. Show it
/// with [`Cx::show_sheet`](crate::Cx::show_sheet) and close it with
/// [`Cx::close_sheet`](crate::Cx::close_sheet); its buttons send messages but don't close
/// it, so the app can keep it open (to point at a mistake, say).
///
/// ```no_run
/// # use neue_kit::{Cx, Label, Sheet, WindowId};
/// # #[derive(Clone)] enum Msg { Save, Cancel }
/// # fn update(cx: &mut Cx<'_, Msg>, window: WindowId) {
/// cx.show_sheet(
///     window,
///     Sheet::new((360, 120), Label::new("Save changes to “Notes”?"))
///         .default_button("Save", Msg::Save)
///         .cancel_button("Cancel", Msg::Cancel),
/// );
/// # }
/// ```
pub struct Sheet<M> {
    size: (u32, u32),
    content: Element<M>,
    title: String,
    default: Option<(String, M)>,
    cancel: Option<(String, M)>,
}

impl<M> std::fmt::Debug for Sheet<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sheet")
            .field("size", &self.size)
            .field("title", &self.title)
            .field("default", &self.default.as_ref().map(|(l, _)| l))
            .field("cancel", &self.cancel.as_ref().map(|(l, _)| l))
            .finish_non_exhaustive()
    }
}

impl<M: Clone + 'static> Sheet<M> {
    /// A sheet of `size` logical pixels (or narrower, on a narrower window) showing
    /// `content`, with no buttons yet.
    pub fn new(size: (u32, u32), content: impl Into<Element<M>>) -> Self {
        Self {
            size,
            content: content.into(),
            title: String::new(),
            default: None,
            cancel: None,
        }
    }

    /// What assistive technologies call the sheet (and, on Wayland, its window's title).
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// The button that completes the sheet ("Save", "OK"): rightmost, in the accent
    /// colour, pressed by Return. Without one, a button in the content made with
    /// [`Button::as_default`] takes its place.
    #[must_use]
    pub fn default_button(mut self, label: impl Into<String>, message: M) -> Self {
        self.default = Some((label.into(), message));
        self
    }

    /// The button that dismisses the sheet ("Cancel"): left of the default button,
    /// pressed by Escape.
    #[must_use]
    pub fn cancel_button(mut self, label: impl Into<String>, message: M) -> Self {
        self.cancel = Some((label.into(), message));
        self
    }

    /// The sheet's widget, the wiring of its content and buttons (with no window default
    /// button: the sheet handles Return itself), and its keys' buttons.
    pub(crate) fn into_parts(self) -> SheetParts<M> {
        let mut wiring = Wiring::default();
        let content = wiring.adopt(self.content);
        let mut keys = SheetButtons {
            default: wiring.default_button.take(),
            cancel: None,
        };
        let mut row = Stack::row().spacing(BUTTON_GAP);
        let mut has_buttons = false;
        if let Some((label, message)) = self.cancel {
            let mut handle = Handle::default();
            row = row.with(Button::new(label).on_press(message).bind(&mut handle));
            keys.cancel = handle.id();
            has_buttons = true;
        }
        if let Some((label, message)) = self.default {
            let mut handle = Handle::default();
            row = row.with(
                Button::new(label)
                    .as_default()
                    .on_press(message)
                    .bind(&mut handle),
            );
            if let Some(content_default) = keys.default {
                tracing::warn!(%content_default, "the sheet's default button replaces the content's");
            }
            keys.default = handle.id();
            has_buttons = true;
        }
        let buttons = has_buttons.then(|| {
            let widget = wiring.adopt(row.into());
            // The row's default button is the sheet's, not the window's.
            wiring.default_button = None;
            widget
        });
        let size = Size::new(f64::from(self.size.0), f64::from(self.size.1));
        let widget = NewWidget::new(SheetWidget::new(content, buttons, size, self.title.clone()));
        SheetParts {
            widget,
            wiring,
            keys,
            title: self.title,
            size: self.size,
        }
    }
}

/// A sheet taken apart for its host.
pub(crate) struct SheetParts<M> {
    pub(crate) widget: NewWidget<SheetWidget>,
    pub(crate) wiring: Wiring<M>,
    pub(crate) keys: SheetButtons,
    pub(crate) title: String,
    pub(crate) size: (u32, u32),
}

/// The buttons a sheet's keys press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SheetButtons {
    /// Return.
    pub(crate) default: Option<WidgetId>,
    /// Escape.
    pub(crate) cancel: Option<WidgetId>,
}

/// A sheet's action: a key the runner acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SheetKey {
    /// Return: press the default button.
    Default,
    /// Escape: press the cancel button.
    Cancel,
    /// Tab (`forward`) or Shift-Tab: move focus to the next or previous control in the
    /// sheet, wrapping around.
    Tab { forward: bool },
}

/// The Masonry widget of a sheet: the panel, the content, and the button row.
pub(crate) struct SheetWidget {
    content: WidgetPod<dyn Widget>,
    buttons: Option<WidgetPod<dyn Widget>>,
    /// The size the app asked for.
    size: Size,
    title: String,
    /// Drawn over its parent window's content (not a window of its own): it casts a
    /// shadow.
    attached: bool,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for SheetWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SheetWidget")
            .field("size", &self.size)
            .field("title", &self.title)
            .field("attached", &self.attached)
            .finish_non_exhaustive()
    }
}

impl SheetWidget {
    fn new(
        content: NewWidget<dyn Widget>,
        buttons: Option<NewWidget<dyn Widget>>,
        size: Size,
        title: String,
    ) -> Self {
        Self {
            content: content.to_pod(),
            buttons: buttons.map(NewWidget::to_pod),
            size,
            title,
            attached: false,
            replayer: ImagingReplayer::new(),
        }
    }

    /// The size the app asked for.
    pub(crate) fn size(&self) -> Size {
        self.size
    }

    /// Marks it as drawn over its parent's content, before it goes into the tree.
    pub(crate) fn set_attached(&mut self, attached: bool) {
        self.attached = attached;
    }
}

/// The widgets in `sheet` that Tab stops at, in Tab order.
pub(crate) fn focusable(sheet: WidgetRef<'_, dyn Widget>) -> Vec<WidgetId> {
    fn walk(widget: WidgetRef<'_, dyn Widget>, out: &mut Vec<WidgetId>) {
        let ctx = widget.ctx();
        if ctx.is_stashed() || ctx.is_disabled() {
            return;
        }
        if widget.accepts_focus() {
            out.push(widget.id());
        }
        for child in widget.children() {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(sheet, &mut out);
    out
}

/// Whether `key` is Return or Enter without a command modifier.
fn is_return(key: &masonry_core::core::keyboard::KeyboardEvent) -> bool {
    key.key == Key::Named(NamedKey::Enter)
        && !key.repeat
        && !key.modifiers.ctrl()
        && !key.modifiers.alt()
        && !key.modifiers.meta()
}

impl UsesProperty<ThemeProp> for SheetWidget {}

impl Widget for SheetWidget {
    type Action = SheetKey;

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
        let action = match &key.key {
            _ if is_return(key) => SheetKey::Default,
            Key::Named(NamedKey::Escape) if !key.repeat => SheetKey::Cancel,
            Key::Named(NamedKey::Tab)
                if !key.modifiers.ctrl() && !key.modifiers.alt() && !key.modifiers.meta() =>
            {
                SheetKey::Tab {
                    forward: !key.modifiers.shift(),
                }
            }
            _ => return,
        };
        ctx.submit_action::<SheetKey>(action);
        ctx.set_handled();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.content);
        if let Some(buttons) = &mut self.buttons {
            ctx.register_child(buttons);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::FocusChanged(_) | Update::ChildFocusChanged(_) = event {
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
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(match axis {
            Axis::Horizontal => self.size.width,
            Axis::Vertical => self.size.height,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let inner = Size::new(
            (size.width - 2.0 * PADDING).max(0.0),
            (size.height - 2.0 * PADDING).max(0.0),
        );
        let mut content = inner;
        if let Some(buttons) = &mut self.buttons {
            let def = SizeDef::new(LenDef::MaxContent, LenDef::MaxContent);
            let row = ctx.compute_size(buttons, def, LayoutSize::NONE);
            let row = Size::new(row.width.min(inner.width), row.height.min(inner.height));
            ctx.run_layout(buttons, row);
            let at = Point::new(
                size.width - PADDING - row.width,
                size.height - PADDING - row.height,
            );
            ctx.place_child(buttons, at);
            content.height = (inner.height - row.height - BUTTON_ROW_GAP).max(0.0);
        }
        ctx.run_layout(&mut self.content, content);
        ctx.place_child(&mut self.content, Point::new(PADDING, PADDING));
        ctx.clear_baselines();
        if self.attached {
            ctx.set_paint_insets(Insets::new(SHADOW_REACH, 0.0, SHADOW_REACH, SHADOW_REACH));
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
        let mut scene = Scene::new(size.width, size.height);
        if self.attached {
            sheet_shadow(&mut scene, size, theme.variant);
        }
        sheet(&mut scene, size, theme.variant, Backdrop::Opaque);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Dialog
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_modal();
        if !self.title.is_empty() {
            node.set_label(self.title.clone());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        std::iter::once(self.content.id())
            .chain(self.buttons.as_ref().map(WidgetPod::id))
            .collect()
    }

    fn accepts_focus(&self) -> bool {
        // Focus comes by request only (on opening); Tab never lands here.
        false
    }
}
