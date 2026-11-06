//! The container that hosts notification panels (task 3.15d2):
//! [`crate::desktop::NotificationHost`] owns one [`NotificationWidget`] per notification
//! and places them with a [`NotificationStack`] stepped on animation frames. Its size is
//! the stack's [`extent`](NotificationStack::extent) (plus the panels' margin), so a layer
//! surface that follows its content (`LayerSpec::fit_content`, ADR 0090 addendum) is no
//! bigger than the stack.
//!
//! The panels' margins (room for the shadow and close button) overlap in the gaps between
//! them, so a point in a gap goes to the panel whose half of the gap it is in rather than
//! to the one drawn last.

use std::any::{Any, TypeId};
use std::time::Duration;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
    PropertiesRef, QueryCtx, RegisterCtx, Update, UpdateCtx, UsesProperty, Widget, WidgetMut,
    WidgetPod, WidgetRef,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Rect, Size};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length};
use neue_paint::Backdrop;
use neue_theme::Motion;
use neue_theme::materials::notification::{MARGIN, SPACING, WIDTH};

use super::notification::{NotificationContent, NotificationWidget};
use crate::notification_stack::NotificationStack;
use crate::theme::ThemeProp;

/// The host's width: a panel and its margin on both sides.
const HOST_WIDTH: f64 = WIDTH + 2.0 * MARGIN;

/// What the host emits when a panel has finished sliding out and left: the
/// notification's id. The kit forgets the panel's message handlers first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PanelGone(pub(crate) u32);

/// One hosted notification.
#[derive(Debug)]
struct Panel {
    id: u32,
    pod: WidgetPod<NotificationWidget>,
    /// The panel's height (without its margin) last given to the stack.
    height: Option<f64>,
    /// Its top in the host, as last laid out.
    y: f64,
}

/// The Masonry widget behind [`crate::desktop::NotificationHost`].
#[derive(Debug)]
pub(crate) struct NotificationHostWidget {
    panels: Vec<Panel>,
    stack: NotificationStack,
    pub(super) backdrop: Backdrop,
    /// The app's event closure (`HostEvents<M>` in `widgets`), type-erased so the widget
    /// is not generic; panels shown later are wired to it.
    pub(super) events: Box<dyn Any>,
    /// Whether the host has been laid out: until then, panels take their heights without
    /// moving (the ones it was built with appear at rest).
    laid_out: bool,
}

/// Motion that jumps.
const JUMP: Motion = Motion { reduced: true };

impl NotificationHostWidget {
    pub(crate) fn new(events: Box<dyn Any>) -> Self {
        Self {
            panels: Vec::new(),
            stack: NotificationStack::new(),
            backdrop: Backdrop::Opaque,
            events,
            laid_out: false,
        }
    }

    /// Adds a panel at rest, on top of the ones added before (the builder's `with`).
    pub(crate) fn push(&mut self, id: u32, panel: NewWidget<NotificationWidget>) {
        if self.panels.iter().any(|p| p.id == id) {
            tracing::warn!(id, "a notification host was given the same id twice");
            return;
        }
        self.stack.push(id, 0.0, JUMP);
        self.panels.push(Panel {
            id,
            pod: panel.to_pod(),
            height: None,
            y: 0.0,
        });
    }

    /// Shows `content` as notification `id`: in place if it is there (and brings it back
    /// if it was sliding out), otherwise in a new panel `build` makes from the app's
    /// event closure, sliding in on top. Returns what `build` returned with the panel.
    pub(crate) fn edit_show<R>(
        this: &mut WidgetMut<'_, Self>,
        id: u32,
        content: NotificationContent,
        build: impl FnOnce(&dyn Any, NotificationContent) -> (NewWidget<NotificationWidget>, R),
    ) -> Option<R> {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        let motion = theme.motion();
        let built = if let Some(i) = this.widget.panels.iter().position(|p| p.id == id) {
            let height = this.widget.panels[i].height.unwrap_or(0.0);
            // Sliding back if it was leaving; a new height is picked up at layout.
            this.widget.stack.push(id, height, motion);
            let mut panel = this.ctx.get_mut(&mut this.widget.panels[i].pod);
            panel.ctx.set_disabled(false);
            NotificationWidget::edit_content(&mut panel, content);
            None
        } else {
            let (mut panel, built) = build(this.widget.events.as_ref(), content);
            panel.widget.backdrop = this.widget.backdrop;
            // Its height is measured at layout; until then it takes no room.
            this.widget.stack.push(id, 0.0, motion);
            this.widget.panels.push(Panel {
                id,
                pod: panel.to_pod(),
                height: None,
                y: 0.0,
            });
            this.ctx.children_changed();
            Some(built)
        };
        Self::moved(this);
        built
    }

    /// Slides notification `id` out; once gone it leaves the host ([`PanelGone`]). Its
    /// buttons stop taking input at once.
    pub(crate) fn edit_dismiss(this: &mut WidgetMut<'_, Self>, id: u32) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        let Some(i) = this.widget.panels.iter().position(|p| p.id == id) else {
            return;
        };
        this.widget.stack.remove(id, theme.motion());
        this.ctx
            .get_mut(&mut this.widget.panels[i].pod)
            .ctx
            .set_disabled(true);
        if !this.widget.stack.contains(id) {
            // Reduced motion: gone at once.
            let panel = this.widget.panels.remove(i);
            this.ctx.remove_child(panel.pod);
            this.ctx.submit_action::<PanelGone>(PanelGone(id));
        }
        Self::moved(this);
    }

    /// Whether notification `id` is shown (also while it slides out).
    #[cfg(test)]
    pub(crate) fn contains(&self, id: u32) -> bool {
        self.stack.contains(id)
    }

    /// The notifications' ids and their panels' widget ids, top to bottom.
    #[cfg(test)]
    pub(crate) fn panel_ids(&self) -> Vec<(u32, masonry_core::core::WidgetId)> {
        self.ordered().map(|p| (p.id, p.pod.id())).collect()
    }

    /// Lays out and repaints after the stack changed, with frames while it moves.
    fn moved(this: &mut WidgetMut<'_, Self>) {
        if this.widget.stack.is_animating() {
            this.ctx.request_anim_frame();
        }
        this.ctx.request_layout();
    }

    /// Gives the stack the panels' heights, measured by `height_of` (with their margin).
    /// Returns whether that set something moving.
    fn sync_heights(
        &mut self,
        motion: Motion,
        mut height_of: impl FnMut(&mut WidgetPod<NotificationWidget>) -> f64,
    ) -> bool {
        let motion = if self.laid_out { motion } else { JUMP };
        let mut changed = false;
        for panel in &mut self.panels {
            let height = (height_of(&mut panel.pod) - 2.0 * MARGIN).max(0.0);
            if panel.height != Some(height) {
                panel.height = Some(height);
                self.stack.set_height(panel.id, height, motion);
                changed = true;
            }
        }
        changed && self.stack.is_animating()
    }

    /// The host's height for the stack's extent: nothing when empty, else the panels'
    /// margins too. At least one pixel, so a layer surface sized by it is never empty.
    fn height(&self) -> f64 {
        if self.stack.is_empty() {
            1.0
        } else {
            self.stack.extent() + 2.0 * MARGIN
        }
    }

    /// The panels top to bottom, as the stack has them.
    fn ordered(&self) -> impl Iterator<Item = &Panel> + '_ {
        self.stack
            .placements()
            .filter_map(|p| self.panels.iter().find(|panel| panel.id == p.id))
    }

    /// The panel whose slot `y` (host coordinates) is in: its panel and half the gap on
    /// either side.
    fn slot_at(&self, y: f64) -> Option<&Panel> {
        self.ordered().find(|panel| {
            let height = panel.height.unwrap_or(0.0);
            let top = panel.y + MARGIN - SPACING / 2.0;
            (top..top + height + SPACING).contains(&y)
        })
    }
}

impl UsesProperty<ThemeProp> for NotificationHostWidget {}

impl Widget for NotificationHostWidget {
    type Action = PanelGone;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for panel in &mut self.panels {
            ctx.register_child(&mut panel.pod);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) && self.stack.is_animating() {
            ctx.request_anim_frame();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        // Reduced motion switched on: the next frame finishes every move.
        if property_type == TypeId::of::<ThemeProp>() && self.stack.is_animating() {
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
        let gone = self
            .stack
            .step(Duration::from_nanos(interval), theme.motion());
        for id in gone {
            if let Some(i) = self.panels.iter().position(|p| p.id == id) {
                let panel = self.panels.remove(i);
                ctx.remove_child(panel.pod);
            }
            ctx.submit_action::<PanelGone>(PanelGone(id));
        }
        if self.stack.is_animating() {
            ctx.request_anim_frame();
        }
        ctx.request_layout();
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        if axis == Axis::Horizontal {
            return Length::px(if self.stack.is_empty() {
                1.0
            } else {
                HOST_WIDTH
            });
        }
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let moving = self.sync_heights(theme.motion(), |pod| {
            ctx.compute_length(
                pod,
                LenDef::MaxContent,
                LayoutSize::NONE,
                Axis::Vertical,
                Some(Length::px(HOST_WIDTH)),
            )
            .get()
        });
        if moving {
            ctx.mutate_self_later(|mut this| this.ctx.request_anim_frame());
        }
        Length::px(self.height())
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, props: &PropertiesRef<'_>, size: Size) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let moving = self.sync_heights(theme.motion(), |pod| {
            ctx.compute_length(
                pod,
                LenDef::MaxContent,
                LayoutSize::NONE,
                Axis::Vertical,
                Some(Length::px(HOST_WIDTH)),
            )
            .get()
        });
        if moving {
            ctx.mutate_self_later(|mut this| this.ctx.request_anim_frame());
        }
        let placements: Vec<_> = self.stack.placements().collect();
        for placement in placements {
            let Some(panel) = self.panels.iter_mut().find(|p| p.id == placement.id) else {
                continue;
            };
            let height = panel.height.unwrap_or(0.0) + 2.0 * MARGIN;
            ctx.run_layout(&mut panel.pod, Size::new(HOST_WIDTH, height));
            ctx.place_child(&mut panel.pod, Point::new(placement.x, placement.y));
            panel.y = placement.y;
        }
        // Panels sliding in or out are cut at the host's edge, as at the screen's.
        ctx.set_clip_path(Rect::from_origin_size(Point::ORIGIN, size));
        self.laid_out = true;
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    /// Top to bottom, so Tab goes down the stack.
    fn children_ids(&self) -> ChildrenIds {
        self.ordered().map(|panel| panel.pod.id()).collect()
    }

    fn find_widget_under_pointer<'c>(
        &'c self,
        ctx: QueryCtx<'c>,
        pos: Point,
    ) -> Option<WidgetRef<'c, dyn Widget>> {
        if ctx.is_stashed() || !ctx.bounding_box().contains(pos) {
            return None;
        }
        let local = ctx.to_local(pos);
        let panel = self.slot_at(local.y)?;
        ctx.get(panel.pod.id()).find_widget_under_pointer(pos)
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("{} notifications", self.panels.len()))
    }
}
