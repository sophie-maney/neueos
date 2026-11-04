//! Masonry spike (task 2.1): a `masonry_core` widget that paints the neue-paint button tile
//! through [`ImagingReplayer`]. Shared by `tests/masonry.rs` and the `masonry_button`
//! example. Not the real push button: that is task 2.9 in neue-kit.

use masonry_core::accesskit::{Node, Role};
use masonry_core::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, Widget,
};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Axis, Size};
use masonry_core::layout::{LenReq, Length};
use neue_paint::Appearance;
use neue_paint::demo::{self, BUTTON_TILE_HEIGHT, BUTTON_TILE_WIDTH};
use neue_paint::imaging::ImagingReplayer;
use neue_paint::materials::ButtonState;

/// One Aqua button on linen. Pressed while the pointer is held on it, unless a state is
/// forced (for goldens).
#[derive(Debug)]
pub struct AquaButtonTile {
    appearance: Appearance,
    forced: Option<ButtonState>,
    replayer: ImagingReplayer,
}

impl AquaButtonTile {
    /// A tile that follows the pointer.
    pub fn new(appearance: Appearance) -> Self {
        Self {
            appearance,
            forced: None,
            replayer: ImagingReplayer::new(),
        }
    }

    /// A tile that always paints `state`.
    pub fn with_state(appearance: Appearance, state: ButtonState) -> Self {
        Self {
            forced: Some(state),
            ..Self::new(appearance)
        }
    }
}

impl Widget for AquaButtonTile {
    type Action = NoAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(..) => {
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(..) | PointerEvent::Cancel(..) => ctx.request_paint_only(),
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => Length::px(BUTTON_TILE_WIDTH),
            Axis::Vertical => Length::px(BUTTON_TILE_HEIGHT),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let state = self
            .forced
            .unwrap_or(if ctx.is_active() && ctx.is_hovered() {
                ButtonState::Pressed
            } else {
                ButtonState::Normal
            });
        let scene = demo::button_tile(self.appearance, state);
        self.replayer.replay(&scene, ctx.scale_factor(), painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label("Aqua button");
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
