//! Children in a row or a column, with even spacing and padding.

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, UsesProperty, Widget, WidgetMut, WidgetPod,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Point, Size};
use masonry_core::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};

use crate::theme::ThemeProp;

/// Where children sit across a stack (horizontally in a column, vertically in a row).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    /// Left in a column, top in a row.
    #[default]
    Start,
    Center,
    /// Right in a column, bottom in a row.
    End,
    /// Stretched to the stack's width (column) or height (row).
    Fill,
}

/// The Masonry widget behind [`crate::Stack`].
#[derive(Debug)]
pub(crate) struct StackWidget {
    axis: Axis,
    pub(crate) spacing: f64,
    pub(crate) padding: f64,
    pub(crate) align: Align,
    children: Vec<WidgetPod<dyn Widget>>,
}

/// A size from lengths along and across `axis`.
fn size_along(axis: Axis, main: f64, cross: f64) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

impl StackWidget {
    pub(crate) fn new(axis: Axis, spacing: f64) -> Self {
        Self {
            axis,
            spacing,
            padding: 0.0,
            align: Align::Start,
            children: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, child: NewWidget<dyn Widget>) {
        self.children.push(child.to_pod());
    }

    pub(crate) fn edit_push(this: &mut WidgetMut<'_, Self>, child: NewWidget<dyn Widget>) {
        this.widget.children.push(child.to_pod());
        this.ctx.children_changed();
    }

    pub(crate) fn edit_clear(this: &mut WidgetMut<'_, Self>) {
        for child in std::mem::take(&mut this.widget.children) {
            this.ctx.remove_child(child);
        }
    }

    fn gaps(&self) -> f64 {
        self.spacing * self.children.len().saturating_sub(1) as f64
    }
}

impl UsesProperty<ThemeProp> for StackWidget {}

impl Widget for StackWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for child in &mut self.children {
            ctx.register_child(child);
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
        let padding = Length::px(2.0 * self.padding);
        let along = axis == self.axis;
        let used = if along {
            padding.saturating_add(Length::px(self.gaps()))
        } else {
            padding
        };
        let auto: LenDef = len_req.reduce(used).into();
        let inner_cross = cross_length.map(|c| c.saturating_sub(padding));
        let context = LayoutSize::maybe(axis.cross(), inner_cross);
        // Across the stack, children get its cross length; along it, they share it, so
        // none of them is told a length.
        let child_cross = if along { None } else { inner_cross };
        let mut length = Length::ZERO;
        for child in &mut self.children {
            let child_length = ctx.compute_length(child, auto, context, axis, child_cross);
            length = if along {
                length.saturating_add(child_length)
            } else {
                length.max(child_length)
            };
        }
        length.saturating_add(used)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let main_axis = self.axis;
        let cross_axis = main_axis.cross();
        let inner = Size::new(
            (size.width - 2.0 * self.padding).max(0.0),
            (size.height - 2.0 * self.padding).max(0.0),
        );
        let inner_cross = inner.get_coord(cross_axis);
        let cross_def = if self.align == Align::Fill {
            LenDef::Fixed(Length::px(inner_cross))
        } else {
            LenDef::FitContent(Length::px(inner_cross))
        };
        let auto = match main_axis {
            Axis::Horizontal => SizeDef::new(LenDef::MaxContent, cross_def),
            Axis::Vertical => SizeDef::new(cross_def, LenDef::MaxContent),
        };
        let mut offset = self.padding;
        for child in &mut self.children {
            let mut child_size = ctx.compute_size(child, auto, inner.into());
            if self.align == Align::Fill {
                child_size = size_along(main_axis, child_size.get_coord(main_axis), inner_cross);
            }
            ctx.run_layout(child, child_size);
            let free = (inner_cross - child_size.get_coord(cross_axis)).max(0.0);
            let cross = self.padding
                + match self.align {
                    Align::Start | Align::Fill => 0.0,
                    Align::Center => (free / 2.0).round(),
                    Align::End => free,
                };
            let origin = size_along(main_axis, offset, cross);
            ctx.place_child(child, Point::new(origin.width, origin.height));
            offset += child_size.get_coord(main_axis) + self.spacing;
        }
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
        self.children.iter().map(WidgetPod::id).collect()
    }
}
