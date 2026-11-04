//! The segmented control: one choice among a few short labels. A click selects a segment;
//! Left and Right move the selection while the control has keyboard focus. The accent
//! highlight slides to the new segment on a spring.

use std::any::TypeId;

use masonry_core::accesskit::{Action, Node, Role};
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Insets, Rect, Size, Vec2};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{Segmented as SegmentedSpec, segment_edges, segmented};
use neue_theme::materials::segmented::SELECTED_LABEL;
use neue_theme::metrics::control;
use neue_theme::{Spring, Theme};

use super::text::{TextLine, TextStyle};
use crate::Cx;
use crate::anim::{AnimCtx, Animated};
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// What a segmented control emits when the user selects a segment: its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selected(pub(crate) usize);

/// The Masonry widget behind [`crate::Segmented`].
#[derive(Debug)]
pub(crate) struct SegmentedWidget {
    lines: Vec<TextLine>,
    replayer: ImagingReplayer,
    selected: usize,
    /// The highlight's position as a (fractional) segment index.
    highlight: Animated,
    /// The segment under the pointer while it is held down.
    pressed: Option<usize>,
    /// Segment widths from the last layout: the labels' natural widths plus an equal share
    /// of any extra space.
    widths: Vec<f64>,
}

impl SegmentedWidget {
    fn new(labels: Vec<String>) -> Self {
        Self {
            lines: labels
                .into_iter()
                .map(|l| TextLine::new(l, TextStyle::Body))
                .collect(),
            replayer: ImagingReplayer::new(),
            selected: 0,
            highlight: Animated::new(0.0, Spring::SNAPPY),
            pressed: None,
            widths: Vec::new(),
        }
    }

    fn start_selection(&mut self, selected: usize) {
        self.selected = selected.min(self.lines.len().saturating_sub(1));
        #[expect(clippy::cast_precision_loss, reason = "segment counts are tiny")]
        let at = self.selected as f64;
        self.highlight = Animated::new(at, Spring::SNAPPY);
    }

    /// Sets the selection from the app: no message goes back.
    pub(crate) fn edit_selected(this: &mut WidgetMut<'_, Self>, selected: usize) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        let selected = selected.min(this.widget.lines.len().saturating_sub(1));
        this.widget.move_to(&mut this.ctx, &theme, selected);
        this.ctx.request_accessibility_update();
    }

    fn move_to(&mut self, ctx: &mut impl AnimCtx, theme: &Theme, selected: usize) {
        self.selected = selected;
        #[expect(clippy::cast_precision_loss, reason = "segment counts are tiny")]
        let at = selected as f64;
        self.highlight.animate_to(ctx, theme, at);
    }

    /// The user selected segment `i`: move the highlight and tell the app.
    fn select(&mut self, ctx: &mut EventCtx<'_>, props: &mut PropertiesMut<'_>, i: usize) {
        if i == self.selected || i >= self.lines.len() {
            return;
        }
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.move_to(ctx, &theme, i);
        ctx.submit_action::<Selected>(Selected(i));
        ctx.request_accessibility_update();
    }

    /// The segment at local `x`, if any.
    fn segment_at(&self, x: f64) -> Option<usize> {
        let edges = segment_edges(Rect::new(0.0, 0.0, 0.0, 0.0), &self.widths);
        (0..self.widths.len()).find(|&i| x >= edges[i] && x < edges[i + 1])
    }
}

impl UsesProperty<ThemeProp> for SegmentedWidget {}

impl Widget for SegmentedWidget {
    type Action = Selected;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                let x = ctx.local_position(state.position).x;
                self.pressed = self.segment_at(x);
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent { state, .. }) => {
                let x = ctx.local_position(state.position).x;
                let pressed = self.pressed.take();
                if ctx.is_active()
                    && ctx.is_hovered()
                    && let Some(i) = pressed.filter(|&p| self.segment_at(x) == Some(p))
                {
                    self.select(ctx, props, i);
                }
                ctx.request_paint_only();
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
        props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        match event {
            TextEvent::Keyboard(key) if key.state.is_down() => {
                let target = match key.key {
                    Key::Named(NamedKey::ArrowLeft) => self.selected.checked_sub(1),
                    Key::Named(NamedKey::ArrowRight) => Some(self.selected + 1),
                    _ => return,
                };
                if let Some(i) = target {
                    self.select(ctx, props, i);
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
        props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        match event.action {
            Action::Increment => self.select(ctx, props, self.selected + 1),
            Action::Decrement => {
                if let Some(i) = self.selected.checked_sub(1) {
                    self.select(ctx, props, i);
                }
            }
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

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
            self.highlight.theme_changed(ctx);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.highlight.on_anim_frame(ctx, &theme, interval);
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
                let mut total = 0.0;
                for line in &mut self.lines {
                    let text = line.size(ctx.text_contexts());
                    total += (text.width + 2.0 * control::SEGMENT_PADDING)
                        .max(control::SEGMENT_MIN_WIDTH)
                        .ceil();
                }
                total
            }
            Axis::Vertical => control::REGULAR_HEIGHT,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let mut widths = Vec::with_capacity(self.lines.len());
        for line in &mut self.lines {
            let text = line.size(ctx.text_contexts());
            widths.push(
                (text.width + 2.0 * control::SEGMENT_PADDING)
                    .max(control::SEGMENT_MIN_WIDTH)
                    .ceil(),
            );
        }
        // Spread any extra room equally, so the segments always fill the control.
        let natural: f64 = widths.iter().sum();
        #[expect(clippy::cast_precision_loss, reason = "segment counts are tiny")]
        let share = (size.width - natural) / widths.len().max(1) as f64;
        for w in &mut widths {
            *w += share;
        }
        self.widths = widths;

        let text_height = self
            .lines
            .first_mut()
            .map_or(0.0, |l| l.size(ctx.text_contexts()).height);
        let baseline =
            (size.height - text_height) / 2.0 + self.lines.first().map_or(0.0, TextLine::baseline);
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
        let rect = Rect::from_origin_size((0.0, 0.0), size);
        let disabled = ctx.is_disabled();
        let selection = self.highlight.value();
        let spec = SegmentedSpec {
            widths: &self.widths,
            selection,
            pressed: self.pressed.filter(|_| ctx.is_active() && ctx.is_hovered()),
            disabled,
            focused: ctx.is_focus_target() && ctx.is_window_focused(),
        };
        let mut scene = Scene::new(size.width, size.height);
        segmented(&mut scene, rect, spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);

        // Labels: white over the highlight, fading with how much of it covers them.
        let colors = theme.colors();
        let rest = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        let edges = segment_edges(rect, &self.widths);
        for (i, line) in self.lines.iter_mut().enumerate() {
            let text = line.size(ctx.text_contexts());
            #[expect(clippy::cast_precision_loss, reason = "segment counts are tiny")]
            let covered = (1.0 - (selection - i as f64).abs()).clamp(0.0, 1.0);
            #[expect(clippy::cast_possible_truncation, reason = "a 0..1 blend amount")]
            let color = if disabled {
                rest
            } else {
                mix(rest, SELECTED_LABEL, covered as f32)
            };
            let origin = Vec2::new(
                (edges[i] + (edges[i + 1] - edges[i] - text.width) / 2.0).round(),
                ((size.height - text.height) / 2.0).round(),
            );
            line.paint(painter, Affine::translate(origin), color);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::RadioGroup
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if let Some(line) = self.lines.get(self.selected) {
            node.set_value(line.text());
        }
        node.add_action(Action::Increment);
        node.add_action(Action::Decrement);
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

/// The colour `t` of the way from `a` to `b`, alpha included.
fn mix(
    a: masonry_core::peniko::Color,
    b: masonry_core::peniko::Color,
    t: f32,
) -> masonry_core::peniko::Color {
    let (a, b) = (a.components, b.components);
    masonry_core::peniko::Color::new(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t))
}

/// A row of segments with one selected: a click selects a segment, and Left and Right move
/// the selection while the control has keyboard focus (Tab moves focus). Sends the new
/// segment's index. The selection is always one of the segments.
pub struct Segmented<M> {
    inner: NewWidget<SegmentedWidget>,
    on_select: Option<Box<dyn FnMut(usize) -> M>>,
}

impl<M> std::fmt::Debug for Segmented<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segmented")
            .field("inner", &self.inner)
            .field("on_select", &self.on_select.is_some())
            .finish()
    }
}

impl<M: 'static> Segmented<M> {
    /// Segments titled `labels`, the first one selected.
    pub fn new<S: Into<String>>(labels: impl IntoIterator<Item = S>) -> Self {
        let labels = labels.into_iter().map(Into::into).collect();
        Self {
            inner: NewWidget::new(SegmentedWidget::new(labels)),
            on_select: None,
        }
    }

    /// Starts with segment `index` selected (the last one if it is out of range).
    #[must_use]
    pub fn selected(mut self, index: usize) -> Self {
        self.inner.widget.start_selection(index);
        self
    }

    /// Sends the message `make` returns, given the index of the segment the user selected.
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

impl<M: 'static> From<Segmented<M>> for Element<M> {
    fn from(control: Segmented<M>) -> Self {
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

impl<M> Handle<Segmented<M>> {
    /// Selects segment `index` (the last one if it is out of range).
    pub fn set_selected<N>(&self, cx: &mut Cx<'_, N>, index: usize) {
        cx.edit::<SegmentedWidget, _>(self, |mut w| SegmentedWidget::edit_selected(&mut w, index));
    }

    /// Enables or disables the control.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<SegmentedWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}
