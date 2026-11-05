//! The desktop background (task 3.2c): [`crate::desktop::Desktop`] fills its surface with the
//! default picture, or a decoded photo placed by a [`Fit`].

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef, RegisterCtx,
    UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Rect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Scene;
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::desktop::{self as material, Fit, Picture};

use crate::theme::ThemeProp;

/// What the background shows.
#[derive(Debug, Clone)]
pub(crate) enum Content {
    /// The built-in picture.
    Default,
    /// A photo and how it is placed.
    Photo(Picture, Fit),
}

/// The Masonry widget behind [`crate::desktop::Desktop`].
#[derive(Debug)]
pub(crate) struct DesktopWidget {
    pub(super) content: Content,
    /// The size it asks for; none when the surface decides (a layer surface does).
    pub(super) size: Option<(f64, f64)>,
    replayer: ImagingReplayer,
}

impl DesktopWidget {
    pub(crate) fn new(content: Content) -> Self {
        Self {
            content,
            size: None,
            replayer: ImagingReplayer::new(),
        }
    }

    pub(crate) fn edit_content(this: &mut WidgetMut<'_, Self>, content: Content) {
        this.widget.content = content;
        // The old photo's pixels would stay in the replayer's texture cache.
        this.widget.replayer = ImagingReplayer::new();
        this.ctx.request_render();
        this.ctx.request_accessibility_update();
    }
}

impl UsesProperty<ThemeProp> for DesktopWidget {}

impl Widget for DesktopWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
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
        // Without a size of its own it takes whatever the surface gives it.
        let (width, height) = self.size.unwrap_or_default();
        Length::px(match axis {
            Axis::Horizontal => width,
            Axis::Vertical => height,
        })
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let size = ctx.content_box().size();
        let rect = Rect::from_origin_size((0.0, 0.0), size);
        let mut scene = Scene::new(size.width, size.height);
        match &self.content {
            Content::Default => material::default_picture(&mut scene, rect, theme.variant),
            Content::Photo(picture, fit) => {
                material::picture(&mut scene, rect, picture, *fit, theme.variant);
            }
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label("Desktop picture");
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
