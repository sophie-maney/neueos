//! Toolbar items (task 2.22c): [`Toolbar`] lays out a row of items in a window's unified
//! title bar and toolbar ([`crate::Window::toolbar`]), and [`ToolbarButton`] is its
//! icon-and-label button. A toolbar takes any widget as an item (a segmented control, a
//! search field), plus fixed and flexible spaces and separators.
//!
//! The buttons have no chrome at rest and show a dark pill while held down. The display mode
//! chooses between icon and label, icon only and label only; assistive technology always
//! gets the label.

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx, NewWidget,
    NoAction, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, UsesProperty, Widget, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Point, Rect, Size, Vec2};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use neue_paint::Scene;
use neue_paint::icon::Icon;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{ToolbarItem, toolbar_item, toolbar_separator};
use neue_theme::materials::toolbar::{
    ICON_SIZE, ITEM_SPACING, LABEL_GAP, PADDING_X, PADDING_Y, SEPARATOR_HEIGHT, SEPARATOR_WIDTH,
    SPACE_WIDTH,
};
use neue_theme::metrics::control;

use super::button::Pressed;
use super::icon::draw as draw_icon;
use super::text::{TextLine, TextStyle};
use super::toggle::is_toggle_key;
use crate::Cx;
use crate::element::{Element, Handle, Wiring};
use crate::theme::ThemeProp;

/// What a toolbar's buttons show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayMode {
    /// The icon with the label under it.
    #[default]
    IconAndText,
    IconOnly,
    TextOnly,
}

/// The Masonry widget behind [`ToolbarButton`].
#[derive(Debug)]
pub(crate) struct ToolbarButtonWidget {
    icon: Icon,
    line: TextLine,
    mode: DisplayMode,
    replayer: ImagingReplayer,
    /// Space went down while focused: drawn pressed until it comes up.
    key_down: bool,
}

impl ToolbarButtonWidget {
    fn new(icon: Icon, label: String) -> Self {
        Self {
            icon,
            line: TextLine::new(label, TextStyle::Small),
            mode: DisplayMode::default(),
            replayer: ImagingReplayer::new(),
            key_down: false,
        }
    }

    pub(crate) fn edit_label(this: &mut WidgetMut<'_, Self>, label: String) {
        if this.widget.line.set_text(label) {
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }

    pub(crate) fn edit_icon(this: &mut WidgetMut<'_, Self>, icon: Icon) {
        if this.widget.icon != icon {
            this.widget.icon = icon;
            this.ctx.request_render();
        }
    }

    fn set_mode(this: &mut WidgetMut<'_, Self>, mode: DisplayMode) {
        if this.widget.mode != mode {
            this.widget.mode = mode;
            this.ctx.request_layout();
        }
    }

    fn shows_icon(&self) -> bool {
        self.mode != DisplayMode::TextOnly
    }

    fn shows_label(&self) -> bool {
        self.mode != DisplayMode::IconOnly
    }
}

impl UsesProperty<ThemeProp> for ToolbarButtonWidget {}

impl Widget for ToolbarButtonWidget {
    type Action = Pressed;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(..) => {
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(..) => {
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<Pressed>(Pressed);
                }
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if is_toggle_key(&key.key) => {
                if key.state.is_down() {
                    self.key_down = true;
                } else if std::mem::take(&mut self.key_down) {
                    ctx.submit_action::<Pressed>(Pressed);
                }
                ctx.request_paint_only();
                ctx.set_handled();
            }
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
            ctx.submit_action::<Pressed>(Pressed);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) | Update::HoveredChanged(_) | Update::ActiveChanged(_) => {
                ctx.request_paint_only();
            }
            Update::FocusChanged(focused) => {
                if !focused {
                    self.key_down = false;
                }
                ctx.request_paint_only();
            }
            Update::FontsChanged => {
                self.line.invalidate();
                ctx.request_layout();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ThemeProp>() {
            ctx.request_render();
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
        let text = self.line.size(ctx.text_contexts());
        Length::px(match axis {
            Axis::Horizontal => {
                let content = match self.mode {
                    DisplayMode::IconAndText => ICON_SIZE.max(text.width),
                    DisplayMode::IconOnly => ICON_SIZE,
                    DisplayMode::TextOnly => text.width,
                };
                (content + 2.0 * PADDING_X).ceil()
            }
            Axis::Vertical => {
                let content = match self.mode {
                    DisplayMode::IconAndText => ICON_SIZE + LABEL_GAP + text.height,
                    DisplayMode::IconOnly => ICON_SIZE,
                    DisplayMode::TextOnly => text.height,
                };
                (content + 2.0 * PADDING_Y).ceil()
            }
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
        ctx.set_paint_insets(Insets::uniform(control::FOCUS_RING));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let rect = Rect::from_origin_size((0.0, 0.0), size);
        let disabled = ctx.is_disabled();
        let pressed = (ctx.is_active() && ctx.is_hovered()) || self.key_down;
        let colors = theme.colors();
        let color = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };

        // The label's place: under the icon, or centred when it is alone.
        let text = self.line.size(ctx.text_contexts());
        let content_height = match self.mode {
            DisplayMode::IconAndText => ICON_SIZE + LABEL_GAP + text.height,
            DisplayMode::IconOnly => ICON_SIZE,
            DisplayMode::TextOnly => text.height,
        };
        let top = ((size.height - content_height) / 2.0).round();

        let mut scene = Scene::new(size.width, size.height);
        toolbar_item(
            &mut scene,
            rect,
            ToolbarItem {
                pressed,
                disabled,
                focused: ctx.is_focus_target() && ctx.is_window_focused(),
            },
            theme.variant,
        );
        if self.shows_icon() {
            let x = ((size.width - ICON_SIZE) / 2.0).round();
            let icon_rect = Rect::new(x, top, x + ICON_SIZE, top + ICON_SIZE);
            draw_icon(
                &mut scene,
                icon_rect,
                &self.icon,
                color,
                theme.variant,
                disabled,
            );
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        if self.shows_label() {
            let y = if self.shows_icon() {
                top + ICON_SIZE + LABEL_GAP
            } else {
                top
            };
            let origin = Vec2::new(((size.width - text.width) / 2.0).round(), y.round());
            self.line.paint(painter, Affine::translate(origin), color);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        // In icon-only mode the label is the only name the button has.
        node.set_label(self.line.text());
        node.add_action(Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.line.text().to_owned())
    }
}

/// The line between two groups of items.
#[derive(Debug)]
pub(crate) struct SeparatorWidget {
    replayer: ImagingReplayer,
}

impl SeparatorWidget {
    fn new() -> Self {
        Self {
            replayer: ImagingReplayer::new(),
        }
    }
}

impl UsesProperty<ThemeProp> for SeparatorWidget {}

impl Widget for SeparatorWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ThemeProp>() {
            ctx.request_render();
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
            Axis::Horizontal => SEPARATOR_WIDTH,
            Axis::Vertical => SEPARATOR_HEIGHT,
        })
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        toolbar_separator(&mut scene, size.to_rect(), theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn accessibility_role(&self) -> Role {
        Role::Splitter
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

/// One entry of a toolbar's row.
#[derive(Debug)]
enum Entry {
    /// A widget: a button, a separator, or any control.
    Widget(WidgetPod<dyn Widget>),
    /// Takes an equal share of whatever room the other entries leave.
    Flexible,
    /// A gap of this width.
    Space(f64),
}

/// The Masonry widget behind [`Toolbar`]: a row, centred vertically, whose flexible spaces
/// share the room left over.
#[derive(Debug)]
pub(crate) struct ToolbarWidget {
    entries: Vec<Entry>,
}

impl ToolbarWidget {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Sets the display mode of every button.
    pub(crate) fn edit_mode(this: &mut WidgetMut<'_, Self>, mode: DisplayMode) {
        for entry in &mut this.widget.entries {
            if let Entry::Widget(pod) = entry {
                let mut child = this.ctx.get_mut(pod);
                if let Some(mut button) = child.try_downcast::<ToolbarButtonWidget>() {
                    ToolbarButtonWidget::set_mode(&mut button, mode);
                }
            }
        }
        this.ctx.request_layout();
    }

    fn gaps(&self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "item counts are tiny")]
        let gaps = self.entries.len().saturating_sub(1) as f64;
        ITEM_SPACING * gaps
    }
}

impl Widget for ToolbarWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for entry in &mut self.entries {
            if let Entry::Widget(pod) = entry {
                ctx.register_child(pod);
            }
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
        let along = axis == Axis::Horizontal;
        let used = if along {
            Length::px(self.gaps())
        } else {
            Length::ZERO
        };
        let auto: LenDef = len_req.reduce(used).into();
        let context = LayoutSize::maybe(axis.cross(), cross_length);
        // Along the row, items share the length; across it, each gets the row's.
        let child_cross = if along { None } else { cross_length };
        let mut length = Length::ZERO;
        for entry in &mut self.entries {
            let item = match entry {
                Entry::Widget(pod) => ctx.compute_length(pod, auto, context, axis, child_cross),
                Entry::Space(width) if along => Length::px(*width),
                Entry::Space(_) | Entry::Flexible => Length::ZERO,
            };
            length = if along {
                length.saturating_add(item)
            } else {
                length.max(item)
            };
        }
        length.saturating_add(used)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let auto = SizeDef::new(
            LenDef::MaxContent,
            LenDef::FitContent(Length::px(size.height)),
        );
        // Natural widths first, to know how much room the flexible spaces share.
        let mut sizes = Vec::with_capacity(self.entries.len());
        let mut natural = self.gaps();
        let mut flexible = 0_u32;
        for entry in &mut self.entries {
            let item = match entry {
                Entry::Widget(pod) => ctx.compute_size(pod, auto, size.into()),
                Entry::Space(width) => Size::new(*width, 0.0),
                Entry::Flexible => {
                    flexible += 1;
                    Size::ZERO
                }
            };
            natural += item.width;
            sizes.push(item);
        }
        let share = if flexible > 0 {
            (size.width - natural).max(0.0) / f64::from(flexible)
        } else {
            0.0
        };

        let mut x = 0.0_f64;
        for (entry, item) in self.entries.iter_mut().zip(sizes) {
            match entry {
                Entry::Widget(pod) => {
                    ctx.run_layout(pod, item);
                    let y = ((size.height - item.height) / 2.0).round();
                    ctx.place_child(pod, Point::new(x.round(), y));
                    x += item.width;
                }
                Entry::Space(width) => x += *width,
                Entry::Flexible => x += share,
            }
            x += ITEM_SPACING;
        }
        ctx.clear_baselines();
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::Toolbar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Widget(pod) => Some(pod.id()),
                _ => None,
            })
            .collect()
    }
}

/// A toolbar button: an icon with a label under it, no chrome until pressed. Sends a message
/// when pressed by pointer, by Space while it has keyboard focus (Tab moves focus), or by an
/// assistive technology, which always announces the label, whatever the toolbar's
/// [`DisplayMode`].
pub struct ToolbarButton<M> {
    inner: NewWidget<ToolbarButtonWidget>,
    on_press: Option<Box<dyn FnMut() -> M>>,
}

impl<M> std::fmt::Debug for ToolbarButton<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolbarButton")
            .field("inner", &self.inner)
            .field("on_press", &self.on_press.is_some())
            .finish()
    }
}

impl<M: 'static> ToolbarButton<M> {
    /// A button titled `label`, drawn with `icon` (a [`crate::Glyph`], a
    /// [`crate::VectorIcon`] or an [`Icon`]).
    pub fn new(label: impl Into<String>, icon: impl Into<Icon>) -> Self {
        Self {
            inner: NewWidget::new(ToolbarButtonWidget::new(icon.into(), label.into())),
            on_press: None,
        }
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

impl<M: 'static> From<ToolbarButton<M>> for Element<M> {
    fn from(button: ToolbarButton<M>) -> Self {
        let id = button.inner.id();
        let mut element = Element::new(button.inner.erased());
        if let Some(mut make) = button.on_press {
            element.on_action(id, move |action| action.is::<Pressed>().then(&mut make));
        }
        element
    }
}

impl<M> Handle<ToolbarButton<M>> {
    /// Replaces the button's title.
    pub fn set_label<N>(&self, cx: &mut Cx<'_, N>, label: impl Into<String>) {
        let label = label.into();
        cx.edit::<ToolbarButtonWidget, _>(self, |mut w| {
            ToolbarButtonWidget::edit_label(&mut w, label);
        });
    }

    /// Replaces the button's icon.
    pub fn set_icon<N>(&self, cx: &mut Cx<'_, N>, icon: impl Into<Icon>) {
        let icon = icon.into();
        cx.edit::<ToolbarButtonWidget, _>(self, |mut w| {
            ToolbarButtonWidget::edit_icon(&mut w, icon);
        });
    }

    /// Enables or disables the button.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ToolbarButtonWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

/// What a [`Toolbar`] was given, in order.
enum Part<M> {
    Item(Box<Element<M>>),
    Flexible,
    Space,
    Separator,
}

/// The items of a window's toolbar ([`crate::Window::toolbar`]), left to right and centred
/// vertically: buttons ([`ToolbarButton`]), any other control (a [`crate::Segmented`], a
/// [`crate::TextField::search`] field), fixed and flexible spaces, and separators. Flexible
/// spaces share the room the items leave; they push items to the right edge or centre a
/// group. A toolbar does not scroll or collapse: items that don't fit are clipped.
///
/// Tab moves through the items, and assistive technology sees a toolbar.
///
/// ```no_run
/// # use neue_kit::{DisplayMode, Glyph, Toolbar, ToolbarButton, TextField};
/// # #[derive(Clone)] enum Msg { Back, Forward, Query(String) }
/// let toolbar = Toolbar::new()
///     .with(ToolbarButton::new("Back", Glyph::Folder).on_press(Msg::Back))
///     .with(ToolbarButton::new("Forward", Glyph::Document).on_press(Msg::Forward))
///     .flexible_space()
///     .with(TextField::search().on_submit(Msg::Query))
///     .display_mode(DisplayMode::IconAndText);
/// ```
pub struct Toolbar<M> {
    inner: NewWidget<ToolbarWidget>,
    parts: Vec<Part<M>>,
    mode: DisplayMode,
}

impl<M> std::fmt::Debug for Toolbar<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toolbar")
            .field("inner", &self.inner)
            .field("parts", &self.parts.len())
            .field("mode", &self.mode)
            .finish()
    }
}

impl<M> Default for Toolbar<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> Toolbar<M> {
    /// An empty toolbar.
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(ToolbarWidget::new()),
            parts: Vec::new(),
            mode: DisplayMode::default(),
        }
    }

    /// Adds an item at the end.
    #[must_use]
    pub fn with(mut self, item: impl Into<Element<M>>) -> Self {
        self.parts.push(Part::Item(Box::new(item.into())));
        self
    }

    /// Adds a space that takes an equal share of the room the other items leave.
    #[must_use]
    pub fn flexible_space(mut self) -> Self {
        self.parts.push(Part::Flexible);
        self
    }

    /// Adds a gap of a fixed width.
    #[must_use]
    pub fn space(mut self) -> Self {
        self.parts.push(Part::Space);
        self
    }

    /// Adds a line between two groups of items.
    #[must_use]
    pub fn separator(mut self) -> Self {
        self.parts.push(Part::Separator);
        self
    }

    /// What the toolbar's buttons show. Icon and label by default.
    #[must_use]
    pub fn display_mode(mut self, mode: DisplayMode) -> Self {
        self.mode = mode;
        self
    }

    /// Points `handle` at this toolbar.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<Toolbar<M>> for Element<M> {
    fn from(toolbar: Toolbar<M>) -> Self {
        let Toolbar {
            mut inner,
            parts,
            mode,
        } = toolbar;
        let mut wiring = Wiring::default();
        for part in parts {
            let entry = match part {
                Part::Item(element) => {
                    let mut widget = wiring.adopt(*element);
                    if let Some(button) = ToolbarButtonWidget::from_dyn_mut(&mut *widget.widget) {
                        button.mode = mode;
                    }
                    Entry::Widget(widget.to_pod())
                }
                Part::Flexible => Entry::Flexible,
                Part::Space => Entry::Space(SPACE_WIDTH),
                Part::Separator => {
                    Entry::Widget(NewWidget::new(SeparatorWidget::new()).erased().to_pod())
                }
            };
            inner.widget.entries.push(entry);
        }
        Element::with_wiring(inner.erased(), wiring)
    }
}

impl<M> Handle<Toolbar<M>> {
    /// Changes what the toolbar's buttons show.
    pub fn set_display_mode<N>(&self, cx: &mut Cx<'_, N>, mode: DisplayMode) {
        cx.edit::<ToolbarWidget, _>(self, |mut w| ToolbarWidget::edit_mode(&mut w, mode));
    }
}
