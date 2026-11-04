//! The tab view: a strip of tabs over a bordered box, with one tab's content shown inside.
//! A click selects a tab; Left and Right move the selection while the strip has keyboard
//! focus, and Ctrl-Tab and Ctrl-Shift-Tab cycle through the tabs from anywhere inside the
//! view. Every tab's content is built up front; only the selected one is laid out and shown.

use std::any::TypeId;

use masonry_core::accesskit::{Action, Node, NodeId, Orientation, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, BrushIndex, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Point, Rect, Size, Vec2};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length};
use masonry_core::parley::{FontContext, LayoutContext};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{TabStrip, tab_box, tab_strip};
use neue_theme::materials::segmented::SELECTED_LABEL;
use neue_theme::materials::tabs::{DISABLED_OPACITY, PADDING, PADDING_TOP};
use neue_theme::metrics::control;

use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::element::{Element, Handle, Wiring};
use crate::theme::ThemeProp;

/// What a tab view emits when the user selects a tab: its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabSelected(pub(crate) usize);

/// The strip's height.
const STRIP_HEIGHT: f64 = control::REGULAR_HEIGHT;

/// What one tab's content sits in: it fills the box's content area, and is the tab panel
/// for assistive technology.
#[derive(Debug)]
pub(crate) struct TabPanelWidget {
    child: WidgetPod<dyn Widget>,
    title: String,
}

impl Widget for TabPanelWidget {
    type Action = ();

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context = LayoutSize::maybe(axis.cross(), cross_length);
        ctx.compute_length(&mut self.child, len_req.into(), context, axis, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::TabPanel
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.title.clone());
    }

    fn children_ids(&self) -> ChildrenIds {
        std::iter::once(self.child.id()).collect()
    }
}

/// The Masonry widget behind [`crate::TabView`].
#[derive(Debug)]
pub(crate) struct TabViewWidget {
    lines: Vec<TextLine>,
    enabled: Vec<bool>,
    panels: Vec<WidgetPod<TabPanelWidget>>,
    selected: usize,
    /// The tab under the pointer while it is held down.
    pressed: Option<usize>,
    /// Tab widths and the strip's rectangle, from the last layout.
    widths: Vec<f64>,
    strip: Rect,
    /// Accessibility ids of the tab list and its tabs (not widgets).
    list_id: Option<NodeId>,
    tab_ids: Vec<NodeId>,
    fixed: (Option<f64>, Option<f64>),
    replayer: ImagingReplayer,
}

impl TabViewWidget {
    pub(crate) fn new() -> Self {
        Self {
            lines: Vec::new(),
            enabled: Vec::new(),
            panels: Vec::new(),
            selected: 0,
            pressed: None,
            widths: Vec::new(),
            strip: Rect::ZERO,
            list_id: None,
            tab_ids: Vec::new(),
            fixed: (None, None),
            replayer: ImagingReplayer::new(),
        }
    }

    pub(crate) fn push(&mut self, title: String, enabled: bool, content: NewWidget<dyn Widget>) {
        let panel = TabPanelWidget {
            child: content.to_pod(),
            title: title.clone(),
        };
        self.lines.push(TextLine::new(title, TextStyle::Body));
        self.enabled.push(enabled);
        self.panels.push(NewWidget::new(panel).to_pod());
    }

    pub(crate) fn set_selection(&mut self, selected: usize) {
        self.selected = selected.min(self.lines.len().saturating_sub(1));
    }

    pub(crate) fn set_fixed(&mut self, width: Option<f64>, height: Option<f64>) {
        self.fixed = (width.or(self.fixed.0), height.or(self.fixed.1));
    }

    fn tab_width(
        line: &mut TextLine,
        text: (&mut FontContext, &mut LayoutContext<BrushIndex>),
    ) -> f64 {
        (line.size(text).width + 2.0 * control::SEGMENT_PADDING)
            .max(control::SEGMENT_MIN_WIDTH)
            .ceil()
    }

    /// The tab at local `pos`, if it is on the strip.
    fn tab_at(&self, pos: Point) -> Option<usize> {
        if !self.strip.contains(pos) {
            return None;
        }
        let mut x = self.strip.x0;
        self.widths.iter().position(|w| {
            let hit = pos.x >= x && pos.x < x + w;
            x += w;
            hit
        })
    }

    /// The enabled tab after (`forward`) or before the selected one, if any; with `wrap`,
    /// going on past either end.
    fn neighbour(&self, forward: bool, wrap: bool) -> Option<usize> {
        let n = self.lines.len();
        let order: Vec<usize> = if wrap {
            (1..n)
                .map(|step| {
                    if forward {
                        (self.selected + step) % n
                    } else {
                        (self.selected + n - step) % n
                    }
                })
                .collect()
        } else if forward {
            (self.selected + 1..n).collect()
        } else {
            (0..self.selected).rev().collect()
        };
        order.into_iter().find(|&i| self.enabled[i])
    }

    /// The user selected tab `i`: show it, take the keyboard focus, and tell the app.
    fn select(&mut self, ctx: &mut EventCtx<'_>, i: usize) {
        if i == self.selected || i >= self.lines.len() || !self.enabled[i] {
            return;
        }
        self.selected = i;
        // The focus may have been inside the tab that is now hidden.
        ctx.request_focus();
        ctx.submit_action::<TabSelected>(TabSelected(i));
        ctx.request_layout();
        ctx.request_accessibility_update();
    }

    /// Sets the selection from the app, enabled or not: no message goes back.
    pub(crate) fn edit_selected(this: &mut WidgetMut<'_, Self>, selected: usize) {
        this.widget.set_selection(selected);
        this.ctx.request_layout();
        this.ctx.request_accessibility_update();
    }

    pub(crate) fn edit_enabled(this: &mut WidgetMut<'_, Self>, tab: usize, enabled: bool) {
        if let Some(slot) = this.widget.enabled.get_mut(tab) {
            *slot = enabled;
            this.ctx.request_paint_only();
            this.ctx.request_accessibility_update();
        }
    }

    /// The selected index and which tabs are enabled, for tests.
    #[cfg(test)]
    pub(crate) fn state(&self) -> (usize, Vec<bool>) {
        (self.selected, self.enabled.clone())
    }

    /// The tabs' rectangles in the view's coordinates, for tests.
    #[cfg(test)]
    pub(crate) fn tab_rects(&self) -> Vec<Rect> {
        let mut x = self.strip.x0;
        self.widths
            .iter()
            .map(|w| {
                let r = Rect::new(x, self.strip.y0, x + w, self.strip.y1);
                x += w;
                r
            })
            .collect()
    }

    /// Which panels are shown (not stashed), for tests.
    #[cfg(test)]
    pub(crate) fn panel_ids(&self) -> Vec<masonry_core::core::WidgetId> {
        self.panels.iter().map(WidgetPod::id).collect()
    }
}

impl UsesProperty<ThemeProp> for TabViewWidget {}

impl Widget for TabViewWidget {
    type Action = TabSelected;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let pos = ctx.local_position(state.position);
                if let Some(i) = self.tab_at(pos) {
                    self.pressed = self.enabled[i].then_some(i);
                    ctx.request_focus();
                    ctx.capture_pointer();
                    ctx.set_handled();
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let pos = ctx.local_position(state.position);
                if let Some(pressed) = self.pressed.take() {
                    if ctx.is_active() && self.tab_at(pos) == Some(pressed) {
                        self.select(ctx, pressed);
                    }
                    ctx.request_paint_only();
                }
            }
            PointerEvent::Cancel(..) => {
                self.pressed = None;
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
        match event {
            TextEvent::Keyboard(key) if key.state.is_down() => {
                let Key::Named(named) = key.key else {
                    return;
                };
                let target = if named == NamedKey::Tab && key.modifiers.ctrl() {
                    self.neighbour(!key.modifiers.shift(), true)
                } else if ctx.is_focus_target() {
                    match named {
                        NamedKey::ArrowLeft => self.neighbour(false, false),
                        NamedKey::ArrowRight => self.neighbour(true, false),
                        NamedKey::Home => (0..self.lines.len()).find(|&i| self.enabled[i]),
                        NamedKey::End => (0..self.lines.len()).rev().find(|&i| self.enabled[i]),
                        _ => return,
                    }
                } else {
                    return;
                };
                if let Some(i) = target {
                    self.select(ctx, i);
                }
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
        let target = match event.action {
            Action::Increment => self.neighbour(true, false),
            Action::Decrement => self.neighbour(false, false),
            _ => None,
        };
        if let Some(i) = target {
            self.select(ctx, i);
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for panel in &mut self.panels {
            ctx.register_child(panel);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::DisabledChanged(_) | Update::HoveredChanged(_) | Update::ActiveChanged(_) => {
                ctx.request_paint_only();
            }
            Update::FocusChanged(_) => ctx.request_paint_only(),
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
            ctx.request_render();
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
        let fixed = match axis {
            Axis::Horizontal => self.fixed.0,
            Axis::Vertical => self.fixed.1,
        };
        if let Some(length) = fixed {
            return Length::px(length);
        }
        // What the box's border and the strip take from the content's room.
        let (used, cross_used) = match axis {
            Axis::Horizontal => (2.0 * PADDING, STRIP_HEIGHT + PADDING_TOP + PADDING),
            Axis::Vertical => (STRIP_HEIGHT + PADDING_TOP + PADDING, 2.0 * PADDING),
        };
        let used = Length::px(used);
        let inner_cross = cross_length.map(|c| c.saturating_sub(Length::px(cross_used)));
        let auto: LenDef = len_req.reduce(used).into();
        let context = LayoutSize::maybe(axis.cross(), inner_cross);
        let mut length = Length::ZERO;
        for panel in &mut self.panels {
            length = length.max(ctx.compute_length(panel, auto, context, axis, inner_cross));
        }
        let mut length = length.saturating_add(used).get();
        if axis == Axis::Horizontal {
            let mut strip = 0.0;
            for line in &mut self.lines {
                strip += Self::tab_width(line, ctx.text_contexts());
            }
            length = length.max(strip);
        }
        Length::px(length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.widths = self
            .lines
            .iter_mut()
            .map(|line| Self::tab_width(line, ctx.text_contexts()))
            .collect();
        // The tabs are centred; if they are wider than the view, they spill out evenly.
        let total: f64 = self.widths.iter().sum();
        let x = ((size.width - total) / 2.0).round();
        self.strip = Rect::new(x, 0.0, x + total, STRIP_HEIGHT);

        let content = Rect::new(
            PADDING,
            STRIP_HEIGHT + PADDING_TOP,
            (size.width - PADDING).max(PADDING),
            (size.height - PADDING).max(STRIP_HEIGHT + PADDING_TOP),
        );
        for (i, panel) in self.panels.iter_mut().enumerate() {
            ctx.set_stashed(panel, i != self.selected);
            if i == self.selected {
                ctx.run_layout(panel, content.size());
                ctx.place_child(panel, content.origin());
            }
        }

        let text_height = self
            .lines
            .first_mut()
            .map_or(0.0, |l| l.size(ctx.text_contexts()).height);
        let baseline =
            (STRIP_HEIGHT - text_height) / 2.0 + self.lines.first().map_or(0.0, TextLine::baseline);
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
        let disabled = ctx.is_disabled();
        let active = ctx.is_window_focused();
        let mut scene = Scene::new(size.width, size.height);
        if disabled {
            scene.push_opacity(DISABLED_OPACITY);
        }
        // The box's top edge runs behind the middle of the strip.
        let box_rect = Rect::new(
            0.0,
            STRIP_HEIGHT / 2.0,
            size.width,
            (size.height - 1.0).max(STRIP_HEIGHT),
        );
        tab_box(&mut scene, box_rect, theme.variant);
        let strip = TabStrip {
            widths: &self.widths,
            selected: self.selected,
            pressed: self.pressed.filter(|_| ctx.is_active()),
            active,
            focused: ctx.is_focus_target() && active && !disabled,
        };
        tab_strip(&mut scene, self.strip, strip, theme.variant);
        if disabled {
            scene.pop_layer();
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        // Labels: white on the accent gel of the selected tab.
        let colors = theme.colors();
        let mut x = self.strip.x0;
        for (i, line) in self.lines.iter_mut().enumerate() {
            let text = line.size(ctx.text_contexts());
            let width = self.widths[i];
            let selected = i == self.selected;
            let color = if disabled || !self.enabled[i] {
                colors.disabled_label
            } else if selected && active {
                SELECTED_LABEL
            } else {
                colors.label
            };
            let origin = Vec2::new(
                (x + (width - text.width) / 2.0).round(),
                ((STRIP_HEIGHT - text.height) / 2.0).round(),
            );
            line.paint(painter, Affine::translate(origin), color);
            x += width;
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let n = self.lines.len();
        let list_id = *self.list_id.get_or_insert_with(AccessCtx::next_node_id);
        while self.tab_ids.len() < n {
            self.tab_ids.push(AccessCtx::next_node_id());
        }

        let mut list = Node::new(Role::TabList);
        list.set_orientation(Orientation::Horizontal);
        list.set_bounds(to_rect(self.strip));
        let mut x = self.strip.x0;
        for i in 0..n {
            let id = self.tab_ids[i];
            let mut tab = Node::new(Role::Tab);
            tab.set_label(self.lines[i].text().to_owned());
            tab.set_bounds(to_rect(Rect::new(
                x,
                self.strip.y0,
                x + self.widths.get(i).copied().unwrap_or(0.0),
                self.strip.y1,
            )));
            tab.set_selected(i == self.selected);
            tab.set_size_of_set(n);
            tab.set_position_in_set(i + 1);
            if !self.enabled[i] {
                tab.set_disabled();
            }
            if i == self.selected {
                tab.push_controlled(self.panels[i].id().into());
            }
            x += self.widths.get(i).copied().unwrap_or(0.0);
            list.push_child(id);
            ctx.tree_update().nodes.push((id, tab));
        }
        ctx.tree_update().nodes.push((list_id, list));

        // The tab list comes before the panel in reading order.
        let mut children = vec![list_id];
        children.extend_from_slice(node.children());
        node.set_children(children);
        if let Some(line) = self.lines.get(self.selected) {
            node.set_value(line.text());
        }
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
    }

    fn children_ids(&self) -> ChildrenIds {
        self.panels.iter().map(WidgetPod::id).collect()
    }

    fn accepts_focus(&self) -> bool {
        !self.lines.is_empty()
    }

    fn get_debug_text(&self) -> Option<String> {
        self.lines.get(self.selected).map(|l| l.text().to_owned())
    }
}

fn to_rect(r: Rect) -> masonry_core::accesskit::Rect {
    masonry_core::accesskit::Rect {
        x0: r.x0,
        y0: r.y0,
        x1: r.x1,
        y1: r.y1,
    }
}

/// Turns the selected tab's index into a message.
type SelectHandler<M> = Box<dyn FnMut(usize) -> M>;

/// A strip of tabs over a bordered box showing the selected tab's content. Every tab's
/// content is built up front and only the selected one is shown. A click selects a tab;
/// while the strip has keyboard focus (Tab moves focus to it), Left and Right move the
/// selection and Home and End go to the first and last tab; Ctrl-Tab and Ctrl-Shift-Tab
/// cycle through the tabs from anywhere inside. A disabled tab can't be selected by the
/// user. Sends the new tab's index.
pub struct TabView<M> {
    inner: NewWidget<TabViewWidget>,
    tabs: Vec<(String, bool, Element<M>)>,
    selected: usize,
    on_select: Option<SelectHandler<M>>,
}

impl<M> std::fmt::Debug for TabView<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabView")
            .field("inner", &self.inner)
            .field("tabs", &self.tabs.len())
            .finish()
    }
}

impl<M: 'static> TabView<M> {
    /// A tab view without tabs; add them with [`with_tab`](Self::with_tab).
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(TabViewWidget::new()),
            tabs: Vec::new(),
            selected: 0,
            on_select: None,
        }
    }

    /// Adds a tab titled `title` showing `content`.
    #[must_use]
    pub fn with_tab(mut self, title: impl Into<String>, content: impl Into<Element<M>>) -> Self {
        self.tabs.push((title.into(), true, content.into()));
        self
    }

    /// Adds a tab the user can't select (until [`Handle::set_tab_enabled`] enables it).
    #[must_use]
    pub fn with_disabled_tab(
        mut self,
        title: impl Into<String>,
        content: impl Into<Element<M>>,
    ) -> Self {
        self.tabs.push((title.into(), false, content.into()));
        self
    }

    /// Starts with tab `index` selected (the last one if it is out of range).
    #[must_use]
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    /// Sends the message `make` returns, given the index of the tab the user selected.
    /// Changes through a [`Handle`] send nothing.
    #[must_use]
    pub fn on_select(mut self, make: impl FnMut(usize) -> M + 'static) -> Self {
        self.on_select = Some(Box::new(make));
        self
    }

    /// Fixes the width, in logical pixels.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.inner.widget.set_fixed(Some(width), None);
        self
    }

    /// Fixes the height, in logical pixels.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.inner.widget.set_fixed(None, Some(height));
        self
    }

    /// Starts disabled: greyed out and ignoring input, contents included.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this tab view.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> Default for TabView<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: 'static> From<TabView<M>> for Element<M> {
    fn from(view: TabView<M>) -> Self {
        let TabView {
            mut inner,
            tabs,
            selected,
            on_select,
        } = view;
        let id = inner.id();
        let mut wiring = Wiring::default();
        for (title, enabled, content) in tabs {
            inner.widget.push(title, enabled, wiring.adopt(content));
        }
        inner.widget.set_selection(selected);
        let mut element = Element::with_wiring(inner.erased(), wiring);
        if let Some(mut make) = on_select {
            element.on_action(id, move |action| {
                let TabSelected(i) = *action.downcast_ref::<TabSelected>()?;
                Some(make(i))
            });
        }
        element
    }
}

impl<M> Handle<TabView<M>> {
    /// Selects tab `index` (the last one if it is out of range), enabled or not.
    pub fn set_selected<N>(&self, cx: &mut Cx<'_, N>, index: usize) {
        cx.edit::<TabViewWidget, _>(self, |mut w| TabViewWidget::edit_selected(&mut w, index));
    }

    /// Enables or disables tab `index`: the user can't select a disabled tab. Does nothing
    /// for an index out of range.
    pub fn set_tab_enabled<N>(&self, cx: &mut Cx<'_, N>, index: usize, enabled: bool) {
        cx.edit::<TabViewWidget, _>(self, |mut w| {
            TabViewWidget::edit_enabled(&mut w, index, enabled);
        });
    }

    /// Enables or disables the whole tab view, contents included.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<TabViewWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
