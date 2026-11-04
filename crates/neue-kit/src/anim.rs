//! Springs on Masonry's animation frames (ADR 0036): the glue between neue-theme's
//! [`SpringValue`] and neue-kit's widgets.
//!
//! A widget keeps an [`Animated`] per moving quantity (a knob's position, a highlight's
//! opacity), reads [`Animated::value`] when it paints, and forwards three hooks:
//! - when the quantity should change, [`Animated::animate_to`] with the widget's theme;
//! - in `Widget::on_anim_frame`, [`Animated::on_anim_frame`] with the widget's theme;
//! - in `Widget::property_changed` for the theme, [`Animated::theme_changed`].
//!
//! Under reduced motion the value snaps to its end and no frames are requested.

use masonry_core::core::{EventCtx, MutateCtx, UpdateCtx};
use neue_theme::{Spring, SpringValue, Theme};
use std::time::Duration;

/// The contexts a widget can start an animation from.
pub(crate) trait AnimCtx {
    fn request_anim_frame(&mut self);
    fn request_paint_only(&mut self);
}

macro_rules! impl_anim_ctx {
    ($($ctx:ident),+) => {$(
        impl AnimCtx for $ctx<'_> {
            fn request_anim_frame(&mut self) {
                $ctx::request_anim_frame(self);
            }
            fn request_paint_only(&mut self) {
                $ctx::request_paint_only(self);
            }
        }
    )+};
}
impl_anim_ctx!(EventCtx, UpdateCtx, MutateCtx);

/// A widget's sprung quantity and the `neue-theme` preset it moves on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Animated {
    value: SpringValue,
    preset: Spring,
}

impl Animated {
    /// At rest at `value`; later moves use `preset`.
    pub(crate) fn new(value: f64, preset: Spring) -> Self {
        Self {
            value: SpringValue::new(value),
            preset,
        }
    }

    /// The value to paint with.
    pub(crate) fn value(&self) -> f64 {
        self.value.value()
    }

    /// Whether the value is still moving.
    pub(crate) fn is_animating(&self) -> bool {
        self.value.is_animating()
    }

    /// Heads for `target`: on the preset spring, or straight there under `theme`'s reduced
    /// motion. Repaints, and requests animation frames while the value moves.
    pub(crate) fn animate_to(&mut self, ctx: &mut impl AnimCtx, theme: &Theme, target: f64) {
        if self.value.target() == target {
            return;
        }
        if self
            .value
            .animate_to(target, theme.motion().spring(self.preset))
        {
            ctx.request_anim_frame();
        }
        ctx.request_paint_only();
    }

    /// Forward from `Widget::on_anim_frame` with the widget's theme: steps by `interval`
    /// (nanoseconds), repaints, and requests the next frame until the value rests. If
    /// reduced motion has been switched on, the value jumps to its end instead.
    pub(crate) fn on_anim_frame(&mut self, ctx: &mut UpdateCtx<'_>, theme: &Theme, interval: u64) {
        if !self.value.is_animating() {
            return;
        }
        if theme.motion().spring(self.preset).is_none() {
            self.value.finish();
        } else if self.value.step(Duration::from_nanos(interval)) {
            ctx.request_anim_frame();
        }
        ctx.request_paint_only();
    }

    /// Forward from `Widget::property_changed` for the theme. Masonry gives that hook no
    /// properties, so a moving value asks for one more frame, where
    /// [`Self::on_anim_frame`] sees the new theme (and snaps under reduced motion).
    pub(crate) fn theme_changed(&mut self, ctx: &mut UpdateCtx<'_>) {
        if self.value.is_animating() {
            ctx.request_anim_frame();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::sync::Arc;

    use masonry_core::accesskit::{Node, Role};
    use masonry_core::core::{
        AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, PaintCtx, PropertiesMut,
        PropertiesRef, RegisterCtx, UsesProperty, Widget,
    };
    use masonry_core::imaging::Painter;
    use masonry_core::kurbo::{Axis, Size};
    use masonry_core::layout::{LenReq, Length};
    use masonry_testing::{TestHarness, TestHarnessParams};
    use neue_theme::{Spring, Theme};

    use super::Animated;
    use crate::theme::{ThemeProp, default_properties};

    /// A widget whose only state is one sprung quantity.
    struct Mover(Animated);

    impl UsesProperty<ThemeProp> for Mover {}

    impl Widget for Mover {
        type Action = ();

        fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

        fn on_anim_frame(
            &mut self,
            ctx: &mut masonry_core::core::UpdateCtx<'_>,
            props: &mut PropertiesMut<'_>,
            interval: u64,
        ) {
            let ThemeProp(theme) = *props.get::<ThemeProp>(ctx.property_cache());
            self.0.on_anim_frame(ctx, &theme, interval);
        }

        fn property_changed(
            &mut self,
            ctx: &mut masonry_core::core::UpdateCtx<'_>,
            property_type: TypeId,
        ) {
            if property_type == TypeId::of::<ThemeProp>() {
                self.0.theme_changed(ctx);
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
            Length::px(10.0)
        }

        fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

        fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

        fn accessibility_role(&self) -> Role {
            Role::GenericContainer
        }

        fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

        fn children_ids(&self) -> ChildrenIds {
            ChildrenIds::new()
        }
    }

    fn properties(theme: Theme) -> masonry_core::core::DefaultProperties {
        let mut props = default_properties(theme);
        props.insert::<Mover, _>(ThemeProp(theme));
        props
    }

    fn harness(theme: Theme) -> TestHarness<Mover> {
        TestHarness::create_with(
            properties(theme),
            NewWidget::new(Mover(Animated::new(0.0, Spring::STANDARD))),
            TestHarnessParams::default().with_size((20, 20)),
        )
    }

    /// Starts the move to `target` as an event handler would, with the harness's theme.
    fn start(harness: &mut TestHarness<Mover>, theme: Theme, target: f64) {
        harness.edit_root_widget(|mut mover| {
            mover.widget.0.animate_to(&mut mover.ctx, &theme, target);
        });
    }

    fn value(harness: &TestHarness<Mover>) -> f64 {
        harness.root_widget().inner().0.value()
    }

    /// Runs 8 ms frames until the widget stops asking; returns the values painted.
    fn frames(harness: &mut TestHarness<Mover>) -> Vec<f64> {
        let mut values = vec![];
        while harness.render_root().needs_anim() {
            harness.animate_ms(8);
            values.push(value(harness));
            assert!(values.len() < 1000, "animation never ended");
        }
        values
    }

    #[test]
    fn anim_frames_move_the_value_to_its_target_deterministically() {
        let theme = Theme::default();
        let run = || {
            let mut h = harness(theme);
            start(&mut h, theme, 100.0);
            frames(&mut h)
        };
        let values = run();
        assert_eq!(values, run(), "same frames, same values");
        assert!(values.len() > 10, "animated over several frames");
        assert!(values.windows(2).all(|w| w[0] <= w[1]), "no overshoot");
        assert_eq!(values.last(), Some(&100.0));
    }

    #[test]
    fn reduced_motion_snaps_without_frames() {
        let theme = Theme {
            reduced_motion: true,
            ..Theme::default()
        };
        let mut h = harness(theme);
        start(&mut h, theme, 100.0);
        assert_eq!(value(&h), 100.0);
        assert!(!h.render_root().needs_anim());
    }

    #[test]
    fn switching_to_reduced_motion_mid_flight_finishes() {
        let theme = Theme::default();
        let mut h = harness(theme);
        start(&mut h, theme, 100.0);
        h.animate_ms(8);
        h.animate_ms(8);
        assert!((0.0..100.0).contains(&value(&h)));

        let reduced = Theme {
            reduced_motion: true,
            ..theme
        };
        h.set_default_properties(Arc::new(properties(reduced)));
        let rest = frames(&mut h);
        assert_eq!(rest, [100.0], "one frame, straight to the end");
        assert_eq!(value(&h), 100.0);
        assert!(!h.root_widget().inner().0.is_animating());
    }
}
