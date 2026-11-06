//! The on-screen display (task 3.14a): [`crate::desktop::Osd`] paints the OSD material
//! (`neue_paint::materials::osd`) and fades in and out on a spring.

use std::any::TypeId;

use masonry_core::accesskit::{Live, Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, UsesProperty, Widget, WidgetMut,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Rect, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::osd::{self as material, Glyph};
use neue_paint::{Backdrop, Scene};
use neue_theme::Spring;

use crate::anim::Animated;
use crate::theme::ThemeProp;

/// The Masonry widget behind [`crate::desktop::Osd`].
#[derive(Debug)]
pub(crate) struct OsdWidget {
    pub(super) glyph: Glyph,
    pub(super) level: f64,
    pub(super) backdrop: Backdrop,
    /// Whether it is (heading to being) visible.
    shown: bool,
    /// Its opacity, 0 to 1.
    opacity: Animated,
    replayer: ImagingReplayer,
}

impl OsdWidget {
    /// Visible, at rest.
    pub(crate) fn new(glyph: Glyph, level: f64) -> Self {
        Self {
            glyph,
            level,
            backdrop: Backdrop::Opaque,
            shown: true,
            opacity: Animated::new(1.0, Spring::STANDARD),
            replayer: ImagingReplayer::new(),
        }
    }

    /// Transparent until it is added, then fading in.
    pub(crate) fn start_transparent(&mut self) {
        self.opacity = Animated::new(0.0, Spring::STANDARD);
    }

    /// Shows `glyph` and `level`, fading in if it was hidden.
    pub(crate) fn edit_show(this: &mut WidgetMut<'_, Self>, glyph: Glyph, level: f64) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        let widget = &mut *this.widget;
        widget.glyph = glyph;
        widget.level = level;
        widget.shown = true;
        widget.opacity.animate_to(&mut this.ctx, &theme, 1.0);
        this.ctx.request_paint_only();
        this.ctx.request_accessibility_update();
    }

    /// Fades out.
    pub(crate) fn edit_hide(this: &mut WidgetMut<'_, Self>) {
        let ThemeProp(theme) = *this.get_prop::<ThemeProp>();
        this.widget.shown = false;
        this.widget.opacity.animate_to(&mut this.ctx, &theme, 0.0);
        this.ctx.request_accessibility_update();
    }

    /// The opacity it paints with now.
    #[cfg(test)]
    pub(crate) fn opacity(&self) -> f64 {
        self.opacity.value()
    }

    fn label(&self) -> &'static str {
        match self.glyph {
            Glyph::Volume => "Volume",
            Glyph::Muted => "Volume muted",
            Glyph::Brightness => "Brightness",
        }
    }
}

impl UsesProperty<ThemeProp> for OsdWidget {}

impl Widget for OsdWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) && self.shown {
            let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
            self.opacity.animate_to(ctx, &theme, 1.0);
        }
    }

    fn property_changed(&mut self, ctx: &mut UpdateCtx<'_>, property_type: TypeId) {
        if property_type == TypeId::of::<ThemeProp>() {
            ctx.request_render();
            self.opacity.theme_changed(ctx);
        }
    }

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
        self.opacity.on_anim_frame(ctx, &theme, interval);
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(material::SIZE)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let opacity = self.opacity.value().clamp(0.0, 1.0);
        let size = ctx.content_box().size();
        let mut scene = Scene::new(size.width, size.height);
        // Fully faded out: an empty frame, so the transparent surface shows nothing.
        if opacity > 0.0 {
            let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
            let rect = Rect::from_origin_size((0.0, 0.0), size);
            let faded = opacity < 1.0;
            if faded {
                #[expect(clippy::cast_possible_truncation, reason = "an opacity in 0..=1")]
                scene.push_opacity(opacity as f32);
            }
            material::osd(
                &mut scene,
                rect,
                self.glyph,
                self.level,
                theme.variant,
                self.backdrop,
            );
            if faded {
                scene.pop_layer();
            }
        }
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Meter
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.label());
        node.set_live(Live::Polite);
        if !self.shown {
            node.set_hidden();
        }
        if self.glyph != Glyph::Muted {
            node.set_min_numeric_value(0.0);
            node.set_max_numeric_value(100.0);
            node.set_numeric_value((self.level.clamp(0.0, 1.0) * 100.0).round());
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn get_debug_text(&self) -> Option<String> {
        Some(format!("{:?} {:.2}", self.glyph, self.level))
    }
}
