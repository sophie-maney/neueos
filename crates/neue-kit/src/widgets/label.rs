//! A single line of non-editable text.

use std::any::TypeId;

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Axis, Size};
use masonry_core::layout::{LenReq, Length};

use neue_theme::materials::Backdrop;
use neue_theme::materials::hud::Palette as HudPalette;

use super::text::{TextLine, TextStyle};
use crate::theme::ThemeProp;

/// The Masonry widget behind [`crate::Label`].
#[derive(Debug)]
pub(crate) struct LabelWidget {
    line: TextLine,
    secondary: bool,
    /// Drawn on a HUD window: light text.
    on_hud: bool,
}

impl LabelWidget {
    pub(crate) fn new(text: String) -> Self {
        Self {
            line: TextLine::new(text, TextStyle::Body),
            secondary: false,
            on_hud: false,
        }
    }

    pub(crate) fn set_style(&mut self, style: TextStyle) {
        self.line.set_style(style);
    }

    pub(crate) fn set_secondary(&mut self, secondary: bool) {
        self.secondary = secondary;
    }

    pub(crate) fn set_on_hud(&mut self, on_hud: bool) {
        self.on_hud = on_hud;
    }

    pub(crate) fn text(&self) -> &str {
        self.line.text()
    }

    pub(crate) fn edit_text(this: &mut WidgetMut<'_, Self>, text: String) {
        if this.widget.line.set_text(text) {
            this.ctx.request_layout();
            this.ctx.request_accessibility_update();
        }
    }
}

impl UsesProperty<ThemeProp> for LabelWidget {}

impl Widget for LabelWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            // Masonry's disabled pass doesn't repaint on its own.
            Update::DisabledChanged(_) => ctx.request_paint_only(),
            Update::FontsChanged => {
                self.line.invalidate();
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
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let size = self.line.size(ctx.text_contexts());
        Length::px(match axis {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        })
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {
        self.line.size(ctx.text_contexts());
        let baseline = self.line.baseline();
        ctx.set_baselines(baseline, baseline);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        let colors = theme.colors();
        let hud = HudPalette::new(theme.variant, Backdrop::Opaque);
        let color = if self.on_hud {
            if self.secondary {
                hud.secondary_label
            } else {
                hud.label
            }
        } else if ctx.is_disabled() {
            colors.disabled_label
        } else if self.secondary {
            colors.secondary_label
        } else {
            colors.label
        };
        self.line.paint(painter, Affine::IDENTITY, color);
    }

    fn accessibility_role(&self) -> Role {
        Role::Label
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_value(self.line.text());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(self.line.text().to_owned())
    }
}
