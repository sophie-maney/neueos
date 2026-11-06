//! The pop-up button and the pull-down button (task 2.17b, ADR 0046): one widget, two
//! behaviours. A pop-up button shows its chosen item and opens its menu with that item over
//! the button; a pull-down button shows a fixed title and opens a menu of commands below
//! itself. Both open on a press (press-drag-release chooses), and from the keyboard with
//! Space, Return or Down.

use std::any::TypeId;
use std::sync::Arc;
use std::time::Instant;

use masonry_core::accesskit::{Action, HasPopup, Node, Role};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Point, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{PopUpButton as PopUpSpec, PopUpKind, popup_button, popup_cap_rect};
use neue_theme::materials::menu::{ITEM_HEIGHT, PADDING_Y};
use neue_theme::metrics::control;

use super::context::handler;
use super::menu::{self, Anchor, Chosen, TypeSelect};
use super::text::{TextLine, TextStyle};
use crate::element::{Element, Handle};
use crate::menu::{ItemHandler, ItemSpec, Menu, MenuSpec, MenuToggle, SpecKind};
use crate::theme::ThemeProp;
use crate::{CheckState, Cx};

/// The Masonry widget behind [`PopUpButton`] and [`PullDownButton`].
pub(crate) struct PopUpWidget {
    kind: PopUpKind,
    /// A pop-up button: every item's title (the widest sets the width). A pull-down
    /// button: just its title.
    lines: Vec<TextLine>,
    /// The chosen item (pop-up buttons).
    selected: usize,
    spec: MenuSpec,
    /// Its menu is up.
    open: bool,
    return_focus: Option<WidgetId>,
    type_select: TypeSelect,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for PopUpWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopUpWidget")
            .field("kind", &self.kind)
            .field("selected", &self.selected)
            .field("open", &self.open)
            .finish_non_exhaustive()
    }
}

/// The menu of a pop-up button with these `labels`: a plain list, the chosen item checked.
fn choices(labels: &[TextLine], selected: usize) -> MenuSpec {
    MenuSpec(
        labels
            .iter()
            .enumerate()
            .map(|(id, line)| ItemSpec {
                id,
                label: line.text().to_owned(),
                shortcut: None,
                signal: None,
                enabled: true,
                toggle: if id == selected {
                    MenuToggle::Check(CheckState::On)
                } else {
                    MenuToggle::None
                },
                kind: SpecKind::Action,
            })
            .collect::<Arc<[_]>>(),
    )
}

impl PopUpWidget {
    fn pop_up(labels: Vec<String>) -> Self {
        let lines: Vec<TextLine> = labels
            .into_iter()
            .map(|l| TextLine::new(l, TextStyle::Body))
            .collect();
        Self {
            kind: PopUpKind::PopUp,
            spec: choices(&lines, 0),
            lines,
            selected: 0,
            open: false,
            return_focus: None,
            type_select: TypeSelect::default(),
            replayer: ImagingReplayer::new(),
        }
    }

    fn pull_down(title: String, spec: MenuSpec) -> Self {
        Self {
            kind: PopUpKind::PullDown,
            lines: vec![TextLine::new(title, TextStyle::Body)],
            selected: 0,
            spec,
            open: false,
            return_focus: None,
            type_select: TypeSelect::default(),
            replayer: ImagingReplayer::new(),
        }
    }

    /// Makes item `index` the chosen one (the last if it is out of range).
    fn select(&mut self, index: usize) {
        if self.kind == PopUpKind::PopUp {
            self.selected = index.min(self.lines.len().saturating_sub(1));
            self.spec = choices(&self.lines, self.selected);
        }
    }

    /// The line the button shows.
    fn shown(&mut self) -> Option<&mut TextLine> {
        let i = self.selected;
        self.lines.get_mut(i)
    }

    /// Opens the menu: a pop-up button's over the button, with the chosen item level with
    /// the button's title; a pull-down button's right below it.
    fn open_menu(&mut self, ctx: &mut EventCtx<'_>, keyboard: bool, pointer: Option<Point>) {
        if ctx.is_disabled() {
            return;
        }
        let rect = ctx.border_box();
        let origin = ctx.to_window(rect.origin());
        let at = match self.kind {
            PopUpKind::PopUp => {
                let row = PADDING_Y + self.selected as f64 * ITEM_HEIGHT + ITEM_HEIGHT / 2.0;
                Point::new(origin.x, origin.y + rect.height() / 2.0 - row)
            }
            PopUpKind::PullDown => Point::new(origin.x, origin.y + rect.height()),
        };
        let anchor = Anchor {
            at,
            pointer: pointer.unwrap_or(at),
            min_width: rect.width(),
            highlight: (self.kind == PopUpKind::PopUp).then_some(self.selected),
            on_closed: Some(closed),
            refocus: None,
            on_step: None,
            return_focus: None,
        };
        menu::open(
            ctx,
            self.spec.clone(),
            anchor,
            keyboard,
            &mut self.return_focus,
        );
        self.open = true;
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }

    /// Typing a title's start while the menu is closed chooses the first item with it.
    fn type_select(&mut self, ctx: &mut EventCtx<'_>, text: &str) {
        let prefix = self.type_select.push(text, Instant::now()).to_owned();
        let found = self
            .lines
            .iter()
            .position(|l| l.text().to_lowercase().starts_with(&prefix));
        if let Some(i) = found
            && i != self.selected
        {
            self.select(i);
            ctx.submit_action::<Chosen>(Chosen(i));
            self.after_change(ctx);
        }
    }

    fn after_change(&self, ctx: &mut EventCtx<'_>) {
        ctx.request_layout();
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }
}

/// The owner's menus closed (`chosen`: the item, if one was chosen).
fn closed(mut owner: WidgetMut<'_, dyn Widget>, chosen: Option<usize>) {
    let mut this = owner.downcast::<PopUpWidget>();
    this.widget.open = false;
    if let Some(i) = chosen {
        this.widget.select(i);
        this.ctx.submit_action::<Chosen>(Chosen(i));
        this.ctx.request_layout();
    }
    this.ctx.request_paint_only();
    this.ctx.request_accessibility_update();
}

impl UsesProperty<ThemeProp> for PopUpWidget {}

impl Widget for PopUpWidget {
    type Action = Chosen;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent {
            button: None | Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            // While its menu is up the menu already took this press as an outside one and
            // is closing: pressing the button again closes it rather than reopening it.
            if ctx.get_attached_layer::<menu::MenuWidget>().is_none() {
                let at = ctx.to_window(ctx.local_position(state.position));
                self.open_menu(ctx, false, Some(at));
            }
            ctx.set_handled();
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if key.state.is_down() => {
                let typing = self.type_select.is_typing(Instant::now());
                match &key.key {
                    Key::Named(NamedKey::Enter | NamedKey::ArrowDown) => {
                        self.open_menu(ctx, true, None);
                        ctx.set_handled();
                    }
                    Key::Character(c) if c == " " && !typing => {
                        self.open_menu(ctx, true, None);
                        ctx.set_handled();
                    }
                    Key::Character(c)
                        if self.kind == PopUpKind::PopUp
                            && !key.modifiers.intersects(
                                Modifiers::CONTROL | Modifiers::META | Modifiers::ALT,
                            ) =>
                    {
                        self.type_select(ctx, c);
                        ctx.set_handled();
                    }
                    _ => {}
                }
            }
            // The focus ring shows only in the active window.
            TextEvent::WindowFocusChange(_) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            self.open_menu(ctx, true, None);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) | Update::FocusChanged(_) => ctx.request_paint_only(),
            // A compositor can take its menu away without telling us.
            Update::HoveredChanged(_) => {
                let open = ctx.get_attached_layer::<menu::MenuWidget>().is_some();
                if open != self.open {
                    self.open = open;
                    ctx.request_paint_only();
                    ctx.request_accessibility_update();
                }
            }
            Update::FontsChanged => {
                for line in &mut self.lines {
                    line.invalidate();
                }
                ctx.request_layout();
            }
            _ => {}
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
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(match axis {
            Axis::Horizontal => {
                let widest = self
                    .lines
                    .iter_mut()
                    .map(|l| l.size(ctx.text_contexts()).width)
                    .fold(0.0, f64::max);
                (control::POPUP_LABEL_INSET
                    + widest
                    + control::BUTTON_PADDING / 2.0
                    + control::POPUP_CAP_WIDTH)
                    .ceil()
                    .max(control::BUTTON_MIN_WIDTH)
            }
            Axis::Vertical => control::REGULAR_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let mut baseline = 0.0;
        if let Some(line) = self.shown() {
            let text = line.size(ctx.text_contexts());
            baseline = (size.height - text.height) / 2.0 + line.baseline();
        }
        ctx.set_baselines(baseline, baseline);
        ctx.set_paint_insets(Insets::uniform(control::FOCUS_RING + control::RIM));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let rect = Rect::from_origin_size(Point::ORIGIN, size);
        let disabled = ctx.is_disabled();
        let spec = PopUpSpec {
            kind: self.kind,
            open: self.open,
            disabled,
            focused: ctx.is_focus_target() && ctx.is_window_focused(),
        };
        let mut scene = Scene::new(size.width, size.height);
        popup_button(&mut scene, rect, spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let colors = theme.colors();
        let colour = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        let clip = Rect::new(rect.x0, rect.y0, popup_cap_rect(rect).x0, rect.y1);
        let text_contexts = ctx.text_contexts();
        if let Some(line) = self.shown() {
            let text = line.size(text_contexts);
            let origin = Vec2::new(
                control::POPUP_LABEL_INSET,
                ((size.height - text.height) / 2.0).round(),
            );
            painter.push_fill_clip(clip);
            line.paint(painter, Affine::translate(origin), colour);
            painter.pop_clip();
        }
    }

    fn accessibility_role(&self) -> Role {
        match self.kind {
            PopUpKind::PopUp => Role::ComboBox,
            PopUpKind::PullDown => Role::Button,
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let shown = self
            .lines
            .get(self.selected)
            .map(|l| l.text().to_owned())
            .unwrap_or_default();
        match self.kind {
            PopUpKind::PopUp => node.set_value(shown),
            PopUpKind::PullDown => node.set_label(shown),
        }
        node.add_action(Action::Click);
        node.set_has_popup(HasPopup::Menu);
        node.set_expanded(self.open);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        self.lines.get(self.selected).map(|l| l.text().to_owned())
    }
}

/// A pop-up button: shows the chosen one of a list of titles; pressing it opens the list
/// with the chosen item over the button. Choose by clicking, by pressing, dragging and
/// releasing, or with the keyboard: Space, Return or Down while it has focus (Tab moves
/// focus) open the list, and typing the start of a title chooses it while the list is
/// closed. Sends the index of the title the user chose.
///
/// ```no_run
/// # use neue_kit::{Handle, PopUpButton};
/// # #[derive(Clone)] enum Msg { Format(usize) }
/// let format = PopUpButton::new(["PNG", "JPEG", "TIFF"]).selected(1).on_select(Msg::Format);
/// ```
pub struct PopUpButton<M> {
    inner: NewWidget<PopUpWidget>,
    on_select: Option<Box<dyn FnMut(usize) -> M>>,
}

impl<M> std::fmt::Debug for PopUpButton<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PopUpButton")
            .field("inner", &self.inner)
            .field("on_select", &self.on_select.is_some())
            .finish()
    }
}

impl<M: 'static> PopUpButton<M> {
    /// A button for the titles `labels`, the first one chosen.
    pub fn new<S: Into<String>>(labels: impl IntoIterator<Item = S>) -> Self {
        let labels = labels.into_iter().map(Into::into).collect();
        Self {
            inner: NewWidget::new(PopUpWidget::pop_up(labels)),
            on_select: None,
        }
    }

    /// Starts with item `index` chosen (the last one if it is out of range).
    #[must_use]
    pub fn selected(mut self, index: usize) -> Self {
        self.inner.widget.select(index);
        self
    }

    /// Sends the message `make` returns, given the index the user chose. Changes through
    /// a [`Handle`] send nothing.
    #[must_use]
    pub fn on_select(mut self, make: impl FnMut(usize) -> M + 'static) -> Self {
        self.on_select = Some(Box::new(make));
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

impl<M: 'static> From<PopUpButton<M>> for Element<M> {
    fn from(button: PopUpButton<M>) -> Self {
        let id = button.inner.id();
        let mut element = Element::new(button.inner.erased());
        if let Some(mut make) = button.on_select {
            element.on_action(id, move |action| {
                let Chosen(i) = *action.downcast_ref::<Chosen>()?;
                Some(make(i))
            });
        }
        element
    }
}

impl<M> Handle<PopUpButton<M>> {
    /// Chooses item `index` (the last one if it is out of range). Sends nothing.
    pub fn set_selected<N>(&self, cx: &mut Cx<'_, N>, index: usize) {
        cx.edit::<PopUpWidget, _>(self, |mut w| {
            w.widget.select(index);
            w.ctx.request_layout();
            w.ctx.request_accessibility_update();
        });
    }

    /// Enables or disables the button.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<PopUpWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

/// A pull-down button: a fixed title that opens a menu of commands right below itself.
/// Opens like a [`PopUpButton`] (a press, or Space, Return or Down with focus); choosing an
/// item sends its message.
///
/// ```no_run
/// # use neue_kit::{Menu, MenuItem, PullDownButton};
/// # #[derive(Clone)] enum Msg { Rename, Delete }
/// let actions = PullDownButton::new(
///     "Actions",
///     Menu::new()
///         .with(MenuItem::new("Rename").on_choose(Msg::Rename))
///         .separator()
///         .with(MenuItem::new("Delete").on_choose(Msg::Delete)),
/// );
/// ```
pub struct PullDownButton<M> {
    inner: NewWidget<PopUpWidget>,
    items: Vec<Option<ItemHandler<M>>>,
}

impl<M> std::fmt::Debug for PullDownButton<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PullDownButton")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M: 'static> PullDownButton<M> {
    /// A button titled `title` that opens `menu`.
    pub fn new(title: impl Into<String>, menu: Menu<M>) -> Self {
        let (spec, items) = menu.into_parts();
        Self {
            inner: NewWidget::new(PopUpWidget::pull_down(title.into(), spec)),
            items,
        }
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

impl<M: 'static> From<PullDownButton<M>> for Element<M> {
    fn from(button: PullDownButton<M>) -> Self {
        let id = button.inner.id();
        let mut element = Element::new(button.inner.erased());
        element.on_action(id, handler(button.items));
        element
    }
}

impl<M: 'static> Handle<PullDownButton<M>> {
    /// Replaces the menu (for the next time it opens): update check marks, enable or
    /// disable items.
    pub fn set_menu(&self, cx: &mut Cx<'_, M>, menu: Menu<M>) {
        let (Some(id), Some(window)) = (self.id(), cx.window_of(self)) else {
            return;
        };
        let (spec, items) = menu.into_parts();
        cx.edit::<PopUpWidget, _>(self, |w| w.widget.spec = spec);
        let mut wiring = crate::element::Wiring::default();
        wiring.handlers.push((id, handler(items)));
        cx.register_wiring(window, wiring);
    }
}

impl<M> Handle<PullDownButton<M>> {
    /// Enables or disables the button.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<PopUpWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
