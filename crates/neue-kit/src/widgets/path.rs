//! The path control (task 2.24a): the standard style is a row of text segments separated
//! by chevrons, one per level of a path; clicking a segment, or pressing Return or Space on
//! the one with the keyboard focus, sends its index. When the row is too narrow the
//! segments between the first and the last collapse into an ellipsis. The popup style is a
//! [`crate::PopUpButton`] listing the same path from the deepest level up
//! ([`PathControl::popup`]).

use std::any::TypeId;

use masonry_core::accesskit::{Action, Node, NodeId, Orientation, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, BrushIndex, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Update, UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use masonry_core::parley::{FontContext, LayoutContext};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{PathControl as PathSpec, path_control, path_segment_rects};
use neue_theme::materials::path_control::{CHEVRON_SLOT, SEGMENT_INSET, SEGMENT_PADDING};
use neue_theme::metrics::control;

use super::PopUpButton;
use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// What a path control emits when the user chooses a segment: its index in the path (0 is
/// the root).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selected(pub(crate) usize);

/// The Masonry widget behind [`crate::PathControl`].
#[derive(Debug)]
pub(crate) struct PathWidget {
    lines: Vec<TextLine>,
    ellipsis: TextLine,
    replayer: ImagingReplayer,
    /// The segments drawn, left to right: a path index, or `None` for the ellipsis.
    visible: Vec<Option<usize>>,
    /// Where each drawn segment sits, parallel to `visible`.
    rects: Vec<Rect>,
    /// The segment that Return and Space choose and that carries the focus ring.
    focus: usize,
    /// The control is about to take the keyboard focus because of a press, which has
    /// already chosen the segment; a keyboard entry (Tab) starts on the current level.
    pointer_focus: bool,
    /// The drawn segment under the pointer.
    hovered: Option<usize>,
    /// The drawn segment held down.
    pressed: Option<usize>,
    /// Accessibility nodes of the segments, made on demand.
    node_ids: Vec<NodeId>,
}

fn lines_of(segments: Vec<String>) -> Vec<TextLine> {
    segments
        .into_iter()
        .map(|s| TextLine::new(s, TextStyle::Body))
        .collect()
}

/// The widths of `lines` with their padding.
fn widths(
    lines: &mut [TextLine],
    (fonts, layouts): (&mut FontContext, &mut LayoutContext<BrushIndex>),
) -> Vec<f64> {
    lines
        .iter_mut()
        .map(|line| (line.size((&mut *fonts, &mut *layouts)).width + 2.0 * SEGMENT_PADDING).ceil())
        .collect()
}

/// The room `widths` take side by side, chevron slots included.
fn row_width(widths: &[f64]) -> f64 {
    let n = widths.len();
    widths.iter().sum::<f64>() + CHEVRON_SLOT * n.saturating_sub(1) as f64
}

/// Which segments to draw in `room`, given each one's width and the ellipsis's: all of
/// them if they fit, otherwise the first, an ellipsis, and as many of the last ones as fit
/// (at least the very last).
fn collapse(widths: &[f64], ellipsis: f64, room: f64) -> Vec<Option<usize>> {
    let n = widths.len();
    let all: Vec<Option<usize>> = (0..n).map(Some).collect();
    if n <= 2 || row_width(widths) <= room {
        return all;
    }
    // Hide segments 1..hidden_to; try the fewest hidden first.
    for hidden_to in 2..n {
        let mut shown: Vec<f64> = vec![widths[0], ellipsis];
        shown.extend_from_slice(&widths[hidden_to..]);
        if row_width(&shown) <= room || hidden_to == n - 1 {
            let mut v = vec![Some(0), None];
            v.extend((hidden_to..n).map(Some));
            return v;
        }
    }
    all
}

impl PathWidget {
    fn new(segments: Vec<String>) -> Self {
        let lines = lines_of(segments);
        Self {
            focus: lines.len().saturating_sub(1),
            lines,
            ellipsis: TextLine::new("…".to_owned(), TextStyle::Body),
            replayer: ImagingReplayer::new(),
            visible: Vec::new(),
            rects: Vec::new(),
            pointer_focus: false,
            hovered: None,
            pressed: None,
            node_ids: Vec::new(),
        }
    }

    /// Replaces the path from the app: the last segment gets the focus, no message goes
    /// back.
    pub(crate) fn edit_segments(this: &mut WidgetMut<'_, Self>, segments: Vec<String>) {
        this.widget.lines = lines_of(segments);
        this.widget.focus = this.widget.lines.len().saturating_sub(1);
        this.widget.hovered = None;
        this.widget.pressed = None;
        this.ctx.request_layout();
        this.ctx.request_accessibility_update();
    }

    /// The drawn segment at `pos`, if it is a real one (the ellipsis does nothing).
    fn segment_at(&self, pos: masonry_core::kurbo::Point) -> Option<usize> {
        // The slot between two segments belongs to neither, but the row is only as tall
        // as its fills plus their inset, so let the whole height count.
        let hit = |r: &Rect| pos.x >= r.x0 && pos.x < r.x1 && pos.y >= r.y0 - SEGMENT_INSET;
        let i = self.rects.iter().position(hit)?;
        self.visible[i].map(|_| i)
    }

    /// The drawn segments and where they sit (tests click on them).
    #[cfg(test)]
    pub(crate) fn drawn(&self) -> Vec<(Option<usize>, Rect)> {
        self.visible
            .iter()
            .copied()
            .zip(self.rects.iter().copied())
            .collect()
    }

    fn choose(&mut self, ctx: &mut EventCtx<'_>, path_index: usize) {
        if path_index >= self.lines.len() {
            return;
        }
        self.focus = path_index;
        ctx.submit_action::<Selected>(Selected(path_index));
        ctx.request_paint_only();
    }

    /// Moves the focus one drawn segment to the left or right, skipping the ellipsis.
    fn step_focus(&mut self, right: bool) {
        let Some(at) = self.visible.iter().position(|v| *v == Some(self.focus)) else {
            return;
        };
        let next = if right {
            self.visible[at + 1..].iter().find_map(|v| *v)
        } else {
            self.visible[..at].iter().rev().find_map(|v| *v)
        };
        if let Some(next) = next {
            self.focus = next;
        }
    }

    fn set_hover(&mut self, ctx: &mut EventCtx<'_>, hovered: Option<usize>) {
        if self.hovered != hovered {
            self.hovered = hovered;
            ctx.request_paint_only();
        }
    }
}

impl UsesProperty<ThemeProp> for PathWidget {}

impl Widget for PathWidget {
    type Action = Selected;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let at = self.segment_at(ctx.local_position(state.position));
                self.pressed = at;
                self.hovered = at;
                if let Some(Some(i)) = at.map(|v| self.visible[v]) {
                    self.focus = i;
                }
                self.pointer_focus = !ctx.is_focus_target();
                ctx.capture_pointer();
                ctx.request_focus();
                ctx.request_paint_only();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = self.segment_at(ctx.local_position(current.position));
                self.set_hover(ctx, at);
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let at = self.segment_at(ctx.local_position(state.position));
                let pressed = self.pressed.take();
                if ctx.is_active()
                    && let Some(v) = pressed.filter(|&p| at == Some(p))
                    && let Some(i) = self.visible[v]
                {
                    self.choose(ctx, i);
                }
                self.set_hover(ctx, at);
                ctx.request_paint_only();
            }
            PointerEvent::Cancel(..) | PointerEvent::Leave(..) => {
                self.pressed = None;
                self.set_hover(ctx, None);
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
                match &key.key {
                    Key::Named(NamedKey::ArrowLeft) => self.step_focus(false),
                    Key::Named(NamedKey::ArrowRight) => self.step_focus(true),
                    Key::Named(NamedKey::Enter) => self.choose(ctx, self.focus),
                    Key::Character(c) if c == " " => self.choose(ctx, self.focus),
                    _ => return,
                }
                ctx.request_paint_only();
                ctx.request_accessibility_update();
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
        // Masonry doesn't say which child node an action was for, so the group takes it:
        // assistive technology activates the focused segment.
        if event.action == Action::Click {
            self.choose(ctx, self.focus);
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::FocusChanged(focused) => {
                // Entering with the keyboard starts on the current level, whatever an
                // earlier press left behind; a press picked its segment itself.
                if *focused && !self.pointer_focus {
                    self.focus = self.lines.len().saturating_sub(1);
                    ctx.request_accessibility_update();
                }
                self.pointer_focus = false;
                ctx.request_paint_only();
            }
            Update::DisabledChanged(_) | Update::ActiveChanged(_) => ctx.request_paint_only(),
            Update::HoveredChanged(false) => {
                self.hovered = None;
                ctx.request_paint_only();
            }
            Update::FontsChanged => {
                for line in &mut self.lines {
                    line.invalidate();
                }
                self.ellipsis.invalidate();
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
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(match axis {
            Axis::Horizontal => {
                let widths = widths(&mut self.lines, ctx.text_contexts());
                match len_req {
                    // As narrow as it collapses: the first and last segments and an ellipsis.
                    LenReq::MinContent if widths.len() > 2 => {
                        let ell = widths_of_ellipsis(&mut self.ellipsis, ctx.text_contexts());
                        row_width(&[widths[0], ell, widths[widths.len() - 1]])
                    }
                    _ => row_width(&widths),
                }
            }
            Axis::Vertical => control::REGULAR_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let widths_all = widths(&mut self.lines, ctx.text_contexts());
        let ellipsis = widths_of_ellipsis(&mut self.ellipsis, ctx.text_contexts());
        self.visible = collapse(&widths_all, ellipsis, size.width);
        let drawn: Vec<f64> = self
            .visible
            .iter()
            .map(|v| v.map_or(ellipsis, |i| widths_all[i]))
            .collect();
        self.rects = path_segment_rects(Rect::from_origin_size((0.0, 0.0), size), &drawn);
        self.hovered = self.hovered.filter(|&h| h < self.rects.len());
        self.pressed = self.pressed.filter(|&p| p < self.rects.len());

        // The focused segment can be hidden by the collapse: move to the nearest drawn one.
        if !self.visible.contains(&Some(self.focus)) {
            self.focus = self
                .visible
                .iter()
                .flatten()
                .copied()
                .find(|&i| i > self.focus)
                .unwrap_or(self.lines.len().saturating_sub(1));
        }

        let text_height = self
            .lines
            .first_mut()
            .map_or(0.0, |l| l.size(ctx.text_contexts()).height);
        let baseline =
            (size.height - text_height) / 2.0 + self.lines.first().map_or(0.0, TextLine::baseline);
        ctx.set_baselines(baseline, baseline);
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
        let disabled = ctx.is_disabled();
        let held = ctx.is_active() && self.pressed.is_some();
        let focused = (ctx.is_focus_target() && ctx.is_window_focused())
            .then(|| self.visible.iter().position(|v| *v == Some(self.focus)))
            .flatten();
        let spec = PathSpec {
            segments: &self.rects,
            hovered: self.hovered.filter(|_| !disabled),
            pressed: self.pressed.filter(|_| held),
            focused,
            disabled,
        };
        let mut scene = Scene::new(size.width, size.height);
        path_control(&mut scene, spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        let colors = theme.colors();
        let colour = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        for (v, rect) in self.visible.iter().zip(&self.rects) {
            let line = match v {
                Some(i) => &mut self.lines[*i],
                None => &mut self.ellipsis,
            };
            let text = line.size(ctx.text_contexts());
            let origin = Vec2::new(
                (rect.x0 + (rect.width() - text.width) / 2.0).round(),
                ((size.height - text.height) / 2.0).round(),
            );
            line.paint(painter, Affine::translate(origin), colour);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let n = self.lines.len();
        while self.node_ids.len() < n {
            self.node_ids.push(AccessCtx::next_node_id());
        }
        node.set_orientation(Orientation::Horizontal);
        let mut children = Vec::new();
        for (v, rect) in self.visible.iter().zip(&self.rects) {
            let Some(i) = *v else { continue };
            let id = self.node_ids[i];
            let mut button = Node::new(Role::Button);
            button.set_label(self.lines[i].text().to_owned());
            button.set_bounds(masonry_core::accesskit::Rect {
                x0: rect.x0,
                y0: rect.y0,
                x1: rect.x1,
                y1: rect.y1,
            });
            if i + 1 == n {
                button.set_selected(true);
            }
            children.push(id);
            ctx.tree_update().nodes.push((id, button));
        }
        node.set_children(children);
        node.add_action(Action::Click);
        let path: Vec<&str> = self.lines.iter().map(TextLine::text).collect();
        node.set_value(path.join(" / "));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_focus(&self) -> bool {
        !self.lines.is_empty()
    }

    fn get_debug_text(&self) -> Option<String> {
        let path: Vec<&str> = self.lines.iter().map(TextLine::text).collect();
        Some(path.join("/"))
    }
}

/// The ellipsis's width with its padding.
fn widths_of_ellipsis(
    line: &mut TextLine,
    (fonts, layouts): (&mut FontContext, &mut LayoutContext<BrushIndex>),
) -> f64 {
    (line.size((fonts, layouts)).width + 2.0 * SEGMENT_PADDING).ceil()
}

/// A path control: a row of segments, one per level of a path, root first. Clicking a
/// segment sends its index (0 is the root); Left and Right move the focus between segments
/// while the control has keyboard focus, and Return or Space choose the focused one. The
/// last segment, the current level, can be chosen too. When the row is too narrow the
/// segments between the first and the last collapse into an ellipsis.
pub struct PathControl<M> {
    inner: NewWidget<PathWidget>,
    on_select: Option<Box<dyn FnMut(usize) -> M>>,
}

impl<M> std::fmt::Debug for PathControl<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathControl")
            .field("inner", &self.inner)
            .field("on_select", &self.on_select.is_some())
            .finish()
    }
}

impl<M: 'static> PathControl<M> {
    /// A path of `segments`, root first.
    pub fn new<S: Into<String>>(segments: impl IntoIterator<Item = S>) -> Self {
        let segments = segments.into_iter().map(Into::into).collect();
        Self {
            inner: NewWidget::new(PathWidget::new(segments)),
            on_select: None,
        }
    }

    /// The popup style: a pop-up button for the same path, listing the current level
    /// first and its ancestors below, root last. Sends the chosen level's index in the
    /// path, as [`PathControl::on_select`] does.
    pub fn popup<S: Into<String>>(
        segments: impl IntoIterator<Item = S>,
        mut make: impl FnMut(usize) -> M + 'static,
    ) -> PopUpButton<M> {
        let mut labels: Vec<String> = segments.into_iter().map(Into::into).collect();
        labels.reverse();
        let last = labels.len().saturating_sub(1);
        PopUpButton::new(labels).on_select(move |i| make(last - i))
    }

    /// Sends the message `make` returns, given the index of the segment the user chose.
    /// Changes through a [`Handle`] send nothing.
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

    /// Points `handle` at this control.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M: 'static> From<PathControl<M>> for Element<M> {
    fn from(control: PathControl<M>) -> Self {
        let id = control.inner.id();
        let mut element = Element::new(control.inner.erased());
        if let Some(mut make) = control.on_select {
            element.on_action(id, move |action| {
                let Selected(i) = *action.downcast_ref::<Selected>()?;
                Some(make(i))
            });
        }
        element
    }
}

impl<M> Handle<PathControl<M>> {
    /// Shows a new path of `segments`, root first.
    pub fn set_segments<N, S: Into<String>>(
        &self,
        cx: &mut Cx<'_, N>,
        segments: impl IntoIterator<Item = S>,
    ) {
        let segments: Vec<String> = segments.into_iter().map(Into::into).collect();
        cx.edit::<PathWidget, _>(self, move |mut w| {
            PathWidget::edit_segments(&mut w, segments)
        });
    }

    /// Enables or disables the control.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<PathWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_shows_when_it_fits() {
        let w = [30.0, 40.0, 50.0];
        assert_eq!(collapse(&w, 20.0, 400.0), [Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn the_middle_collapses_from_the_root_end() {
        // 4 segments of 40: 4*40 + 3*slot doesn't fit in 150, first+…+last+last-1 does not
        // either, first+…+last does.
        let w = [40.0; 4];
        let three = row_width(&[40.0, 20.0, 40.0, 40.0]);
        assert_eq!(collapse(&w, 20.0, three), [Some(0), None, Some(2), Some(3)]);
        let two = row_width(&[40.0, 20.0, 40.0]);
        assert_eq!(collapse(&w, 20.0, two), [Some(0), None, Some(3)]);
    }

    #[test]
    fn the_first_and_last_always_stay() {
        assert_eq!(collapse(&[40.0; 5], 20.0, 10.0), [Some(0), None, Some(4)]);
    }

    #[test]
    fn two_segments_never_collapse() {
        assert_eq!(collapse(&[40.0; 2], 20.0, 10.0), [Some(0), Some(1)]);
    }
}
