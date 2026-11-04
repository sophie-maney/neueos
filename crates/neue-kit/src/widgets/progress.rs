//! The progress bar and the spinner: how far along something is, or just that it is busy.
//!
//! A bar with a value fills from the left (springing to each new value). A bar without
//! one is the barber pole: accent gel with diagonal stripes that move to the right. The
//! spinner turns in twelve steps. Both are ambient animation, so reduced motion stops them
//! where they are (ADR 0041).

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Insets, Rect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::{Progress, Spinner as SpinnerSpec, progress_bar, spinner};
use neue_theme::metrics::control;
use neue_theme::motion::{BARBER_POLE_PERIOD, SPINNER_PERIOD};
use neue_theme::{Spring, Theme};

use super::ticker::Ticker;
use crate::Cx;
use crate::anim::Animated;
use crate::element::{Element, Handle};
use crate::theme::ThemeProp;

/// Whether ambient animation should be running in `ctx`'s widget.
fn animating(ctx: &UpdateCtx<'_>, theme: &Theme, busy: bool) -> bool {
    busy && theme.motion().ambient() && !ctx.is_disabled()
}

/// The Masonry widget behind [`ProgressBar`].
#[derive(Debug)]
pub(crate) struct ProgressWidget {
    /// How far along, 0..1; `None` while it's unknown.
    value: Option<f64>,
    /// What is drawn: follows `value` on a spring.
    shown: Animated,
    stripes: Ticker,
    label: Option<String>,
    width: f64,
    replayer: ImagingReplayer,
}

impl ProgressWidget {
    fn new() -> Self {
        Self {
            value: None,
            shown: Animated::new(0.0, Spring::STANDARD),
            stripes: Ticker::default(),
            label: None,
            width: control::PROGRESS_WIDTH,
            replayer: ImagingReplayer::new(),
        }
    }

    fn set_start(&mut self, value: Option<f64>) {
        self.value = value.map(|v| v.clamp(0.0, 1.0));
        self.shown = Animated::new(self.value.unwrap_or(0.0), Spring::STANDARD);
    }

    /// Shows `value` (`None`: unknown, the barber pole), from the app.
    pub(crate) fn edit_value(this: &mut WidgetMut<'_, Self>, value: Option<f64>) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        let widget = &mut *this.widget;
        widget.value = value.map(|v| v.clamp(0.0, 1.0));
        if let Some(v) = widget.value {
            widget.shown.animate_to(&mut this.ctx, &theme, v);
        } else {
            widget.stripes.start(&mut this.ctx, true);
        }
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }
}

impl UsesProperty<ThemeProp> for ProgressWidget {}

impl Widget for ProgressWidget {
    type Action = ();

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => self.stripes.start(ctx, self.value.is_none()),
            Update::DisabledChanged(_) => {
                ctx.request_paint_only();
                self.stripes.start(ctx, self.value.is_none());
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
            self.shown.theme_changed(ctx);
            // Reduced motion may have been switched off.
            self.stripes.start(ctx, self.value.is_none());
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.shown.on_anim_frame(ctx, &theme, interval);
        let wanted = animating(ctx, &theme, self.value.is_none());
        self.stripes
            .frame(ctx, wanted, BARBER_POLE_PERIOD, interval);
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // The etched highlight below the groove sits inside the bar's height.
        Length::px(match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => control::PROGRESS_HEIGHT + 1.0,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
        ctx.set_paint_insets(Insets::uniform(control::RIM));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 fraction")]
        let spec = Progress {
            fraction: self.value.map(|_| self.shown.value() as f32),
            phase: self.stripes.amount(BARBER_POLE_PERIOD) as f32,
            disabled: ctx.is_disabled(),
        };
        let mut scene = Scene::new(size.width, size.height);
        let bar = Rect::new(0.0, 0.0, size.width, control::PROGRESS_HEIGHT);
        progress_bar(&mut scene, bar, spec, theme.variant);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::ProgressIndicator
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if let Some(label) = &self.label {
            node.set_label(label.as_str());
        }
        // Indeterminate bars have no value.
        if let Some(value) = self.value {
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(1.0);
            node.set_numeric_value(value);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("{:?}", self.value))
    }
}

/// A horizontal progress bar. Without a value it is a barber pole, for work of unknown
/// length; with one (0 to 1) it fills from the left, and springs to each new value.
///
/// Shows no text: put a label beside it. Unknown until [`ProgressBar::value`] says
/// otherwise.
#[derive(Debug)]
pub struct ProgressBar {
    inner: NewWidget<ProgressWidget>,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(ProgressWidget::new()),
        }
    }

    /// Starts at `value`, from 0 to 1 (kept inside it).
    #[must_use]
    pub fn value(mut self, value: f64) -> Self {
        self.inner.widget.set_start(Some(value));
        self
    }

    /// The width, in logical pixels. 150 by default.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.inner.widget.width = width.max(control::PROGRESS_HEIGHT);
        self
    }

    /// What assistive technology calls this bar, for example "Copying".
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.widget.label = Some(label.into());
        self
    }

    /// Starts disabled: greyed out, and the stripes stop.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.disabled(disabled);
        self
    }

    /// Points `handle` at this bar.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<ProgressBar> for Element<M> {
    fn from(bar: ProgressBar) -> Self {
        Element::new(bar.inner.erased())
    }
}

impl Handle<ProgressBar> {
    /// Fills the bar to `value`, from 0 to 1 (kept inside it); `None` turns it back into
    /// the barber pole.
    pub fn set_value<N>(&self, cx: &mut Cx<'_, N>, value: Option<f64>) {
        cx.edit::<ProgressWidget, _>(self, |mut w| ProgressWidget::edit_value(&mut w, value));
    }

    /// Enables or disables the bar.
    pub fn set_enabled<N>(&self, cx: &mut Cx<'_, N>, enabled: bool) {
        cx.edit::<ProgressWidget, _>(self, |mut w| w.ctx.set_disabled(!enabled));
    }
}

/// The Masonry widget behind [`Spinner`].
#[derive(Debug)]
pub(crate) struct SpinnerWidget {
    spinning: bool,
    turn: Ticker,
    label: Option<String>,
    replayer: ImagingReplayer,
}

impl SpinnerWidget {
    fn new() -> Self {
        Self {
            spinning: true,
            turn: Ticker::default(),
            label: None,
            replayer: ImagingReplayer::new(),
        }
    }

    /// Starts or stops spinning, from the app.
    pub(crate) fn edit_spinning(this: &mut WidgetMut<'_, Self>, spinning: bool) {
        this.widget.spinning = spinning;
        this.widget.turn.start(&mut this.ctx, spinning);
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }
}

impl UsesProperty<ThemeProp> for SpinnerWidget {}

impl Widget for SpinnerWidget {
    type Action = ();

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::WidgetAdded => self.turn.start(ctx, self.spinning),
            Update::DisabledChanged(_) => {
                ctx.request_paint_only();
                self.turn.start(ctx, self.spinning);
            }
            _ => {}
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
            // Reduced motion may have been switched off.
            self.turn.start(ctx, self.spinning);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let wanted = animating(ctx, &theme, self.spinning);
        self.turn.frame(ctx, wanted, SPINNER_PERIOD, interval);
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(control::SPINNER_SIZE)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        if !self.spinning {
            return;
        }
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        #[expect(clippy::cast_possible_truncation, reason = "a 0..1 phase")]
        let spec = SpinnerSpec {
            phase: self.turn.amount(SPINNER_PERIOD) as f32,
            disabled: ctx.is_disabled(),
        };
        let mut scene = Scene::new(size.width, size.height);
        spinner(
            &mut scene,
            Rect::from_origin_size((0.0, 0.0), size),
            spec,
            theme.variant,
        );
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::ProgressIndicator
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if let Some(label) = &self.label {
            node.set_label(label.as_str());
        }
        // A stopped spinner draws nothing, so it is hidden from assistive technology.
        if !self.spinning {
            node.set_hidden();
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.spinning.to_string())
    }
}

/// An indeterminate spinner: twelve spokes that turn while something runs. Stopped, it
/// draws nothing but keeps its space.
#[derive(Debug)]
pub struct Spinner {
    inner: NewWidget<SpinnerWidget>,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    /// A spinner, turning.
    pub fn new() -> Self {
        Self {
            inner: NewWidget::new(SpinnerWidget::new()),
        }
    }

    /// Starts stopped (`false`) or turning (`true`, the default).
    #[must_use]
    pub fn spinning(mut self, spinning: bool) -> Self {
        self.inner.widget.spinning = spinning;
        self
    }

    /// What assistive technology calls this spinner, for example "Loading".
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.inner.widget.label = Some(label.into());
        self
    }

    /// Points `handle` at this spinner.
    #[must_use]
    pub fn bind(self, handle: &mut Handle<Self>) -> Self {
        *handle = Handle::new(self.inner.id());
        self
    }
}

impl<M> From<Spinner> for Element<M> {
    fn from(spinner: Spinner) -> Self {
        Element::new(spinner.inner.erased())
    }
}

impl Handle<Spinner> {
    /// Starts or stops the spinner.
    pub fn set_spinning<N>(&self, cx: &mut Cx<'_, N>, spinning: bool) {
        cx.edit::<SpinnerWidget, _>(self, |mut w| SpinnerWidget::edit_spinning(&mut w, spinning));
    }
}
