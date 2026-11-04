//! A child drawn at a multiple of its size: laid out in its own (smaller) coordinates and
//! scaled up by its transform, so vector content stays sharp and pointer input still hits.

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Point, Size};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length};

/// The Masonry widget behind [`crate::Zoom`].
#[derive(Debug)]
pub(crate) struct ZoomWidget {
    factor: f64,
    child: WidgetPod<dyn Widget>,
}

/// Zoom factors below this are clamped: layout divides by the factor.
const MIN_FACTOR: f64 = 0.1;

impl ZoomWidget {
    pub(crate) fn new(child: NewWidget<dyn Widget>, factor: f64) -> Self {
        let factor = factor.max(MIN_FACTOR);
        Self {
            factor,
            child: child.with_transform(Affine::scale(factor)).to_pod(),
        }
    }

    pub(crate) fn edit_factor(this: &mut WidgetMut<'_, Self>, factor: f64) {
        let factor = factor.max(MIN_FACTOR);
        if this.widget.factor == factor {
            return;
        }
        this.widget.factor = factor;
        this.ctx.request_layout();
        let mut child = this.ctx.get_mut(&mut this.widget.child);
        child.ctx.set_transform(Affine::scale(factor));
    }

    /// `length` in the child's coordinates.
    fn inward(&self, length: Length) -> Length {
        Length::px(length.get() / self.factor)
    }
}

impl Widget for ZoomWidget {
    type Action = NoAction;

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
        let auto = match len_req {
            LenReq::MinContent => LenDef::MinContent,
            LenReq::MaxContent => LenDef::MaxContent,
            LenReq::FitContent(space) => LenDef::FitContent(self.inward(space)),
        };
        let cross = cross_length.map(|c| self.inward(c));
        let context = LayoutSize::maybe(axis.cross(), cross);
        let inner = ctx.compute_length(&mut self.child, auto, context, axis, cross);
        Length::px(inner.get() * self.factor)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let inner = Size::new(size.width / self.factor, size.height / self.factor);
        ctx.run_layout(&mut self.child, inner);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.clear_baselines();
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

    fn children_ids(&self) -> ChildrenIds {
        std::iter::once(self.child.id()).collect()
    }
}
