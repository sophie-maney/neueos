//! A menu on screen (ADR 0045): a Masonry layer (an xdg-popup on Wayland, an in-window
//! overlay on macOS) holding one row widget per item. Submenus are layers attached to the
//! menu they open from, so closing a menu closes its submenus.
//!
//! - **Opening.** An owner widget (a context menu area) calls [`open`]. The menu takes
//!   keyboard focus once it is in the tree, and gives it back to whatever had it when it
//!   closes.
//! - **Pointer.** Every open menu of one session sees every pointer event (Masonry's
//!   [`Layer::capture_pointer_event`]). Moving over an item highlights it and opens its
//!   submenu. Releasing the button over an item chooses it once the session is armed: by
//!   a press inside a menu, or by dragging past [`DRAG_THRESHOLD`] from where the press
//!   opened the menu (press, drag, release). A press outside every menu closes them all.
//! - **Keyboard.** Up and Down (Home, End) move the highlight over enabled items, Right
//!   opens a submenu, Left closes one, Return or Space chooses, Escape closes, and typing
//!   jumps to the first item starting with what was typed ([`TYPE_SELECT_RESET`]). In a
//!   menu bar's menu ([`Anchor::on_step`]), Left in the top menu and Right on an item with
//!   no submenu go to the neighbouring menu of the bar.
//! - **Choosing.** The owner receives [`Chosen`] with the item's id and closes the menus.

use std::any::TypeId;
use std::cell::Cell;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use masonry_core::accesskit::{Action, HasPopup, Node, Role, Toggled};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ActionCtx, ChildrenIds, ErasedAction, EventCtx, Layer, LayerType,
    LayoutCtx, MeasureCtx, MutateCtx, NewWidget, NoAction, PaintCtx, PointerButtonEvent,
    PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, UsesProperty, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry_core::dpi::PhysicalPosition;
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Point, Rect, Size, Vec2};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{
    MenuMark, menu_highlight, menu_lock, menu_mark, menu_panel, menu_separator, menu_signal,
    submenu_arrow,
};
use neue_paint::{Backdrop, Scene};
use neue_platform::{PopupBlur, PopupBlurRequest};
use neue_theme::materials::menu::{
    ACCESSORY_GAP, ARROW_WIDTH, DRAG_THRESHOLD, ITEM_HEIGHT, LABEL_INSET, LOCK_HEIGHT, LOCK_WIDTH,
    MARK_SIZE, MIN_WIDTH, PADDING_Y, Palette, RADIUS, RIM, SEPARATOR_HEIGHT, SHADOW_OFFSET,
    SHADOW_REACH, SHORTCUT_GAP, SIGNAL_HEIGHT, SIGNAL_WIDTH, SUBMENU_OVERLAP, TRAILING_INSET,
};
use neue_theme::motion::TYPE_SELECT_RESET;

use super::text::{TextLine, TextStyle};
use crate::CheckState;
use crate::menu::{ItemSpec, MenuSpec, MenuToggle, Signal, SpecKind};
use crate::theme::{BackdropProp, ThemeProp};

/// What a menu's owner receives when an item is chosen: the item's id in the menu tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Chosen(pub(crate) usize);

/// Called on a menu's owner, once its menus are gone, with the id of the item chosen (if
/// any), for owners that keep state of their own (an open flag, a selection) or whose
/// action type isn't [`Chosen`]. It then reports the choice itself; without it the owner
/// receives [`Chosen`].
pub(crate) type OnClosed = fn(WidgetMut<'_, dyn Widget>, Option<usize>);

/// Called on a menu-bar title, the owner, when the keyboard leaves its menus for the
/// bar's next menu (`true`, Right) or previous one (`false`, Left). The menus stay up
/// until the bar replaces them.
pub(crate) type OnStep = fn(WidgetMut<'_, dyn Widget>, bool);

/// Where and how a menu opens.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Anchor {
    /// The menu's top-left corner, in window coordinates.
    pub(crate) at: Point,
    /// Where the pointer went down, for the drag that chooses on release (the menu's
    /// corner for a context menu).
    pub(crate) pointer: Point,
    /// The menu is at least this wide (a pop-up button's width).
    pub(crate) min_width: f64,
    /// The item highlighted on opening, if any (a pop-up button's chosen one).
    pub(crate) highlight: Option<usize>,
    /// Told when the menus close.
    pub(crate) on_closed: Option<OnClosed>,
    /// Where focus goes back to if nothing had it when the menu opened.
    pub(crate) refocus: Option<WidgetId>,
    /// The owner is a menu-bar title: Left and Right may leave for its neighbours.
    pub(crate) on_step: Option<OnStep>,
    /// The menu replaces another menu of the same bar (switching menus): what had focus
    /// before that one opened, which focus goes back to. `None`: what has focus now.
    pub(crate) return_focus: Option<Option<WidgetId>>,
}

impl Anchor {
    /// A menu with its corner at the pointer, `at`.
    pub(crate) fn at(at: Point) -> Self {
        Self {
            at,
            pointer: at,
            min_width: 0.0,
            highlight: None,
            on_closed: None,
            refocus: None,
            on_step: None,
            return_focus: None,
        }
    }
}

thread_local! {
    /// How many menus this thread has opened. A widget that opens a default menu after
    /// an event (so that a context menu around it can take the event first) compares this
    /// with its value at the event: a change means somebody else opened one. Menus open on
    /// the UI thread only.
    static OPENED: Cell<usize> = const { Cell::new(0) };
}

/// See [`OPENED`].
pub(crate) fn opened() -> usize {
    OPENED.with(Cell::get)
}

/// What opening a menu needs from the owner's context: an event, or a deferred mutation.
pub(crate) trait Opener {
    fn owner(&self) -> WidgetId;
    /// The owner's open menu (root layer), if any.
    fn open_menu(&self) -> Option<WidgetId>;
    fn focus_target(&self) -> Option<WidgetId>;
    fn create_menu(&mut self, menu: NewWidget<MenuWidget>, at: Point);
}

macro_rules! opener {
    ($($ctx:ty),+) => {$(
        impl Opener for $ctx {
            fn owner(&self) -> WidgetId {
                self.widget_id()
            }
            fn open_menu(&self) -> Option<WidgetId> {
                self.get_attached_layer::<MenuWidget>()
            }
            fn focus_target(&self) -> Option<WidgetId> {
                self.focus_target_id()
            }
            fn create_menu(&mut self, menu: NewWidget<MenuWidget>, at: Point) {
                self.create_attached_layer(LayerType::Other, menu, at);
            }
        }
    )+};
}

opener!(EventCtx<'_>, MutateCtx<'_>);

/// Opens `spec` as a menu owned by the widget of `ctx`, placed by `anchor`. The owner's
/// action type must be [`Chosen`] unless `anchor` has an `on_closed` that reports the
/// choice. `keyboard`: opened from the keyboard (or an assistive technology), so the first
/// item is highlighted and releasing a button chooses straight away. An open menu of the
/// same owner is replaced. `return_focus` is the owner's record of what had focus before
/// its menu opened, kept across a reopening while the menu is up (focus is in that menu
/// then).
pub(crate) fn open(
    ctx: &mut impl Opener,
    spec: MenuSpec,
    anchor: Anchor,
    keyboard: bool,
    return_focus: &mut Option<WidgetId>,
) {
    if spec.0.is_empty() {
        return;
    }
    let owner = ctx.owner();
    if let Some(inherited) = anchor.return_focus {
        *return_focus = inherited;
    } else if ctx.open_menu().is_none() {
        *return_focus = ctx.focus_target().or(anchor.refocus);
    }
    let return_focus = *return_focus;
    let session = Arc::new(Mutex::new(Session {
        inside: false,
        armed: keyboard,
        origin: anchor.pointer,
    }));
    let menu = MenuWidget::new(
        spec,
        Chain {
            owner,
            root: None,
            parent: None,
            return_focus,
            on_closed: anchor.on_closed,
            on_step: anchor.on_step,
            session,
        },
        if keyboard || anchor.highlight.is_some() {
            OnReady::FocusFirst
        } else {
            OnReady::Focus
        },
        anchor.min_width,
        anchor.highlight,
    );
    OPENED.with(|n| n.set(n.get() + 1));
    ctx.create_menu(popup_menu(menu), anchor.at);
}

/// `menu` as a popup layer's root: it asks for a blur behind its panel (honoured when the
/// compositor offers one; see [`BackdropProp`]).
fn popup_menu(menu: MenuWidget) -> NewWidget<MenuWidget> {
    NewWidget::new(menu).with_props(PopupBlur(Some(PopupBlurRequest {
        radius: RADIUS,
        arrow: 0.0,
    })))
}

/// State one menu and its submenus share.
#[derive(Debug)]
struct Session {
    /// During a press: whether a menu checked so far contains it.
    inside: bool,
    /// Releasing the button over an item chooses it.
    armed: bool,
    /// Where the menu opened, in window coordinates.
    origin: Point,
}

/// Where a menu sits among its session's menus.
#[derive(Debug, Clone)]
struct Chain {
    /// The widget that opened the root menu; it receives [`Chosen`].
    owner: WidgetId,
    /// The root menu, for submenus (`None`: this is the root).
    root: Option<WidgetId>,
    /// The menu this submenu opens from, and the row that opened it.
    parent: Option<(WidgetId, WidgetId)>,
    /// What had keyboard focus before the root menu opened.
    return_focus: Option<WidgetId>,
    /// What the owner wants to know when the menus close.
    on_closed: Option<OnClosed>,
    /// Set for a menu bar's menus.
    on_step: Option<OnStep>,
    session: Arc<Mutex<Session>>,
}

impl Chain {
    fn session(&self) -> std::sync::MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What a menu does once its rows are in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnReady {
    /// Nothing (a submenu the pointer opened: focus stays in its parent).
    Nothing,
    /// Take keyboard focus.
    Focus,
    /// Highlight the first enabled item and focus it.
    FocusFirst,
}

/// Type-select: keys typed in quick succession form a prefix to look for.
#[derive(Debug, Default)]
pub(crate) struct TypeSelect {
    typed: String,
    last: Option<Instant>,
}

impl TypeSelect {
    /// Whether a key typed at `now` would add to the prefix rather than start over.
    pub(crate) fn is_typing(&self, now: Instant) -> bool {
        self.last
            .is_some_and(|last| now.duration_since(last) <= TYPE_SELECT_RESET)
    }

    /// Adds `text` typed at `now` and returns the prefix to look for (lower-case).
    pub(crate) fn push(&mut self, text: &str, now: Instant) -> &str {
        if !self.is_typing(now) {
            self.typed.clear();
        }
        self.last = Some(now);
        self.typed.extend(text.chars().flat_map(char::to_lowercase));
        &self.typed
    }
}

/// The contexts a menu acts from: events (pointer, keys) and its rows' actions
/// (assistive technology clicks, rows being added).
trait MenuCtx {
    fn id(&self) -> WidgetId;
    fn focus(&mut self, id: WidgetId);
    fn later(&mut self, id: WidgetId, f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static);
    fn attach(&mut self, menu: NewWidget<MenuWidget>, at: Point);
    fn drop_layer(&mut self, id: WidgetId);
    fn window_point(&self, p: Point) -> Point;
    fn width(&self) -> f64;
    fn repaint(&mut self);
}

macro_rules! menu_ctx {
    ($($ctx:ty),+) => {$(
        impl MenuCtx for $ctx {
            fn id(&self) -> WidgetId {
                self.widget_id()
            }
            fn focus(&mut self, id: WidgetId) {
                self.set_focus(id);
            }
            fn later(
                &mut self,
                id: WidgetId,
                f: impl FnOnce(WidgetMut<'_, dyn Widget>) + Send + 'static,
            ) {
                self.mutate_later(id, f);
            }
            fn attach(&mut self, menu: NewWidget<MenuWidget>, at: Point) {
                self.create_attached_layer(LayerType::Other, menu, at);
            }
            fn drop_layer(&mut self, id: WidgetId) {
                self.remove_layer(id);
            }
            fn window_point(&self, p: Point) -> Point {
                self.to_window(p)
            }
            fn width(&self) -> f64 {
                self.border_box().width()
            }
            fn repaint(&mut self) {
                self.request_paint_only();
                self.request_accessibility_update();
            }
        }
    )+};
}

menu_ctx!(EventCtx<'_>, ActionCtx<'_>);

/// The Masonry widget of one open menu: the panel, with a row widget per item.
pub(crate) struct MenuWidget {
    spec: MenuSpec,
    rows: Vec<WidgetPod<MenuRowWidget>>,
    /// Each row's top edge, from the last layout.
    tops: Vec<f64>,
    chain: Chain,
    on_ready: OnReady,
    /// The menu is at least this wide.
    min_width: f64,
    /// The row under the pointer or the keyboard (keyboard focus is on it).
    highlighted: Option<usize>,
    /// The open submenu: the row it opened from, and its layer.
    submenu: Option<(usize, WidgetId)>,
    type_select: TypeSelect,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for MenuWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuWidget")
            .field("items", &self.spec.0.len())
            .field("highlighted", &self.highlighted)
            .field("submenu", &self.submenu)
            .finish_non_exhaustive()
    }
}

impl MenuWidget {
    fn new(
        spec: MenuSpec,
        chain: Chain,
        on_ready: OnReady,
        min_width: f64,
        highlighted: Option<usize>,
    ) -> Self {
        let rows = spec
            .0
            .iter()
            .map(|item| {
                NewWidget::new(MenuRowWidget::new(item.clone()))
                    .disabled(!item.enabled)
                    .to_pod()
            })
            .collect();
        Self {
            spec,
            rows,
            tops: Vec::new(),
            chain,
            on_ready,
            min_width,
            highlighted,
            submenu: None,
            type_select: TypeSelect::default(),
            replayer: ImagingReplayer::new(),
        }
    }

    fn row_height(item: &ItemSpec) -> f64 {
        match item.kind {
            SpecKind::Separator => SEPARATOR_HEIGHT,
            _ => ITEM_HEIGHT,
        }
    }

    /// The row at local `p`, if `p` is inside the menu.
    fn row_at(&self, p: Point, size: Size) -> Option<usize> {
        if p.x < 0.0 || p.x >= size.width {
            return None;
        }
        self.spec.0.iter().enumerate().find_map(|(i, item)| {
            let top = *self.tops.get(i)?;
            (p.y >= top && p.y < top + Self::row_height(item)).then_some(i)
        })
    }

    fn selectable(&self, i: usize) -> bool {
        self.spec.0.get(i).is_some_and(ItemSpec::selectable)
    }

    /// The next selectable row after `from` in `step` direction (from the ends if `from`
    /// is `None`).
    fn step(&self, from: Option<usize>, forward: bool) -> Option<usize> {
        let n = self.spec.0.len();
        let mut order: Box<dyn Iterator<Item = usize>> = match (from, forward) {
            (None, true) => Box::new(0..n),
            (None, false) => Box::new((0..n).rev()),
            (Some(i), true) => Box::new(i + 1..n),
            (Some(i), false) => Box::new((0..i).rev()),
        };
        order.find(|&i| self.selectable(i))
    }

    /// Highlights row `i` (or nothing) and moves keyboard focus to it. Clearing the
    /// highlight leaves focus alone: the pointer moved to another menu, which takes it.
    fn highlight(&mut self, ctx: &mut impl MenuCtx, i: Option<usize>) {
        let i = i.filter(|&i| self.selectable(i));
        if let Some(i) = i {
            ctx.focus(self.rows[i].id());
        }
        if self.highlighted == i {
            return;
        }
        for (row, on) in [(self.highlighted, false), (i, true)] {
            if let Some(row) = row {
                ctx.later(self.rows[row].id(), move |mut w| {
                    MenuRowWidget::set_highlighted(&mut w.downcast(), on);
                });
            }
        }
        self.highlighted = i;
        ctx.repaint();
    }

    /// Opens row `i`'s submenu (closing another one). `keyboard`: highlight its first item
    /// and move focus into it.
    fn open_submenu(&mut self, ctx: &mut impl MenuCtx, i: usize, keyboard: bool) {
        let Some(SpecKind::Submenu(spec)) = self.spec.0.get(i).map(|s| s.kind.clone()) else {
            return;
        };
        if spec.0.is_empty() {
            return;
        }
        if let Some((open, sub)) = self.submenu {
            if open == i {
                if keyboard {
                    ctx.later(sub, |mut w| MenuWidget::focus_first(&mut w.downcast()));
                }
                return;
            }
            self.close_submenu(ctx);
        }
        let top = self.tops.get(i).copied().unwrap_or(0.0);
        let at = ctx.window_point(Point::new(ctx.width() - SUBMENU_OVERLAP, top - PADDING_Y));
        let chain = Chain {
            root: Some(self.chain.root.unwrap_or(ctx.id())),
            parent: Some((ctx.id(), self.rows[i].id())),
            ..self.chain.clone()
        };
        let on_ready = if keyboard {
            OnReady::FocusFirst
        } else {
            OnReady::Nothing
        };
        let menu = popup_menu(MenuWidget::new(spec, chain, on_ready, 0.0, None));
        self.submenu = Some((i, menu.id()));
        ctx.attach(menu, at);
        ctx.repaint();
    }

    fn close_submenu(&mut self, ctx: &mut impl MenuCtx) {
        if let Some((_, sub)) = self.submenu.take() {
            ctx.drop_layer(sub);
            ctx.repaint();
        }
    }

    /// Closes every menu of the session, gives focus back, and tells the owner `chosen`.
    fn close_all(&self, ctx: &mut impl MenuCtx, chosen: Option<usize>) {
        if let Some(focus) = self.chain.return_focus {
            ctx.focus(focus);
        }
        let root = self.chain.root.unwrap_or(ctx.id());
        let on_closed = self.chain.on_closed;
        ctx.later(self.chain.owner, move |mut owner| {
            // The owner may have replaced this menu with a new one already.
            let current = owner.ctx.get_attached_layer::<MenuWidget>() == Some(root);
            if current {
                owner.ctx.remove_layer(root);
            }
            match on_closed {
                // The owner reports the choice in its own action type.
                Some(on_closed) => {
                    if current {
                        on_closed(owner, chosen);
                    }
                }
                None => {
                    if let Some(id) = chosen {
                        owner.ctx.submit_action::<Chosen>(Chosen(id));
                    }
                }
            }
        });
    }

    /// Row `i` was clicked, released on, or activated by key: open its submenu or choose it.
    fn activate(&mut self, ctx: &mut impl MenuCtx, i: usize, keyboard: bool) {
        let Some(item) = self.spec.0.get(i).filter(|s| s.selectable()) else {
            return;
        };
        match item.kind {
            SpecKind::Action => self.close_all(ctx, Some(item.id)),
            SpecKind::Submenu(_) => {
                self.highlight(ctx, Some(i));
                self.open_submenu(ctx, i, keyboard);
            }
            SpecKind::Separator => {}
        }
    }

    /// Leaves this submenu for its parent (Left).
    fn back_to_parent(&self, ctx: &mut impl MenuCtx) {
        let Some((parent, row)) = self.chain.parent else {
            return;
        };
        ctx.focus(row);
        ctx.later(parent, |mut w| {
            let mut menu = w.downcast::<MenuWidget>();
            if let Some((_, sub)) = menu.widget.submenu.take() {
                menu.ctx.remove_layer(sub);
            }
            menu.ctx.request_paint_only();
        });
    }

    /// Whether row `i` opens a submenu with items in it.
    fn has_submenu(&self, i: usize) -> bool {
        matches!(
            self.spec.0.get(i).map(|s| &s.kind),
            Some(SpecKind::Submenu(spec)) if !spec.0.is_empty()
        )
    }

    /// Leaves for the bar's next (`forward`) or previous menu, in a menu bar's menus.
    fn step_bar(&self, ctx: &mut impl MenuCtx, forward: bool) {
        if let Some(on_step) = self.chain.on_step {
            ctx.later(self.chain.owner, move |owner| on_step(owner, forward));
        }
    }

    /// Highlights and focuses the first item of a submenu that is already up (Right on
    /// the item that the pointer opened it from). A mutation can't move focus, so a row
    /// reports ready again, as on opening.
    fn focus_first(this: &mut WidgetMut<'_, Self>) {
        this.widget.on_ready = OnReady::FocusFirst;
        if let Some(row) = this.widget.rows.first_mut() {
            this.ctx
                .get_mut(row)
                .ctx
                .submit_action::<RowEvent>(RowEvent::Ready);
        }
    }

    fn on_key(&mut self, ctx: &mut EventCtx<'_>, key: &Key, modifiers: Modifiers) {
        match key {
            Key::Named(NamedKey::ArrowDown) => {
                let next = self.step(self.highlighted, true).or(self.highlighted);
                self.highlight(ctx, next);
            }
            Key::Named(NamedKey::ArrowUp) => {
                let next = self.step(self.highlighted, false).or(self.highlighted);
                self.highlight(ctx, next);
            }
            Key::Named(NamedKey::Home) => {
                let first = self.step(None, true);
                self.highlight(ctx, first);
            }
            Key::Named(NamedKey::End) => {
                let last = self.step(None, false);
                self.highlight(ctx, last);
            }
            Key::Named(NamedKey::ArrowRight) => match self.highlighted {
                Some(i) if self.has_submenu(i) => self.open_submenu(ctx, i, true),
                _ => self.step_bar(ctx, true),
            },
            Key::Named(NamedKey::ArrowLeft) => {
                if self.chain.parent.is_some() {
                    self.back_to_parent(ctx);
                } else {
                    self.step_bar(ctx, false);
                }
            }
            Key::Named(NamedKey::Enter) => match self.highlighted {
                Some(i) => self.activate(ctx, i, true),
                None => self.close_all(ctx, None),
            },
            Key::Character(c) if c == " " && !self.type_select.is_typing(Instant::now()) => {
                match self.highlighted {
                    Some(i) => self.activate(ctx, i, true),
                    None => self.close_all(ctx, None),
                }
            }
            Key::Named(NamedKey::Escape) => self.close_all(ctx, None),
            Key::Character(c)
                if !modifiers.intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT) =>
            {
                let prefix = self.type_select.push(c, Instant::now()).to_owned();
                let found =
                    self.spec.0.iter().position(|s| {
                        s.selectable() && s.label.to_lowercase().starts_with(&prefix)
                    });
                if found.is_some() {
                    self.highlight(ctx, found);
                }
            }
            _ => {}
        }
    }

    fn on_pointer(&mut self, ctx: &mut EventCtx<'_>, event: &PointerEvent) {
        let size = ctx.border_box().size();
        let local = |ctx: &EventCtx<'_>, p: PhysicalPosition<f64>| ctx.local_position(p);
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let p = local(ctx, state.position);
                let inside = Rect::from_origin_size(Point::ORIGIN, size).contains(p);
                let deepest = self.submenu.is_none();
                let mut session = self.chain.session();
                if self.chain.root.is_none() {
                    session.inside = inside;
                } else {
                    session.inside |= inside;
                }
                session.armed = true;
                let outside_all = deepest && !session.inside;
                drop(session);
                if outside_all {
                    self.close_all(ctx, None);
                }
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let p = local(ctx, current.position);
                {
                    let mut session = self.chain.session();
                    let window = ctx.to_window(p);
                    if !session.armed && (window - session.origin).hypot() > DRAG_THRESHOLD {
                        session.armed = true;
                    }
                }
                match self.row_at(p, size) {
                    Some(i) if self.selectable(i) => {
                        self.highlight(ctx, Some(i));
                        match self.spec.0[i].kind {
                            SpecKind::Submenu(_) => self.open_submenu(ctx, i, false),
                            _ => self.close_submenu(ctx),
                        }
                    }
                    Some(_) => {
                        self.close_submenu(ctx);
                        self.highlight(ctx, None);
                    }
                    // Outside: keep the item whose submenu is open highlighted.
                    None if self.submenu.is_some() => {}
                    None => self.highlight(ctx, None),
                }
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let p = local(ctx, state.position);
                let armed = self.chain.session().armed;
                if armed
                    && let Some(i) = self.row_at(p, size)
                    && matches!(self.spec.0[i].kind, SpecKind::Action)
                {
                    self.activate(ctx, i, false);
                }
            }
            PointerEvent::Cancel(..) if self.chain.root.is_none() => self.close_all(ctx, None),
            _ => {}
        }
    }
}

impl UsesProperty<ThemeProp> for MenuWidget {}
impl UsesProperty<BackdropProp> for MenuWidget {}

impl Widget for MenuWidget {
    type Action = NoAction;

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) => {
                if key.state.is_down() {
                    self.on_key(ctx, &key.key, key.modifiers);
                }
                // Keys stay in the menu: no Tab focus moves, no default button.
                ctx.set_handled();
            }
            TextEvent::WindowFocusChange(false) if self.chain.root.is_none() => {
                self.close_all(ctx, None);
            }
            _ => {}
        }
    }

    fn on_action(
        &mut self,
        ctx: &mut ActionCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        action: &ErasedAction,
        source: WidgetId,
    ) {
        let Some(event) = action.downcast_ref::<RowEvent>() else {
            return;
        };
        ctx.set_handled();
        let Some(i) = self.rows.iter().position(|r| r.id() == source) else {
            return;
        };
        match event {
            RowEvent::Ready => match std::mem::replace(&mut self.on_ready, OnReady::Nothing) {
                OnReady::Nothing => {}
                OnReady::Focus => ctx.request_focus(),
                OnReady::FocusFirst => {
                    let first = self.highlighted.or_else(|| self.step(None, true));
                    self.highlighted = None;
                    self.highlight(ctx, first);
                    if first.is_none() {
                        ctx.request_focus();
                    }
                }
            },
            RowEvent::Clicked => self.activate(ctx, i, true),
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for row in &mut self.rows {
            ctx.register_child(row);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::FocusChanged(_) | Update::ChildFocusChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>()
            || property_type == TypeId::of::<BackdropProp>()
        {
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
                let mut width: f64 = (MIN_WIDTH - 2.0 * RIM).max(self.min_width - 2.0 * RIM);
                for row in &mut self.rows {
                    let w = ctx.compute_length(
                        row,
                        LenDef::MaxContent,
                        LayoutSize::maybe(Axis::Vertical, None),
                        Axis::Horizontal,
                        None,
                    );
                    width = width.max(w.get());
                }
                width + 2.0 * RIM
            }
            Axis::Vertical => {
                2.0 * PADDING_Y + self.spec.0.iter().map(Self::row_height).sum::<f64>()
            }
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let width = size.width - 2.0 * RIM;
        let mut y = PADDING_Y;
        self.tops.clear();
        for (row, item) in self.rows.iter_mut().zip(self.spec.0.iter()) {
            let h = Self::row_height(item);
            ctx.run_layout(row, Size::new(width, h));
            ctx.place_child(row, Point::new(RIM, y));
            self.tops.push(y);
            y += h;
        }
        ctx.clear_baselines();
        ctx.set_paint_insets(Insets::new(
            SHADOW_REACH,
            SHADOW_REACH,
            SHADOW_REACH,
            SHADOW_REACH + SHADOW_OFFSET,
        ));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let BackdropProp(backdrop) = *props.get::<BackdropProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        // The shadow falls outside the box, in the paint insets.
        let mut scene = Scene::new(size.width, size.height);
        menu_panel(
            &mut scene,
            Rect::from_origin_size(Point::ORIGIN, size),
            theme.variant,
            backdrop,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Menu
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        self.rows.iter().map(WidgetPod::id).collect()
    }

    fn accepts_focus(&self) -> bool {
        // Focus comes by request only (the menu takes it on opening); Tab never lands here.
        false
    }

    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}

impl Layer for MenuWidget {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        self.on_pointer(ctx, event);
    }
}

/// Room an item's signal glyph takes at its right: the fan, and the padlock with its gap.
fn signal_width(signal: Signal) -> f64 {
    if signal.locked {
        LOCK_WIDTH + ACCESSORY_GAP + SIGNAL_WIDTH
    } else {
        SIGNAL_WIDTH
    }
}

/// What a row tells its menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowEvent {
    /// The row is in the tree: the menu can take focus now.
    Ready,
    /// An assistive technology clicked the row.
    Clicked,
}

/// One row of a menu: an item, a submenu item or a separator.
pub(crate) struct MenuRowWidget {
    spec: ItemSpec,
    label: TextLine,
    shortcut: Option<TextLine>,
    highlighted: bool,
    replayer: ImagingReplayer,
}

impl std::fmt::Debug for MenuRowWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuRowWidget")
            .field("label", &self.spec.label)
            .field("highlighted", &self.highlighted)
            .finish_non_exhaustive()
    }
}

impl MenuRowWidget {
    fn new(spec: ItemSpec) -> Self {
        Self {
            label: TextLine::new(spec.label.clone(), TextStyle::Body),
            shortcut: spec
                .shortcut
                .as_ref()
                .map(|s| TextLine::new(s.label(), TextStyle::Body)),
            spec,
            highlighted: false,
            replayer: ImagingReplayer::new(),
        }
    }

    fn set_highlighted(this: &mut WidgetMut<'_, Self>, on: bool) {
        if this.widget.highlighted != on {
            this.widget.highlighted = on;
            this.ctx.request_paint_only();
        }
    }

    fn mark(&self) -> MenuMark {
        match self.spec.toggle {
            MenuToggle::Check(CheckState::On) => MenuMark::Check,
            MenuToggle::Check(CheckState::Mixed) => MenuMark::Dash,
            MenuToggle::Radio(true) => MenuMark::Dot,
            _ => MenuMark::None,
        }
    }

    fn is_submenu(&self) -> bool {
        matches!(self.spec.kind, SpecKind::Submenu(_))
    }
}

impl UsesProperty<ThemeProp> for MenuRowWidget {}

impl Widget for MenuRowWidget {
    type Action = RowEvent;

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            ctx.submit_action::<RowEvent>(RowEvent::Clicked);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => ctx.submit_action::<RowEvent>(RowEvent::Ready),
            Update::FontsChanged => {
                self.label.invalidate();
                if let Some(s) = &mut self.shortcut {
                    s.invalidate();
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
            Axis::Horizontal if matches!(self.spec.kind, SpecKind::Separator) => 0.0,
            Axis::Horizontal => {
                let label = self.label.size(ctx.text_contexts()).width;
                let shortcut = match &mut self.shortcut {
                    Some(s) => SHORTCUT_GAP + s.size(ctx.text_contexts()).width,
                    None => 0.0,
                };
                let shortcut = match self.spec.signal {
                    Some(signal) => shortcut + SHORTCUT_GAP + signal_width(signal),
                    None => shortcut,
                };
                let trailing = if self.is_submenu() {
                    SHORTCUT_GAP.max(shortcut) + ARROW_WIDTH
                } else {
                    shortcut + TRAILING_INSET
                };
                (LABEL_INSET + label + trailing).ceil()
            }
            Axis::Vertical => match self.spec.kind {
                SpecKind::Separator => SEPARATOR_HEIGHT,
                _ => ITEM_HEIGHT,
            },
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let text = self.label.size(ctx.text_contexts());
        if let Some(s) = &mut self.shortcut {
            s.size(ctx.text_contexts());
        }
        let baseline = ((size.height - text.height) / 2.0).round() + self.label.baseline();
        ctx.set_baselines(baseline, baseline);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let variant = theme.variant;
        let size = ctx.content_box().size();
        let rect = Rect::from_origin_size(Point::ORIGIN, size);
        let p = Palette::new(variant, Backdrop::Opaque);
        let mut scene = Scene::new(size.width, size.height);
        if matches!(self.spec.kind, SpecKind::Separator) {
            menu_separator(&mut scene, rect, variant);
            self.replayer.replay(&scene, ctx.scale_factor(), painter);
            return;
        }
        let enabled = self.spec.enabled && !ctx.is_disabled();
        let highlighted = self.highlighted && enabled;
        if highlighted {
            menu_highlight(&mut scene, rect, variant);
        }
        let (label, shortcut) = match (enabled, highlighted) {
            (false, _) => (p.disabled_label, p.disabled_label),
            (true, true) => (p.highlighted_label, p.highlighted_label),
            (true, false) => (p.label, p.shortcut),
        };
        let mark = Rect::from_center_size(
            Point::new(LABEL_INSET / 2.0, size.height / 2.0),
            Size::new(MARK_SIZE, MARK_SIZE),
        );
        menu_mark(&mut scene, mark, self.mark(), label);
        let right = if self.is_submenu() {
            submenu_arrow(
                &mut scene,
                Rect::new(size.width - ARROW_WIDTH, 0.0, size.width, size.height),
                label,
            );
            size.width - ARROW_WIDTH
        } else {
            size.width - TRAILING_INSET
        };
        if let Some(signal) = self.spec.signal {
            // The fan at the right edge, the padlock (a network that needs a password)
            // before it.
            let fan = Rect::from_center_size(
                Point::new(right - SIGNAL_WIDTH / 2.0, size.height / 2.0),
                Size::new(SIGNAL_WIDTH, SIGNAL_HEIGHT),
            );
            menu_signal(&mut scene, fan, signal.bars, shortcut);
            if signal.locked {
                let lock = Rect::from_center_size(
                    Point::new(fan.x0 - ACCESSORY_GAP - LOCK_WIDTH / 2.0, size.height / 2.0),
                    Size::new(LOCK_WIDTH, LOCK_HEIGHT),
                );
                menu_lock(&mut scene, lock, shortcut);
            }
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let text = self.label.size(ctx.text_contexts());
        let y = ((size.height - text.height) / 2.0).round();
        self.label
            .paint(painter, Affine::translate(Vec2::new(LABEL_INSET, y)), label);
        if let Some(s) = &mut self.shortcut {
            let w = s.size(ctx.text_contexts()).width;
            s.paint(
                painter,
                Affine::translate(Vec2::new((right - w).round(), y)),
                shortcut,
            );
        }
    }

    fn accessibility_role(&self) -> Role {
        match self.spec.toggle {
            MenuToggle::None => Role::MenuItem,
            MenuToggle::Check(_) => Role::MenuItemCheckBox,
            MenuToggle::Radio(_) => Role::MenuItemRadio,
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if matches!(self.spec.kind, SpecKind::Separator) {
            node.set_hidden();
            return;
        }
        node.set_label(self.spec.label.clone());
        node.add_action(Action::Click);
        if let Some(s) = &self.spec.shortcut {
            node.set_keyboard_shortcut(s.spoken());
        }
        if let Some(signal) = self.spec.signal {
            node.set_description(signal.spoken());
        }
        match self.spec.toggle {
            MenuToggle::None => {}
            MenuToggle::Check(state) => node.set_toggled(match state {
                CheckState::Off => Toggled::False,
                CheckState::On => Toggled::True,
                CheckState::Mixed => Toggled::Mixed,
            }),
            MenuToggle::Radio(on) => {
                node.set_toggled(if on { Toggled::True } else { Toggled::False })
            }
        }
        if self.is_submenu() {
            node.set_has_popup(HasPopup::Menu);
        }
        if self.highlighted {
            node.set_selected(true);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.spec.label.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn type_select_starts_over_after_a_pause() {
        let mut t = TypeSelect::default();
        let t0 = Instant::now();
        assert_eq!(t.push("C", t0), "c");
        assert_eq!(t.push("o", t0 + Duration::from_millis(200)), "co");
        let later = t0 + Duration::from_millis(200) + TYPE_SELECT_RESET * 2;
        assert_eq!(t.push("p", later), "p");
    }
}
