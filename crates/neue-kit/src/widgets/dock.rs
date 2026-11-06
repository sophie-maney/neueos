//! The dock (tasks 3.16 and 3.17a): [`crate::desktop::Dock`] draws the glass shelf
//! (`neue_paint::materials::dock_shelf`) with a tile per app standing on it: the app's
//! icon (reflected in the glass at the bottom), a light under it while the app runs, a
//! bounce while it launches or asks for attention, and icons growing under the pointer
//! when magnification is on. It stands on the bottom, left or right edge.
//!
//! The dock widget paints everything and takes the pointer; each tile is also a child
//! widget ([`DockTileWidget`]) that only takes keyboard focus and speaks for its tile to
//! assistive technology. Keys pressed on a tile bubble up to the dock, which knows the tile
//! by the child's id.
//!
//! Where everything goes is [`DockLayout`]'s business (`dock_layout.rs`). The dock reports
//! where its icons are whenever they move ([`Dock::on_placed`](crate::desktop::Dock)), so
//! the shell can tell the compositor where each app's windows minimise to.
//!
//! Editing (task 3.17b, ADR 0094): a tile pressed and moved past [`DRAG_THRESHOLD`] is
//! dragged. Its icon follows the pointer inside the surface (which keeps
//! [`DRAG_ROOM`](neue_theme::materials::dock::DRAG_ROOM) beyond the icons for it), the others open a gap where it would land, and dropping it
//! asks the app to move it there; dragged more than `REMOVE_DISTANCE` off the dock, the
//! gap closes and dropping it asks the app to take it off. The dock never reorders its
//! tiles itself: the app answers with new tiles, and every tile slides from where it was
//! shown to its new place ([`SLIDE`]), so a drop, a refusal and tiles coming and going all
//! move smoothly. A secondary click, the Menu key or Shift-F10 on a tile, or an assistive
//! technology asks the app for the tile's menu, which it shows with
//! `Handle::show_menu`, centred above the icon (beside it on the sides).

use std::any::TypeId;
use std::collections::HashMap;
use std::time::Duration;

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx,
    UsesProperty, Widget, WidgetId, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, RoundedRect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::icon::{Icon, icon};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::desktop::Picture;
use neue_paint::materials::dock_shelf::{
    ShelfStyle, dock_divider, dock_shelf_with, minimised_window, placeholder_icon, reflected,
    running_indicator,
};
use neue_paint::materials::focus_ring;
use neue_paint::{Backdrop, Scene};
use neue_platform::Side;
use neue_theme::materials::dock::{
    ATTENTION_REST, BOUNCE_HEIGHT, DRAG_THRESHOLD, FALL, MAGNIFY, MENU_GAP, REMOVING_OPACITY, RISE,
    RISE_TURN, SLIDE, STILL_LIFT,
};
use neue_theme::materials::dock_shelf::INDICATOR;
use neue_theme::motion::SpringValue;
use neue_theme::{Motion, SemanticColors, Theme};

use super::dock_layout::{DockEdge, DockLayout, DockPlacement};
use super::menu::{self, Anchor, Chosen};
use crate::menu::MenuSpec;
use crate::theme::ThemeProp;

/// Whether a dock icon bounces, and why.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DockBounce {
    /// Standing still.
    #[default]
    None,
    /// The app is starting: it bounces without rest until told otherwise.
    Launch,
    /// A window of the app asks for attention: it bounces with a rest between bounces.
    /// Under reduced motion it stands a little raised instead.
    Attention,
}

/// What a dock tile is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DockTileKind {
    /// An app, or anything shown as an icon on the shelf (the Trash).
    #[default]
    App,
    /// A minimised window: its thumbnail (the tile's `thumbnail`; a drawing of a window
    /// while there is none) with the app's icon (the tile's `icon`) in its corner. It
    /// can't be dragged.
    Minimised,
    /// The line between the dock's sections. It takes a tile's room, but is no tile to
    /// the pointer or the keyboard.
    Divider,
}

/// One app on the dock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DockTile {
    /// Identifies the tile across updates (its bounce goes on) and in clicks.
    pub key: String,
    /// The app's name, for assistive technology.
    pub name: String,
    /// The app's icon; `None` shows a plain placeholder tile.
    pub icon: Option<Icon>,
    /// The app has windows: the light under it is on.
    pub running: bool,
    pub bounce: DockBounce,
    pub kind: DockTileKind,
    /// The tile stays where it is: it can be clicked, but not dragged. (Minimised windows
    /// and dividers never can.)
    pub fixed: bool,
    /// A minimised window's picture; other kinds ignore it.
    pub thumbnail: Option<Picture>,
}

impl DockTile {
    /// Whether the tile can't be dragged.
    fn is_fixed(&self) -> bool {
        self.fixed || self.kind != DockTileKind::App
    }
}

/// What the dock emits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DockAction {
    /// A tile was clicked (or pressed from the keyboard, or by an assistive technology):
    /// its key.
    Clicked(String),
    /// The icons moved (tiles came or went, or magnification moved them): each tile's key
    /// and its icon's rect at rest on the shelf, in the dock's coordinates.
    Placed(Vec<(String, Rect)>),
    /// A tile was dragged to another place and dropped: its key and its index in the new
    /// order.
    Moved(String, usize),
    /// A tile was dragged off the dock and dropped there: its key.
    Removed(String),
    /// A tile's menu is wanted: its key. The app shows it with `Handle::show_menu`.
    Menu(String),
}

/// A press on a tile, which becomes a drag once the pointer moves far enough.
#[derive(Clone, Debug)]
struct Press {
    key: String,
    /// The tile can't be dragged.
    fixed: bool,
    /// Where the pointer went down, in the dock.
    at: Point,
    /// Where on the icon: a share of its size along the dock and away from the edge.
    grab: (f64, f64),
}

/// A tile being dragged.
#[derive(Clone, Debug)]
struct Drag {
    key: String,
    grab: (f64, f64),
    /// The pointer, in the dock (it may be beyond the surface).
    pointer: Point,
    /// Where the tile lands among the others if dropped now; `None`: off the dock.
    slot: Option<usize>,
}

/// A tile's menu, asked of the app and not shown yet.
#[derive(Clone, Debug)]
struct MenuRequest {
    key: String,
    /// The icon grown by [`MENU_GAP`], in window coordinates: the menu goes against it.
    anchor: Rect,
    side: Side,
    /// Asked for from the keyboard: the first item is highlighted.
    keyboard: bool,
}

/// Magnification: where the pointer is along the dock and how far the icons have grown.
#[derive(Clone, Debug)]
struct Magnify {
    /// The pointer along the dock; kept when it leaves, so the icons shrink where they are.
    pointer: Option<f64>,
    /// 0 at rest, 1 grown in full.
    amount: SpringValue,
}

/// Where a bouncing icon is in its bounce.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// At rest (or raised and still, under reduced motion).
    Still,
    Rising,
    Falling,
    /// Between two attention bounces, for what is left of the rest.
    Resting(Duration),
}

/// An icon's bounce: a rise on [`RISE`] that turns at [`RISE_TURN`] of the height, and a
/// fall on [`FALL`] that ends when the icon lands, as a ball hitting the floor. Stepped by
/// elapsed time, so it is the same at any frame rate.
#[derive(Clone, Debug)]
pub(crate) struct Bounce {
    lift: SpringValue,
    phase: Phase,
    kind: DockBounce,
}

impl Bounce {
    fn new() -> Self {
        Self {
            lift: SpringValue::new(0.0),
            phase: Phase::Still,
            kind: DockBounce::None,
        }
    }

    /// Bounces for `kind` from now on. A bounce under way finishes before the icon stops.
    /// Returns whether it moves.
    fn set(&mut self, kind: DockBounce, motion: Motion) -> bool {
        self.kind = kind;
        if motion.reduced {
            self.phase = Phase::Still;
            let lift = if kind == DockBounce::Attention {
                STILL_LIFT
            } else {
                0.0
            };
            self.lift.snap_to(lift);
            return false;
        }
        if self.phase == Phase::Still {
            if kind == DockBounce::None {
                // Raised while motion was reduced: down it comes.
                if self.lift.value() > 0.0 {
                    self.fall();
                }
            } else {
                self.rise();
            }
        }
        self.moving()
    }

    fn rise(&mut self) {
        self.phase = Phase::Rising;
        self.lift.animate_to(BOUNCE_HEIGHT, Some(RISE));
    }

    fn fall(&mut self) {
        self.phase = Phase::Falling;
        self.lift.animate_to(0.0, Some(FALL));
    }

    /// Moves on by `dt`. Returns whether it still moves.
    fn step(&mut self, dt: Duration, motion: Motion) -> bool {
        if motion.reduced {
            return self.set(self.kind, motion);
        }
        match self.phase {
            Phase::Still => {}
            Phase::Rising => {
                self.lift.step(dt);
                if self.lift.value() >= RISE_TURN * BOUNCE_HEIGHT || !self.lift.is_animating() {
                    self.fall();
                }
            }
            Phase::Falling => {
                self.lift.step(dt);
                if self.lift.value() <= 0.0 || !self.lift.is_animating() {
                    self.lift.snap_to(0.0);
                    match self.kind {
                        DockBounce::Launch => self.rise(),
                        DockBounce::Attention => self.phase = Phase::Resting(ATTENTION_REST),
                        DockBounce::None => self.phase = Phase::Still,
                    }
                }
            }
            Phase::Resting(left) => {
                if self.kind == DockBounce::None {
                    self.phase = Phase::Still;
                } else if dt >= left {
                    self.rise();
                } else {
                    self.phase = Phase::Resting(left - dt);
                }
            }
        }
        self.moving()
    }

    fn moving(&self) -> bool {
        self.phase != Phase::Still
    }

    /// How far above its place the icon is.
    pub(crate) fn lift(&self) -> f64 {
        self.lift.value().max(0.0)
    }
}

/// What a pointer move asks of the dock.
#[derive(Clone, Copy, Debug, Default)]
struct Hover {
    layout: bool,
    frames: bool,
}

/// One tile as the dock keeps it.
#[derive(Debug)]
struct Tile {
    key: String,
    icon: Option<Icon>,
    thumbnail: Option<Picture>,
    kind: DockTileKind,
    fixed: bool,
    running: bool,
    bounce: Bounce,
    /// How far the icon is from its place along the dock and away from the edge, sliding
    /// back to 0 after the order changed.
    slide: (SpringValue, SpringValue),
    pod: WidgetPod<DockTileWidget>,
}

impl Tile {
    fn new(tile: &DockTile) -> Self {
        Self {
            key: tile.key.clone(),
            icon: None,
            thumbnail: None,
            kind: tile.kind,
            fixed: tile.is_fixed(),
            running: false,
            bounce: Bounce::new(),
            slide: (SpringValue::new(0.0), SpringValue::new(0.0)),
            pod: WidgetPod::new(DockTileWidget::new(tile)),
        }
    }
}

/// The Masonry widget behind [`crate::desktop::Dock`].
#[derive(Debug)]
pub(crate) struct DockWidget {
    tiles: Vec<Tile>,
    pub(super) backdrop: Backdrop,
    pub(super) layout: DockLayout,
    magnify: Magnify,
    /// The tiles as shown, by index: in their order, but a dragged tile in its slot or
    /// nowhere (off the dock).
    order: Vec<usize>,
    /// Where the tiles shown are now, in `order`.
    placement: DockPlacement,
    /// The icons' rects last reported.
    placed: Vec<(String, Rect)>,
    /// The tile the pointer went down on.
    press: Option<Press>,
    drag: Option<Drag>,
    menu: Option<MenuRequest>,
    /// What had keyboard focus before a tile's menu opened.
    return_focus: Option<WidgetId>,
    replayer: ImagingReplayer,
}

impl DockWidget {
    pub(crate) fn new(tiles: Vec<DockTile>) -> Self {
        let mut dock = Self {
            tiles: Vec::new(),
            backdrop: Backdrop::Opaque,
            layout: DockLayout::default(),
            magnify: Magnify {
                pointer: None,
                amount: SpringValue::new(0.0),
            },
            order: (0..tiles.len()).collect(),
            placement: DockPlacement::default(),
            placed: Vec::new(),
            press: None,
            drag: None,
            menu: None,
            return_focus: None,
            replayer: ImagingReplayer::new(),
        };
        for tile in tiles {
            let mut kept = Tile::new(&tile);
            // Moving once added, with the theme known.
            kept.bounce.kind = tile.bounce;
            kept.icon = tile.icon;
            kept.thumbnail = tile.thumbnail;
            kept.running = tile.running;
            dock.tiles.push(kept);
        }
        dock
    }

    /// Shows `tiles` in this order. Tiles whose key was there before keep their bounce
    /// (and keyboard focus) and slide from where they were to their new place; others come
    /// and go.
    pub(crate) fn edit_tiles(this: &mut WidgetMut<'_, Self>, tiles: Vec<DockTile>) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        let motion = theme.motion();
        let shown: HashMap<String, (f64, f64)> = (0..this.widget.tiles.len())
            .filter_map(|i| Some((this.widget.tiles[i].key.clone(), this.widget.shown(i)?)))
            .collect();
        let mut old = std::mem::take(&mut this.widget.tiles);
        let mut moving = false;
        for tile in tiles {
            let kept = old.iter().position(|t| t.key == tile.key);
            let mut kept = match kept {
                Some(i) => {
                    let mut kept = old.remove(i);
                    DockTileWidget::edit(&mut this.ctx.get_mut(&mut kept.pod), &tile);
                    kept
                }
                None => {
                    this.ctx.children_changed();
                    Tile::new(&tile)
                }
            };
            kept.kind = tile.kind;
            kept.fixed = tile.is_fixed();
            kept.icon = tile.icon;
            kept.thumbnail = tile.thumbnail;
            kept.running = tile.running;
            if kept.bounce.kind != tile.bounce || !kept.bounce.moving() {
                kept.bounce.set(tile.bounce, motion);
            }
            moving |= kept.bounce.moving();
            this.widget.tiles.push(kept);
        }
        for gone in old {
            this.ctx.remove_child(gone.pod);
        }
        let dock = &mut *this.widget;
        // A press or drag on a tile that went away ends.
        if let Some(press) = &dock.press
            && dock.index_of(&press.key).is_none()
        {
            dock.press = None;
            dock.drag = None;
        }
        let from: Vec<_> = dock
            .tiles
            .iter()
            .map(|t| shown.get(&t.key).copied())
            .collect();
        moving |= dock.reflow(&from, motion);
        if moving {
            this.ctx.request_anim_frame();
        }
        this.ctx.request_layout();
        this.ctx.request_render();
    }

    /// The tile at `p` (dock coordinates): its icon and half the spacing on either side,
    /// along the dock.
    fn tile_at(&self, p: Point) -> Option<usize> {
        let (u, _) = self.layout.frame_of(p);
        self.order
            .get(self.placement.tile_at(u)?)
            .copied()
            .filter(|&i| self.tiles[i].kind != DockTileKind::Divider)
    }

    fn index_of(&self, key: &str) -> Option<usize> {
        self.tiles.iter().position(|t| t.key == key)
    }

    /// Whether tile `index` is the one being dragged.
    fn is_dragged(&self, index: usize) -> bool {
        self.drag
            .as_ref()
            .is_some_and(|d| self.tiles.get(index).is_some_and(|t| t.key == d.key))
    }

    /// The tiles as shown (see [`Self::order`]) and where they go. Off the dock, the
    /// others close up in the middle of the surface, which still has room for all.
    fn arrange(&self) -> (Vec<usize>, DockPlacement) {
        let count = self.tiles.len();
        let mut order: Vec<usize> = (0..count).collect();
        if let Some(drag) = &self.drag
            && let Some(index) = self.index_of(&drag.key)
        {
            order.retain(|&i| i != index);
            if let Some(slot) = drag.slot {
                order.insert(slot.min(order.len()), index);
            }
        }
        let mut placement = self.layout.place(
            order.len(),
            self.magnify.pointer,
            self.magnify.amount.value(),
        );
        #[expect(clippy::cast_precision_loss, reason = "a handful of tiles")]
        let missing = (count - order.len()) as f64;
        if missing > 0.0 {
            self.layout
                .shift(&mut placement, missing * self.layout.pitch() / 2.0);
        }
        (order, placement)
    }

    /// Tile `index`'s icon as drawn, before any bounce: under the pointer while dragged,
    /// else its place and how far it has yet to slide.
    fn drawn(&self, index: usize) -> Option<Rect> {
        let tile = self.tiles.get(index)?;
        if let Some(drag) = self.drag.as_ref().filter(|d| d.key == tile.key) {
            return Some(
                self.layout
                    .dragged(self.tiles.len(), drag.pointer, drag.grab),
            );
        }
        let at = self.order.iter().position(|&i| i == index)?;
        let place = *self.placement.icons.get(at)?;
        Some(
            place
                + self
                    .layout
                    .along(tile.slide.0.value(), tile.slide.1.value()),
        )
    }

    /// Where tile `index`'s icon is drawn: its middle from the dock's middle along it, and
    /// from the screen edge. The dock's middle stays put on the screen as tiles come and
    /// go, so this is where it is on the screen.
    fn shown(&self, index: usize) -> Option<(f64, f64)> {
        let (u, v) = self.layout.frame_of(self.drawn(index)?.center());
        Some((u - self.layout.middle(self.tiles.len()), v))
    }

    /// Arranges the tiles anew (their order or the drag changed); each tile with a place
    /// in `from` (by index, as [`Self::shown`] gave it) slides from there to its new place.
    /// Returns whether any slides.
    fn reflow(&mut self, from: &[Option<(f64, f64)>], motion: Motion) -> bool {
        (self.order, self.placement) = self.arrange();
        let middle = self.layout.middle(self.tiles.len());
        let mut moving = false;
        for (&index, place) in self.order.iter().zip(&self.placement.icons) {
            let (Some(Some((u0, v0))), Some(tile)) = (from.get(index), self.tiles.get_mut(index))
            else {
                continue;
            };
            let (u, v) = self.layout.frame_of(place.center());
            tile.slide.0.snap_to(u0 - (u - middle));
            tile.slide.1.snap_to(v0 - v);
            moving |= tile.slide.0.animate_to(0.0, motion.spring(SLIDE));
            moving |= tile.slide.1.animate_to(0.0, motion.spring(SLIDE));
        }
        moving
    }

    /// Where every tile is shown, by index (see [`Self::shown`]).
    fn snapshot(&self) -> Vec<Option<(f64, f64)>> {
        (0..self.tiles.len()).map(|i| self.shown(i)).collect()
    }

    /// Where a tile dragged with the pointer at `p`, held `grab` of the way across its
    /// icon, lands among the others: the place under its icon's middle, or `None` off the
    /// dock.
    fn slot_at(&self, p: Point, grab: (f64, f64)) -> Option<usize> {
        let count = self.tiles.len();
        if count == 0 || self.layout.is_off(p) {
            return None;
        }
        let (u, _) = self
            .layout
            .frame_of(self.layout.dragged(count, p, grab).center());
        let all = self
            .layout
            .place(count, self.magnify.pointer, self.magnify.amount.value());
        let first = all.spans.first().map_or(0.0, |s| s.0);
        Some(
            all.tile_at(u)
                .unwrap_or(if u < first { 0 } else { count - 1 }),
        )
    }

    /// The pointer moved to `p` (in the dock) with the button down: a press far enough
    /// from where it went down becomes a drag, and a drag follows it.
    fn drag_to(&mut self, ctx: &mut EventCtx<'_>, p: Point, motion: Motion) {
        let Some(press) = &self.press else {
            return;
        };
        if self.drag.is_none() {
            if press.fixed || (p - press.at).hypot() < DRAG_THRESHOLD {
                return;
            }
            self.drag = Some(Drag {
                key: press.key.clone(),
                grab: press.grab,
                pointer: press.at,
                slot: self.index_of(&press.key),
            });
        }
        let from = self.snapshot();
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        drag.pointer = p;
        let (grab, slot) = (drag.grab, drag.slot);
        let new = self.slot_at(p, grab);
        if new != slot {
            if let Some(drag) = self.drag.as_mut() {
                drag.slot = new;
            }
            if self.reflow(&from, motion) {
                ctx.request_anim_frame();
            }
            ctx.request_layout();
        }
        ctx.request_paint_only();
    }

    /// The drag ends: dropped (`true`) the app is asked to move the tile to its slot or
    /// take it off the dock; cancelled, it goes back. Either way it slides from under the
    /// pointer to its place, and the tiles close up behind it.
    fn end_drag(&mut self, ctx: &mut EventCtx<'_>, dropped: bool, motion: Motion) {
        let from = self.snapshot();
        let Some(drag) = self.drag.take() else {
            return;
        };
        let was = self.index_of(&drag.key);
        if self.reflow(&from, motion) {
            ctx.request_anim_frame();
        }
        ctx.request_layout();
        ctx.request_paint_only();
        if !dropped {
            return;
        }
        match drag.slot {
            None => ctx.submit_action::<DockAction>(DockAction::Removed(drag.key)),
            Some(slot) if Some(slot) != was => {
                ctx.submit_action::<DockAction>(DockAction::Moved(drag.key, slot));
            }
            Some(_) => {}
        }
    }

    /// Asks the app for tile `index`'s menu, to go against its icon as shown now.
    fn ask_menu(&mut self, ctx: &mut EventCtx<'_>, index: usize, keyboard: bool) {
        let Some(icon) = self.drawn(index) else {
            return;
        };
        let grown = icon.inflate(MENU_GAP, MENU_GAP);
        let anchor = Rect::from_origin_size(ctx.to_window(grown.origin()), grown.size());
        let side = match self.layout.edge {
            DockEdge::Bottom => Side::Top,
            DockEdge::Left => Side::Right,
            DockEdge::Right => Side::Left,
        };
        let key = self.tiles[index].key.clone();
        self.menu = Some(MenuRequest {
            key: key.clone(),
            anchor,
            side,
            keyboard,
        });
        ctx.submit_action::<DockAction>(DockAction::Menu(key));
    }

    /// Opens `spec` as tile `key`'s menu if the dock asked for it and the tile is still
    /// there. The tile's focus node owns the menu: it receives the item chosen. Returns
    /// its id.
    pub(crate) fn show_menu(
        this: &mut WidgetMut<'_, Self>,
        key: &str,
        spec: MenuSpec,
    ) -> Option<WidgetId> {
        let request = this.widget.menu.take().filter(|r| r.key == key)?;
        let index = this.widget.index_of(key)?;
        let dock = &mut *this.widget;
        let pod = &mut dock.tiles[index].pod;
        let owner = pod.id();
        let mut tile = this.ctx.get_mut(pod);
        let anchor = Anchor {
            beside: Some((request.anchor, request.side)),
            ..Anchor::at(request.anchor.origin())
        };
        menu::open(
            &mut tile.ctx,
            spec,
            anchor,
            request.keyboard,
            &mut dock.return_focus,
        );
        Some(owner)
    }

    /// The pointer moved to `at` (`None`: it left): the icons grow while it is over the
    /// dock and shrink once it is off, unless magnification is off or motion reduced.
    /// Returns whether the icons move: lay out again, and animate if `frames`.
    fn hover(&mut self, at: Option<Point>, motion: Motion) -> Hover {
        let mut hover = Hover::default();
        if !self.layout.magnifies() {
            return hover;
        }
        let over = at
            .filter(|&p| !motion.reduced && self.layout.is_over(self.tiles.len(), p))
            .map(|p| self.layout.frame_of(p).0);
        let target = if over.is_some() { 1.0 } else { 0.0 };
        let moved = over.is_some() && over != self.magnify.pointer;
        if let Some(u) = over {
            self.magnify.pointer = Some(u);
        }
        if self.magnify.amount.target() != target {
            hover.frames = self
                .magnify
                .amount
                .animate_to(target, motion.spring(MAGNIFY));
            hover.layout = true;
        }
        hover.layout |= moved;
        hover
    }

    /// The tile whose focus node `id` is.
    fn tile_of(&self, id: WidgetId) -> Option<usize> {
        self.tiles.iter().position(|t| t.pod.id() == id)
    }

    fn click(&self, ctx: &mut EventCtx<'_>, index: usize) {
        if let Some(tile) = self.tiles.get(index) {
            ctx.submit_action::<DockAction>(DockAction::Clicked(tile.key.clone()));
        }
    }

    /// The icons' rects, for tests.
    #[cfg(test)]
    pub(crate) fn icons(&self) -> Vec<Rect> {
        self.placement.icons.clone()
    }

    /// The tiles' bounces, for tests.
    #[cfg(test)]
    pub(crate) fn lifts(&self) -> Vec<f64> {
        self.tiles.iter().map(|t| t.bounce.lift()).collect()
    }

    /// The tiles' focus nodes, for tests.
    #[cfg(test)]
    pub(crate) fn tile_ids(&self) -> Vec<WidgetId> {
        self.tiles.iter().map(|t| t.pod.id()).collect()
    }

    /// The keys of the tiles laid out, in the order shown, for tests.
    #[cfg(test)]
    pub(crate) fn shown_keys(&self) -> Vec<String> {
        self.order
            .iter()
            .map(|&i| self.tiles[i].key.clone())
            .collect()
    }

    /// Each tile's key and its icon as drawn (before any bounce), for tests.
    #[cfg(test)]
    pub(crate) fn drawn_icons(&self) -> Vec<(String, Option<Rect>)> {
        (0..self.tiles.len())
            .map(|i| (self.tiles[i].key.clone(), self.drawn(i)))
            .collect()
    }
}

impl UsesProperty<ThemeProp> for DockWidget {}

impl Widget for DockWidget {
    type Action = DockAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for tile in &mut self.tiles {
            ctx.register_child(&mut tile.pod);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        // The pointer left the window (Masonry sends no pointer event for that).
        if matches!(event, Update::HoveredChanged(false)) {
            let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
            let hover = self.hover(None, theme.motion());
            if hover.frames {
                ctx.request_anim_frame();
            }
            if hover.layout {
                ctx.request_layout();
            }
        }
        if matches!(event, Update::WidgetAdded) {
            let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
            let mut moving = false;
            for tile in &mut self.tiles {
                moving |= tile.bounce.set(tile.bounce.kind, theme.motion());
            }
            if moving {
                ctx.request_anim_frame();
            }
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_layout();
            ctx.request_render();
            // Reduced motion switched on or off: the next frame settles each bounce.
            ctx.request_anim_frame();
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let dt = Duration::from_nanos(interval);
        let mut moving = false;
        for tile in &mut self.tiles {
            moving |= tile.bounce.step(dt, theme.motion());
            if theme.motion().reduced {
                tile.slide.0.finish();
                tile.slide.1.finish();
            } else {
                moving |= tile.slide.0.step(dt);
                moving |= tile.slide.1.step(dt);
            }
        }
        let amount = self.magnify.amount.value();
        if theme.motion().reduced {
            self.magnify.amount.snap_to(0.0);
        } else {
            moving |= self.magnify.amount.step(dt);
        }
        if self.magnify.amount.value() != amount {
            ctx.request_layout();
        }
        if moving {
            ctx.request_anim_frame();
        }
        ctx.request_paint_only();
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let motion = theme.motion();
        match event {
            PointerEvent::Move(update) => {
                let local = ctx.local_position(update.current.position);
                let hover = self.hover(Some(local), motion);
                if hover.frames {
                    ctx.request_anim_frame();
                }
                if hover.layout {
                    ctx.request_layout();
                }
                self.drag_to(ctx, local, motion);
            }
            PointerEvent::Down(down) if self.press.is_none() => {
                let local = ctx.local_position(down.state.position);
                let Some(index) = self.tile_at(local) else {
                    return;
                };
                if down.button == Some(PointerButton::Secondary) {
                    self.ask_menu(ctx, index, false);
                    ctx.set_handled();
                    return;
                }
                let Some(icon) = self.drawn(index) else {
                    return;
                };
                // Where on the icon it was taken, so it stays under the pointer that way.
                let (u, v) = self.layout.frame_of(local);
                let (uc, vc) = self.layout.frame_of(icon.center());
                let size = icon.width().max(1.0);
                let grab = (
                    (0.5 + (u - uc) / size).clamp(0.0, 1.0),
                    (0.5 + (v - vc) / size).clamp(0.0, 1.0),
                );
                self.press = Some(Press {
                    key: self.tiles[index].key.clone(),
                    fixed: self.tiles[index].fixed,
                    at: local,
                    grab,
                });
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(up) if up.button != Some(PointerButton::Secondary) => {
                let local = ctx.local_position(up.state.position);
                let Some(press) = self.press.take() else {
                    return;
                };
                if self.drag.is_some() {
                    self.end_drag(ctx, true, motion);
                } else {
                    let inside = ctx.content_box().contains(local);
                    let index = self.index_of(&press.key);
                    if inside && let Some(index) = index.filter(|&i| self.tile_at(local) == Some(i))
                    {
                        self.click(ctx, index);
                    }
                }
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) if self.press.take().is_some() => {
                self.end_drag(ctx, false, motion);
                ctx.request_paint_only();
            }
            _ => {}
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
        let Some(index) = self.tile_of(ctx.target()) else {
            return;
        };
        if is_press_key(&key.key) {
            if !key.state.is_down() {
                self.click(ctx, index);
            }
            ctx.set_handled();
        } else if is_menu_key(&key.key, key.modifiers) {
            if key.state.is_down() {
                self.ask_menu(ctx, index, true);
            }
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        let Some(index) = self.tile_of(ctx.target()) else {
            return;
        };
        match event.action {
            Action::Click => self.click(ctx, index),
            Action::ShowContextMenu => self.ask_menu(ctx, index, true),
            _ => return,
        }
        ctx.set_handled();
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let size = self.layout.size(self.tiles.len());
        Length::px(match axis {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
        (self.order, self.placement) = self.arrange();
        let mut places = vec![None; self.tiles.len()];
        for (&index, icon) in self.order.iter().zip(&self.placement.icons) {
            places[index] = Some(*icon);
        }
        for (i, place) in places.iter().enumerate() {
            // A tile dragged off the dock has its focus node under the pointer.
            let Some(icon) = place.or_else(|| self.drawn(i)) else {
                continue;
            };
            let tile = &mut self.tiles[i];
            ctx.run_layout(&mut tile.pod, icon.size());
            ctx.place_child(&mut tile.pod, icon.origin());
        }
        let placed: Vec<(String, Rect)> = self
            .order
            .iter()
            .zip(&self.placement.icons)
            .map(|(&i, icon)| (self.tiles[i].key.clone(), *icon))
            .collect();
        if placed != self.placed {
            self.placed = placed.clone();
            ctx.submit_action::<DockAction>(DockAction::Placed(placed));
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
        let layout = self.layout;
        let placement = &self.placement;
        // Bounce metrics are for 48 px icons.
        let outwards = layout.outwards() * layout.scale();
        let pressed = self
            .press
            .as_ref()
            .filter(|_| self.drag.is_none())
            .map(|p| p.key.as_str());
        let icons = |scene: &mut Scene| {
            for &i in &self.order {
                let tile = &self.tiles[i];
                if self.is_dragged(i) {
                    continue;
                }
                let Some(drawn) = self.drawn(i) else {
                    continue;
                };
                let rect = drawn + outwards * tile.bounce.lift();
                let dimmed = pressed == Some(tile.key.as_str());
                if dimmed {
                    scene.push_opacity(0.6);
                }
                draw_tile(scene, rect, tile, &theme, layout.edge);
                if dimmed {
                    scene.pop_layer();
                }
            }
        };
        // The shelf and the lights on it are drawn as at the bottom and turned onto the
        // edge; the icons stand upright wherever the dock is.
        let shelf = layout.shelf_rect(placement);
        let bottom = layout.edge == DockEdge::Bottom;
        // A flat shelf's outer corners are past the screen edge.
        scene.push_clip(&size.to_rect());
        scene.with_transform(layout.frame_transform(), |scene| {
            let style = if bottom {
                ShelfStyle::Perspective
            } else {
                ShelfStyle::Flat
            };
            dock_shelf_with(scene, shelf, style, theme.variant, self.backdrop, |scene| {
                // Only the shelf seen in perspective reflects the icons.
                if bottom {
                    reflected(scene, layout.base_line(), |scene| icons(scene));
                }
            });
            for (&i, span) in self.order.iter().zip(&placement.spans) {
                let tile = &self.tiles[i];
                if tile.running && !self.is_dragged(i) {
                    let u = (span.0 + span.1) / 2.0 + tile.slide.0.value();
                    running_indicator(
                        scene,
                        Point::new(u, layout.frame_depth() - INDICATOR.1 - 1.0),
                        theme.variant,
                    );
                }
            }
        });
        icons(&mut scene);
        // The dragged icon over everything, faded while dropping it takes it off.
        if let Some(drag) = &self.drag
            && let Some(index) = self.index_of(&drag.key)
            && let Some(rect) = self.drawn(index)
        {
            let off = drag.slot.is_none();
            if off {
                scene.push_opacity(REMOVING_OPACITY);
            }
            draw_tile(&mut scene, rect, &self.tiles[index], &theme, layout.edge);
            if off {
                scene.pop_layer();
            }
        }
        scene.pop_layer();
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
        // Thumbnails change and icons are placed at every magnified size: keep only what
        // this paint drew.
        self.replayer.evict_unused();
    }

    fn accessibility_role(&self) -> Role {
        Role::Toolbar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label("Dock");
    }

    fn children_ids(&self) -> ChildrenIds {
        self.tiles.iter().map(|t| t.pod.id()).collect()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("{} tiles", self.tiles.len()))
    }
}

/// Draws `tile` in its icon's `rect`: its icon (or the placeholder), a minimised window
/// (its thumbnail, or a drawing of one), or the divider.
fn draw_tile(scene: &mut Scene, rect: Rect, tile: &Tile, theme: &Theme, edge: DockEdge) {
    match tile.kind {
        DockTileKind::App => draw_icon(scene, rect, tile.icon.as_ref(), theme),
        DockTileKind::Minimised => {
            let thumbnail = tile.thumbnail.as_ref();
            minimised_window(scene, rect, theme.variant, thumbnail, |scene, badge| {
                if let Some(app) = &tile.icon {
                    draw_icon(scene, badge, Some(app), theme);
                }
            });
        }
        DockTileKind::Divider => dock_divider(scene, rect, edge == DockEdge::Bottom, theme.variant),
    }
}

/// Draws `icon` (or the placeholder) filling `rect`.
fn draw_icon(scene: &mut Scene, rect: Rect, icon_: Option<&Icon>, theme: &Theme) {
    match icon_ {
        Some(app) => {
            let color = SemanticColors::new(theme.variant).label;
            icon(scene, rect, app, color, theme.variant);
        }
        None => placeholder_icon(scene, rect, theme.variant),
    }
}

fn is_press_key(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ") || *key == Key::Named(NamedKey::Enter)
}

/// The Menu key, or Shift-F10.
fn is_menu_key(key: &Key, modifiers: Modifiers) -> bool {
    match key {
        Key::Named(NamedKey::ContextMenu) => true,
        Key::Named(NamedKey::F10) => modifiers == Modifiers::SHIFT,
        _ => false,
    }
}

/// A tile's keyboard focus and its node for assistive technology; the dock paints it,
/// except for the focus ring.
#[derive(Debug)]
pub(crate) struct DockTileWidget {
    kind: DockTileKind,
    name: String,
    running: bool,
    launching: bool,
    attention: bool,
}

impl DockTileWidget {
    fn new(tile: &DockTile) -> Self {
        Self {
            kind: tile.kind,
            name: tile.name.clone(),
            running: tile.running,
            launching: tile.bounce == DockBounce::Launch,
            attention: tile.bounce == DockBounce::Attention,
        }
    }

    fn edit(this: &mut WidgetMut<'_, Self>, tile: &DockTile) {
        let new = Self::new(tile);
        if this.widget.name != new.name
            || this.widget.kind != new.kind
            || this.widget.running != new.running
            || this.widget.launching != new.launching
            || this.widget.attention != new.attention
        {
            *this.widget = new;
            this.ctx.request_accessibility_update();
        }
    }
}

impl UsesProperty<ThemeProp> for DockTileWidget {}

impl Widget for DockTileWidget {
    /// The item chosen from the tile's menu, which it owns.
    type Action = Chosen;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        // The focus ring shows only in the active window.
        if matches!(event, TextEvent::WindowFocusChange(_)) {
            ctx.request_paint_only();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FocusChanged(_)) {
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
        let _ = axis;
        Length::px(neue_theme::materials::dock::ICON)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if !(ctx.is_focus_target() && ctx.is_window_focused()) {
            return;
        }
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        let outline = RoundedRect::from_rect(size.to_rect(), size.width.min(size.height) * 0.2);
        focus_ring(&mut scene, outline, theme.variant);
        ImagingReplayer::new().replay(&scene, ctx.scale_factor(), painter);
    }

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        self.kind != DockTileKind::Divider
    }

    fn accessibility_role(&self) -> Role {
        match self.kind {
            DockTileKind::Divider => Role::GenericContainer,
            _ => Role::Button,
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if self.kind == DockTileKind::Divider {
            return;
        }
        node.set_label(self.name.clone());
        node.add_action(Action::Click);
        node.add_action(Action::ShowContextMenu);
        let state = match (self.launching, self.attention, self.running) {
            (true, _, _) => Some("Opening"),
            (_, true, _) => Some("Wants attention"),
            (_, _, true) => Some("Running"),
            _ => None,
        };
        if let Some(state) = state {
            node.set_description(state);
        }
        if self.launching {
            node.set_busy();
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: Motion = Motion { reduced: false };
    const REDUCED: Motion = Motion { reduced: true };
    const FRAME: Duration = Duration::from_millis(8);

    /// Steps until the bounce has landed `landings` times; returns the time it took and
    /// the highest lift seen.
    fn run(bounce: &mut Bounce, landings: usize) -> (Duration, f64) {
        let (mut time, mut top, mut landed, mut was_up) = (Duration::ZERO, 0.0_f64, 0, false);
        while landed < landings {
            assert!(time < Duration::from_secs(10), "never landed");
            bounce.step(FRAME, FULL);
            time += FRAME;
            top = top.max(bounce.lift());
            let up = bounce.lift() > 0.0;
            if was_up && !up {
                landed += 1;
            }
            was_up = up;
        }
        (time, top)
    }

    #[test]
    fn a_bounce_rises_turns_and_lands() {
        let mut bounce = Bounce::new();
        assert!(bounce.set(DockBounce::Launch, FULL));
        let (time, top) = run(&mut bounce, 1);
        assert!(
            top > RISE_TURN * BOUNCE_HEIGHT * 0.99 && top <= BOUNCE_HEIGHT,
            "{top}"
        );
        assert!(
            (Duration::from_millis(300)..Duration::from_millis(800)).contains(&time),
            "{time:?}"
        );
        // A launch bounces again at once.
        bounce.step(FRAME, FULL);
        assert!(bounce.lift() > 0.0);
    }

    #[test]
    fn a_stopped_bounce_finishes_and_rests() {
        let mut bounce = Bounce::new();
        bounce.set(DockBounce::Launch, FULL);
        bounce.step(Duration::from_millis(100), FULL);
        assert!(
            bounce.set(DockBounce::None, FULL),
            "the bounce under way goes on"
        );
        run(&mut bounce, 1);
        assert!(!bounce.moving());
        assert_eq!(bounce.lift(), 0.0);
    }

    #[test]
    fn attention_rests_between_bounces() {
        let mut bounce = Bounce::new();
        bounce.set(DockBounce::Attention, FULL);
        run(&mut bounce, 1);
        // Resting on the shelf for the rest, then up again.
        let mut rested = Duration::ZERO;
        while bounce.lift() == 0.0 {
            bounce.step(FRAME, FULL);
            rested += FRAME;
            assert!(rested < Duration::from_secs(2));
        }
        assert!(rested >= ATTENTION_REST, "{rested:?}");
    }

    #[test]
    fn reduced_motion_raises_attention_and_never_bounces() {
        let mut bounce = Bounce::new();
        assert!(!bounce.set(DockBounce::Launch, REDUCED));
        assert_eq!(bounce.lift(), 0.0);
        assert!(!bounce.set(DockBounce::Attention, REDUCED));
        assert_eq!(bounce.lift(), STILL_LIFT);
        // Motion back on: it bounces from there; attention over, it comes down.
        assert!(bounce.set(DockBounce::None, FULL));
        run(&mut bounce, 1);
        assert!(!bounce.moving());
    }
}
