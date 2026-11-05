//! A material specimen: one of neue-paint's translucent materials drawn over a stand-in for
//! a blurred desktop. For the gallery's Materials page; apps draw materials through the
//! widgets that use them.

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, UsesProperty, Widget,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::demo::{self, MaterialTile};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::{Backdrop, Scene, Variant};

use crate::theme::ThemeProp;

/// The Masonry widget behind [`crate::Specimen`].
#[derive(Debug)]
pub(crate) struct SpecimenWidget {
    tile: MaterialTile,
    backdrop: Backdrop,
    replayer: ImagingReplayer,
    /// The scene for the theme variant it was drawn with: laying out its text is the
    /// expensive part.
    scene: Option<(Variant, Scene)>,
}

impl SpecimenWidget {
    pub(crate) fn new(tile: MaterialTile, backdrop: Backdrop) -> Self {
        Self {
            tile,
            backdrop,
            replayer: ImagingReplayer::new(),
            scene: None,
        }
    }

    pub(crate) fn set_backdrop(&mut self, backdrop: Backdrop) {
        self.backdrop = backdrop;
        self.scene = None;
    }

    fn description(&self) -> &'static str {
        match self.tile {
            MaterialTile::MenuBar => "Menu bar material",
            MaterialTile::DockShelf => "Dock shelf material, in perspective",
            MaterialTile::DockShelfFlat => "Dock shelf material, flat",
            MaterialTile::Hud => "HUD panel material",
            MaterialTile::Menu => "Menu material",
            MaterialTile::Popover => "Popover material",
            MaterialTile::Sheet => "Sheet material",
        }
    }
}

impl UsesProperty<ThemeProp> for SpecimenWidget {}

impl Widget for SpecimenWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Masonry's disabled pass doesn't repaint on its own.
        if matches!(event, Update::DisabledChanged(_)) {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: std::any::TypeId) {
        if property_type == std::any::TypeId::of::<ThemeProp>() {
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
        let (width, height) = self.tile.size();
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
        if self.scene.as_ref().is_none_or(|(v, _)| *v != theme.variant) {
            let scene = demo::material_tile(self.tile, theme.variant, self.backdrop);
            self.scene = Some((theme.variant, scene));
        }
        if let Some((_, scene)) = &self.scene {
            self.replayer.replay(scene, ctx.scale_factor(), painter);
        }
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
        node.set_label(self.description());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
