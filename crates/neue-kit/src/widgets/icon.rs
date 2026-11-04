//! An icon on its own (task 2.20a, ADR 0051): [`crate::IconImage`] draws an
//! [`neue_paint::icon::Icon`] at a size. Rows and items that show icons draw them with
//! [`draw`].

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Rect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::peniko::Color;
use neue_paint::{Scene, Variant, icon::Icon};

use crate::theme::ThemeProp;

/// Draws `icon` in `rect`. A disabled vector icon is drawn at half opacity, because its own
/// colours don't follow `color` (which is the disabled colour); glyphs just take it.
pub(super) fn draw(
    scene: &mut Scene,
    rect: Rect,
    icon: &Icon,
    color: Color,
    variant: Variant,
    disabled: bool,
) {
    let dim = disabled && icon.vector().is_some();
    if dim {
        scene.push_opacity(0.5);
    }
    neue_paint::icon::icon(scene, rect, icon, color, variant);
    if dim {
        scene.pop_layer();
    }
}

/// The Masonry widget behind [`crate::IconImage`].
#[derive(Debug)]
pub(crate) struct IconWidget {
    icon: Icon,
    size: f64,
    label: Option<String>,
    replayer: ImagingReplayer,
}

impl IconWidget {
    pub(crate) fn new(icon: Icon, size: f64) -> Self {
        Self {
            icon,
            size,
            label: None,
            replayer: ImagingReplayer::new(),
        }
    }

    pub(crate) fn set_size(&mut self, size: f64) {
        self.size = size;
    }

    pub(crate) fn set_label(&mut self, label: String) {
        self.label = Some(label);
    }

    pub(crate) fn edit_icon(this: &mut WidgetMut<'_, Self>, icon: Icon) {
        if this.widget.icon != icon {
            this.widget.icon = icon;
            this.ctx.request_render();
            this.ctx.request_accessibility_update();
        }
    }

    /// What assistive technology is told: the given label, else a glyph's name, else nothing
    /// (a decorative vector icon).
    fn description(&self) -> Option<&str> {
        self.label
            .as_deref()
            .or_else(|| self.icon.glyph().map(neue_paint::icon::Glyph::name))
    }
}

impl UsesProperty<ThemeProp> for IconWidget {}

impl Widget for IconWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Masonry's disabled pass doesn't repaint on its own.
        if matches!(event, Update::DisabledChanged(_)) {
            ctx.request_paint_only();
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(self.size)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let colors = theme.colors();
        let disabled = ctx.is_disabled();
        let color = if disabled {
            colors.disabled_label
        } else {
            colors.label
        };
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        draw(
            &mut scene,
            size.to_rect(),
            &self.icon,
            color,
            theme.variant,
            disabled,
        );
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
        if let Some(description) = self.description() {
            node.set_label(description);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
