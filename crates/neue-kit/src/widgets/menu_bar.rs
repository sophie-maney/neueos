//! The shell's menu bar (task 3.2b, ADR 0068): a strip in the menu-bar material, with menu
//! titles from its left edge and menu extras from its right edge. Each title (the logo,
//! the app's name, a menu extra such as the clock) opens its menu right below itself and
//! stays highlighted while the menu is up. The active app's menus (task 3.7a, ADR 0076)
//! are a group of titles after the fixed ones that the shell replaces as apps change.

use masonry_core::accesskit::{Action, HasPopup, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetId, WidgetMut,
    WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Point, Rect, Size, Vec2};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{menu_bar, menu_highlight};
use neue_paint::{Backdrop, Scene};
use neue_theme::materials::menu as menu_tokens;
use neue_theme::materials::menu_bar::{
    EDGE_INSET, HEIGHT, LABEL_BASELINE, LABEL_EMBOSS_OFFSET, LOGO_SIZE, Palette, TITLE_PADDING,
};

use super::menu::{self, Anchor};
use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::element::{Element, Handle, Handler, Wiring};
use crate::menu::{ItemHandler, Menu, MenuSpec};
use crate::theme::ThemeProp;

/// The Masonry widget behind [`MenuBar`].
pub(crate) struct MenuBarWidget {
    leading: Vec<WidgetPod<dyn Widget>>,
    /// The active app's menus, after `leading`.
    menus: Vec<WidgetPod<dyn Widget>>,
    trailing: Vec<WidgetPod<dyn Widget>>,
    backdrop: Backdrop,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for MenuBarWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBarWidget")
            .field("leading", &self.leading.len())
            .field("menus", &self.menus.len())
            .field("trailing", &self.trailing.len())
            .field("backdrop", &self.backdrop)
            .finish_non_exhaustive()
    }
}

impl MenuBarWidget {
    fn left(&mut self) -> impl Iterator<Item = &mut WidgetPod<dyn Widget>> {
        self.leading.iter_mut().chain(&mut self.menus)
    }

    fn edit_set_menus(this: &mut WidgetMut<'_, Self>, menus: Vec<NewWidget<dyn Widget>>) {
        for old in std::mem::take(&mut this.widget.menus) {
            this.ctx.remove_child(old);
        }
        this.widget.menus = menus.into_iter().map(NewWidget::to_pod).collect();
        this.ctx.children_changed();
    }
}

impl UsesProperty<ThemeProp> for MenuBarWidget {}

impl Widget for MenuBarWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for child in self
            .leading
            .iter_mut()
            .chain(&mut self.menus)
            .chain(&mut self.trailing)
        {
            ctx.register_child(child);
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ThemeProp>() {
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
        match axis {
            Axis::Vertical => Length::px(HEIGHT),
            // Every title side by side: what the bar needs to show them all.
            Axis::Horizontal => {
                let mut width = Length::px(2.0 * EDGE_INSET);
                let all = self.leading.iter_mut().chain(&mut self.menus);
                for child in all.chain(&mut self.trailing) {
                    let w = ctx.compute_length(
                        child,
                        LenDef::MaxContent,
                        LayoutSize::maybe(Axis::Vertical, Some(Length::px(HEIGHT))),
                        Axis::Horizontal,
                        Some(Length::px(HEIGHT)),
                    );
                    width = width.saturating_add(w);
                }
                width
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let auto = SizeDef::new(LenDef::MaxContent, LenDef::Fixed(Length::px(HEIGHT)));
        let available = Size::new(size.width, HEIGHT);
        let mut x = EDGE_INSET;
        for child in self.left() {
            let child_size = ctx.compute_size(child, auto, available.into());
            ctx.run_layout(child, child_size);
            ctx.place_child(child, Point::new(x, 0.0));
            x += child_size.width;
        }
        // Extras from the right edge leftwards, the first one rightmost.
        let mut right = size.width - EDGE_INSET;
        for child in &mut self.trailing {
            let child_size = ctx.compute_size(child, auto, available.into());
            ctx.run_layout(child, child_size);
            right -= child_size.width;
            ctx.place_child(child, Point::new(right, 0.0));
        }
        ctx.clear_baselines();
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
        menu_bar::menu_bar(
            &mut scene,
            Rect::from_origin_size(Point::ORIGIN, size),
            theme.variant,
            self.backdrop,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::MenuBar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        self.leading
            .iter()
            .chain(&self.menus)
            .chain(&self.trailing)
            .map(WidgetPod::id)
            .collect()
    }
}

/// What a menu title shows.
#[derive(Debug)]
enum Content {
    Text(Box<TextLine>),
    /// The logo mark; the string is its accessible name.
    Logo(String),
}

/// What a [`MenuTitleWidget`] tells its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TitleAction {
    /// Its menu opened.
    Opened,
    /// An item was chosen (its id in the menu tree).
    Chosen(usize),
    /// Its menu closed (after [`TitleAction::Chosen`], if an item was chosen).
    Closed,
}

/// The Masonry widget behind [`MenuTitle`].
pub(crate) struct MenuTitleWidget {
    content: Content,
    spec: MenuSpec,
    /// Its menu is up.
    open: bool,
    return_focus: Option<WidgetId>,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for MenuTitleWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuTitleWidget")
            .field("content", &self.content)
            .field("open", &self.open)
            .finish_non_exhaustive()
    }
}

impl MenuTitleWidget {
    fn new(content: Content, spec: MenuSpec) -> Self {
        Self {
            content,
            spec,
            open: false,
            return_focus: None,
            replayer: ImagingReplayer::new(),
        }
    }

    fn name(&self) -> &str {
        match &self.content {
            Content::Text(line) => line.text(),
            Content::Logo(name) => name,
        }
    }

    /// Opens the menu right below the title, its left edge at the title's.
    fn open_menu(&mut self, ctx: &mut EventCtx<'_>, keyboard: bool, pointer: Option<Point>) {
        if ctx.is_disabled() || self.spec.0.is_empty() {
            return;
        }
        let rect = ctx.border_box();
        let origin = ctx.to_window(rect.origin());
        let at = Point::new(origin.x, origin.y + rect.height());
        let anchor = Anchor {
            at,
            pointer: pointer.unwrap_or(at),
            min_width: 0.0,
            highlight: None,
            on_closed: Some(closed),
            refocus: None,
        };
        menu::open(
            ctx,
            self.spec.clone(),
            anchor,
            keyboard,
            &mut self.return_focus,
        );
        self.open = true;
        ctx.submit_action::<TitleAction>(TitleAction::Opened);
        ctx.request_paint_only();
        ctx.request_accessibility_update();
    }
}

/// The title's menu closed (`chosen`: the item, if one was chosen).
fn closed(mut owner: WidgetMut<'_, dyn Widget>, chosen: Option<usize>) {
    let mut this = owner.downcast::<MenuTitleWidget>();
    this.widget.open = false;
    if let Some(i) = chosen {
        this.ctx
            .submit_action::<TitleAction>(TitleAction::Chosen(i));
    }
    this.ctx.submit_action::<TitleAction>(TitleAction::Closed);
    this.ctx.request_paint_only();
    this.ctx.request_accessibility_update();
}

impl UsesProperty<ThemeProp> for MenuTitleWidget {}

impl Widget for MenuTitleWidget {
    type Action = TitleAction;

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
            // While its menu is up the menu took this press as an outside one and is
            // closing: pressing the title again closes it rather than reopening it.
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
                let opens = match &key.key {
                    Key::Named(NamedKey::Enter | NamedKey::ArrowDown) => true,
                    Key::Character(c) => c == " ",
                    _ => false,
                };
                if opens {
                    self.open_menu(ctx, true, None);
                    ctx.set_handled();
                }
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
                if let Content::Text(line) = &mut self.content {
                    line.invalidate();
                }
                ctx.request_layout();
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ThemeProp>() {
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
            Axis::Vertical => HEIGHT,
            Axis::Horizontal => {
                let content = match &mut self.content {
                    Content::Text(line) => line.size(ctx.text_contexts()).width,
                    Content::Logo(_) => LOGO_SIZE,
                };
                (content + 2.0 * TITLE_PADDING).ceil()
            }
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
        ctx.set_baselines(LABEL_BASELINE, LABEL_BASELINE);
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
        // Highlighted while its menu is up, and while it has keyboard focus (the menu bar
        // reached from the keyboard).
        let highlighted = self.open || (ctx.is_focus_target() && ctx.is_window_focused());
        // The labels' colours don't depend on the backdrop.
        let bar = Palette::new(theme.variant, Backdrop::Opaque);
        let mut scene = Scene::new(size.width, size.height);
        if highlighted {
            menu_highlight(&mut scene, rect, theme.variant);
        }
        if let Content::Logo(_) = self.content {
            let logo = Rect::from_origin_size(
                Point::new(
                    TITLE_PADDING,
                    ((HEIGHT - LOGO_SIZE) / 2.0 - LABEL_EMBOSS_OFFSET / 2.0).round(),
                ),
                Size::new(LOGO_SIZE, LOGO_SIZE),
            );
            if highlighted {
                let colour =
                    menu_tokens::Palette::new(theme.variant, Backdrop::Opaque).highlighted_label;
                menu_bar::logo(&mut scene, logo, colour);
            } else {
                menu_bar::logo_label(&mut scene, logo, theme.variant);
            }
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        if let Content::Text(line) = &mut self.content {
            line.size(ctx.text_contexts());
            let origin = Vec2::new(TITLE_PADDING, (LABEL_BASELINE - line.baseline()).round());
            if highlighted {
                let colour =
                    menu_tokens::Palette::new(theme.variant, Backdrop::Opaque).highlighted_label;
                line.paint(painter, Affine::translate(origin), colour);
            } else {
                let shadow = origin + Vec2::new(0.0, LABEL_EMBOSS_OFFSET);
                line.paint(painter, Affine::translate(shadow), bar.label_shadow);
                line.paint(painter, Affine::translate(origin), bar.label);
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::MenuItem
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.name().to_owned());
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
        Some(self.name().to_owned())
    }
}

/// The desktop's menu bar: menu titles from the left edge, menu extras from the right,
/// over the menu-bar material. Its height is the theme's menu-bar height; the shell puts
/// it on a layer surface along the top of each output.
///
/// ```no_run
/// # use neue_kit::{Menu, MenuItem};
/// # use neue_kit::desktop::{MenuBar, MenuTitle};
/// # #[derive(Clone)] enum Msg { About }
/// let bar = MenuBar::new()
///     .with(MenuTitle::logo("neueOS", Menu::new().with(MenuItem::new("About").on_choose(Msg::About))))
///     .with(MenuTitle::new("Files", Menu::new()).bold(true))
///     .with_extra(MenuTitle::new("10:09", Menu::new()));
/// ```
pub struct MenuBar<M> {
    inner: NewWidget<MenuBarWidget>,
    leading: Vec<Element<M>>,
    menus: Vec<Element<M>>,
    trailing: Vec<Element<M>>,
}

impl<M> std::fmt::Debug for MenuBar<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBar")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M> Default for MenuBar<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> MenuBar<M> {
    /// An empty bar, drawn opaque (no blur behind it).
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(MenuBarWidget {
                leading: Vec::new(),
                menus: Vec::new(),
                trailing: Vec::new(),
                backdrop: Backdrop::Opaque,
                replayer: ImagingReplayer::new(),
            }),
            leading: Vec::new(),
            menus: Vec::new(),
            trailing: Vec::new(),
        }
    }

    /// Adds a title after the ones on the left.
    #[must_use]
    pub fn with(mut self, title: impl Into<Element<M>>) -> Self {
        self.leading.push(title.into());
        self
    }

    /// Adds a title to the active app's menus, after the fixed titles ([`MenuBar::with`]);
    /// [`Handle::set_menus`] replaces them all.
    #[must_use]
    pub fn with_menu(mut self, title: MenuTitle<M>) -> Self
    where
        M: 'static,
    {
        self.menus.push(title.into());
        self
    }

    /// Adds a menu extra to the left of the ones on the right (the first is rightmost).
    #[must_use]
    pub fn with_extra(mut self, extra: impl Into<Element<M>>) -> Self {
        self.trailing.push(extra.into());
        self
    }

    /// [`Backdrop::Translucent`] once the compositor blurs what is behind the bar.
    #[must_use]
    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.inner.widget.backdrop = backdrop;
        self
    }

    /// Points `handle` at this bar.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<MenuBar<M>> for Element<M> {
    fn from(bar: MenuBar<M>) -> Self {
        let MenuBar {
            mut inner,
            leading,
            menus,
            trailing,
        } = bar;
        let mut wiring = Wiring::default();
        for child in leading {
            inner.widget.leading.push(wiring.adopt(child).to_pod());
        }
        for child in menus {
            inner.widget.menus.push(wiring.adopt(child).to_pod());
        }
        for child in trailing {
            inner.widget.trailing.push(wiring.adopt(child).to_pod());
        }
        Element::with_wiring(inner.erased(), wiring)
    }
}

impl<M: 'static> Handle<MenuBar<M>> {
    /// Replaces the active app's menus (the titles added with [`MenuBar::with_menu`]).
    /// Don't replace a title while its menu is up: wait for its close message
    /// ([`MenuTitle::on_close`]).
    pub fn set_menus(&self, cx: &mut Cx<'_, M>, titles: Vec<MenuTitle<M>>) {
        let Some(window) = cx.window_of(self) else {
            tracing::debug!("setting the menus of a bar that isn't in an open window");
            return;
        };
        let mut wiring = Wiring::default();
        let widgets = titles
            .into_iter()
            .map(|title| wiring.adopt(title.into()))
            .collect();
        cx.edit::<MenuBarWidget, _>(self, |mut bar| {
            MenuBarWidget::edit_set_menus(&mut bar, widgets);
        });
        cx.prune_handlers(window);
        cx.register_wiring(window, wiring);
    }
}

/// A title in the [`MenuBar`]: text, bold text (the active app's name) or the logo mark,
/// that opens its menu below itself. Press it, or press, drag into the menu and release;
/// from the keyboard, Space, Return or Down open it while it has focus. Choosing an item
/// sends its message. A title with an empty menu shows its text and opens nothing.
pub struct MenuTitle<M> {
    inner: NewWidget<MenuTitleWidget>,
    items: Vec<Option<ItemHandler<M>>>,
    on_open: Option<ItemHandler<M>>,
    on_close: Option<ItemHandler<M>>,
}

impl<M> std::fmt::Debug for MenuTitle<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuTitle")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl<M: 'static> MenuTitle<M> {
    /// A title reading `title` that opens `menu`.
    pub fn new(title: impl Into<String>, menu: Menu<M>) -> Self {
        let line = TextLine::new(title.into(), TextStyle::Body);
        Self::with_content(Content::Text(Box::new(line)), menu)
    }

    /// The logo mark, opening `menu`; `name` is what assistive technologies read.
    pub fn logo(name: impl Into<String>, menu: Menu<M>) -> Self {
        Self::with_content(Content::Logo(name.into()), menu)
    }

    fn with_content(content: Content, menu: Menu<M>) -> Self {
        let (spec, items) = menu.into_parts();
        Self {
            inner: NewWidget::new(MenuTitleWidget::new(content, spec)),
            items,
            on_open: None,
            on_close: None,
        }
    }

    /// Sends `message` whenever its menu opens. [`Handle::set_menu`] keeps only the items:
    /// to change this, build a new title.
    #[must_use]
    pub fn on_open(mut self, message: M) -> Self
    where
        M: Clone,
    {
        self.on_open = Some(Box::new(move || message.clone()));
        self
    }

    /// Sends `message` whenever its menu closes, after the chosen item's message if one
    /// was chosen. Like [`MenuTitle::on_open`], not kept by [`Handle::set_menu`].
    #[must_use]
    pub fn on_close(mut self, message: M) -> Self
    where
        M: Clone,
    {
        self.on_close = Some(Box::new(move || message.clone()));
        self
    }

    /// Bold text (the active app's name).
    #[must_use]
    pub fn bold(mut self, bold: bool) -> Self {
        if let Content::Text(line) = &mut self.inner.widget.content {
            line.set_bold(bold);
        }
        self
    }

    /// Points `handle` at this title.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<MenuTitle<M>> for Element<M> {
    fn from(title: MenuTitle<M>) -> Self {
        let id = title.inner.id();
        let mut element = Element::new(title.inner.erased());
        element.on_action(
            id,
            title_handler(title.items, title.on_open, title.on_close),
        );
        element
    }
}

/// Turns a title's actions into messages: the chosen item's, and the open and close ones
/// if it has them.
fn title_handler<M: 'static>(
    mut items: Vec<Option<ItemHandler<M>>>,
    mut on_open: Option<ItemHandler<M>>,
    mut on_close: Option<ItemHandler<M>>,
) -> Handler<M> {
    Box::new(move |action| match *action.downcast_ref::<TitleAction>()? {
        TitleAction::Opened => on_open.as_mut().map(|make| make()),
        TitleAction::Closed => on_close.as_mut().map(|make| make()),
        TitleAction::Chosen(i) => items.get_mut(i)?.as_mut().map(|make| make()),
    })
}

impl<M: 'static> Handle<MenuTitle<M>> {
    /// Replaces the menu (for the next time it opens).
    pub fn set_menu(&self, cx: &mut Cx<'_, M>, menu: Menu<M>) {
        let (Some(id), Some(window)) = (self.id(), cx.window_of(self)) else {
            return;
        };
        let (spec, items) = menu.into_parts();
        cx.edit::<MenuTitleWidget, _>(self, |w| w.widget.spec = spec);
        let mut wiring = Wiring::default();
        wiring.handlers.push((id, title_handler(items, None, None)));
        cx.register_wiring(window, wiring);
    }
}

impl<M> Handle<MenuTitle<M>> {
    /// Changes a text title's text (the logo has none).
    pub fn set_text<N>(&self, cx: &mut Cx<'_, N>, text: impl Into<String>) {
        let text = text.into();
        cx.edit::<MenuTitleWidget, _>(self, |mut w| {
            if let Content::Text(line) = &mut w.widget.content
                && line.set_text(text)
            {
                w.ctx.request_layout();
                w.ctx.request_paint_only();
                w.ctx.request_accessibility_update();
            }
        });
    }
}
