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

use std::any::TypeId;
use std::time::Duration;

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget,
    WidgetId, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, RoundedRect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::icon::{Icon, icon};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::dock_shelf::{
    ShelfStyle, dock_shelf_with, placeholder_icon, reflected, running_indicator,
};
use neue_paint::materials::focus_ring;
use neue_paint::{Backdrop, Scene};
use neue_theme::materials::dock::{
    ATTENTION_REST, BOUNCE_HEIGHT, FALL, MAGNIFY, RISE, RISE_TURN, STILL_LIFT,
};
use neue_theme::materials::dock_shelf::INDICATOR;
use neue_theme::motion::SpringValue;
use neue_theme::{Motion, SemanticColors, Theme};

use super::dock_layout::{DockEdge, DockLayout, DockPlacement};
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
    running: bool,
    bounce: Bounce,
    pod: WidgetPod<DockTileWidget>,
}

/// The Masonry widget behind [`crate::desktop::Dock`].
#[derive(Debug)]
pub(crate) struct DockWidget {
    tiles: Vec<Tile>,
    pub(super) backdrop: Backdrop,
    pub(super) layout: DockLayout,
    magnify: Magnify,
    /// Where the tiles are now, from the last layout pass.
    placement: DockPlacement,
    /// The icons' rects last reported.
    placed: Vec<(String, Rect)>,
    /// The tile the pointer went down on.
    pressed: Option<usize>,
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
            placement: DockPlacement::default(),
            placed: Vec::new(),
            pressed: None,
            replayer: ImagingReplayer::new(),
        };
        for tile in tiles {
            let mut bounce = Bounce::new();
            // Moving once added, with the theme known.
            bounce.kind = tile.bounce;
            dock.tiles.push(Tile {
                pod: WidgetPod::new(DockTileWidget::new(&tile)),
                key: tile.key,
                icon: tile.icon,
                running: tile.running,
                bounce,
            });
        }
        dock
    }

    /// Shows `tiles` in this order. Tiles whose key was there before keep their bounce
    /// (and keyboard focus); others come and go.
    pub(crate) fn edit_tiles(this: &mut WidgetMut<'_, Self>, tiles: Vec<DockTile>) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        let motion = theme.motion();
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
                    Tile {
                        key: tile.key.clone(),
                        icon: None,
                        running: false,
                        bounce: Bounce::new(),
                        pod: WidgetPod::new(DockTileWidget::new(&tile)),
                    }
                }
            };
            kept.icon = tile.icon;
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
        this.widget.pressed = None;
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
        self.placement.tile_at(u)
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
        match event {
            PointerEvent::Move(update) => {
                let local = ctx.local_position(update.current.position);
                let hover = self.hover(Some(local), theme.motion());
                if hover.frames {
                    ctx.request_anim_frame();
                }
                if hover.layout {
                    ctx.request_layout();
                }
            }
            PointerEvent::Down(down) => {
                let local = ctx.local_position(down.state.position);
                self.pressed = self.tile_at(local);
                if self.pressed.is_some() {
                    ctx.capture_pointer();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Up(up) => {
                let local = ctx.local_position(up.state.position);
                if let Some(index) = self.pressed.take() {
                    let inside = ctx.content_box().contains(local);
                    if inside && self.tile_at(local) == Some(index) {
                        self.click(ctx, index);
                    }
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Cancel(..) if self.pressed.take().is_some() => {
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
        if let TextEvent::Keyboard(key) = event
            && is_press_key(&key.key)
            && let Some(index) = self.tile_of(ctx.target())
        {
            if !key.state.is_down() {
                self.click(ctx, index);
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
        if event.action == Action::Click
            && let Some(index) = self.tile_of(ctx.target())
        {
            self.click(ctx, index);
            ctx.set_handled();
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
        let size = self.layout.size(self.tiles.len());
        Length::px(match axis {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
        self.placement = self.layout.place(
            self.tiles.len(),
            self.magnify.pointer,
            self.magnify.amount.value(),
        );
        for (tile, icon) in self.tiles.iter_mut().zip(&self.placement.icons) {
            ctx.run_layout(&mut tile.pod, icon.size());
            ctx.place_child(&mut tile.pod, icon.origin());
        }
        let placed: Vec<(String, Rect)> = self
            .tiles
            .iter()
            .zip(&self.placement.icons)
            .map(|(tile, icon)| (tile.key.clone(), *icon))
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
        let icons = |scene: &mut Scene| {
            for ((i, tile), rest) in self.tiles.iter().enumerate().zip(&placement.icons) {
                let rect = *rest + outwards * tile.bounce.lift();
                let dimmed = self.pressed == Some(i);
                if dimmed {
                    scene.push_opacity(0.6);
                }
                draw_icon(scene, rect, tile.icon.as_ref(), &theme);
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
            for (tile, span) in self.tiles.iter().zip(&placement.spans) {
                if tile.running {
                    let u = (span.0 + span.1) / 2.0;
                    running_indicator(
                        scene,
                        Point::new(u, layout.frame_depth() - INDICATOR.1 - 1.0),
                        theme.variant,
                    );
                }
            }
        });
        icons(&mut scene);
        scene.pop_layer();
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
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

/// A tile's keyboard focus and its node for assistive technology; the dock paints it,
/// except for the focus ring.
#[derive(Debug)]
pub(crate) struct DockTileWidget {
    name: String,
    running: bool,
    launching: bool,
    attention: bool,
}

impl DockTileWidget {
    fn new(tile: &DockTile) -> Self {
        Self {
            name: tile.name.clone(),
            running: tile.running,
            launching: tile.bounce == DockBounce::Launch,
            attention: tile.bounce == DockBounce::Attention,
        }
    }

    fn edit(this: &mut WidgetMut<'_, Self>, tile: &DockTile) {
        let new = Self::new(tile);
        if this.widget.name != new.name
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
    type Action = masonry_core::core::NoAction;

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
        true
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
        node.set_label(self.name.clone());
        node.add_action(Action::Click);
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
