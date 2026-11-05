//! Sheets (task 2.23b, ADR 0057): a modal panel that comes out from under a window's
//! title bar. Square-cornered, in the window's background colour, with the bar's shadow
//! falling on its top edge and a hairline rim down its sides and along its bottom. Drawn
//! over the window it belongs to (an in-window sheet), it also casts a drop shadow beside
//! and below it; as a window of its own the compositor draws that.
//!
//! Colours and metrics come from [`neue_theme::materials::sheet`].

use neue_theme::materials::sheet::{
    INNER_SHADOW, Palette, RIM, SHADOW_BLUR, SHADOW_OFFSET, SHADOW_REACH,
};
use vello_common::kurbo::{Point, Rect, Shape, Size, Vec2};
use vello_common::peniko::Gradient;

use crate::Variant;
use crate::scene::Scene;

/// Draw a sheet of `size` at the origin (logical pixels): body, the title bar's shadow
/// along the top, and the rim.
pub fn sheet(scene: &mut Scene, size: Size, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let body = Rect::from_origin_size(Point::ORIGIN, size);
    scene.fill(&body, p.fill);

    let depth = INNER_SHADOW.min(size.height);
    if depth > 0.0 {
        let fade = Gradient::new_linear(Point::ORIGIN, Point::new(0.0, depth))
            .with_stops([(0.0, p.inner_shadow), (1.0, p.inner_shadow.with_alpha(0.0))]);
        scene.fill(&Rect::new(0.0, 0.0, size.width, depth), fade);
    }

    // The rim runs down both sides and along the bottom; the top is under the title bar.
    let rim = RIM.min(size.width / 2.0).min(size.height);
    for edge in [
        Rect::new(0.0, 0.0, rim, size.height),
        Rect::new(size.width - rim, 0.0, size.width, size.height),
        Rect::new(rim, size.height - rim, size.width - rim, size.height),
    ] {
        scene.fill(&edge, p.rim);
    }
}

/// Draw the drop shadow of a sheet of `size` at the origin: beside and below it, reaching
/// [`SHADOW_REACH`] out, never above its top edge (that is the title bar) nor under it.
pub fn sheet_shadow(scene: &mut Scene, size: Size, variant: impl Into<Variant>) {
    let p = Palette::new(variant.into());
    let body = Rect::from_origin_size(Point::ORIGIN, size);
    let mut frame = Rect::new(
        -SHADOW_REACH,
        0.0,
        size.width + SHADOW_REACH,
        size.height + SHADOW_REACH,
    )
    .to_path(0.1);
    frame.extend(body.to_path(0.1).reverse_subpaths());
    scene.push_clip(&frame);
    scene.blurred_rounded_rect(
        body + Vec2::new(0.0, SHADOW_OFFSET),
        0.0,
        SHADOW_BLUR,
        p.shadow,
    );
    scene.pop_layer();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Appearance;

    #[test]
    fn layers_are_balanced() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let mut scene = Scene::new(300.0, 200.0);
            sheet_shadow(&mut scene, Size::new(200.0, 120.0), appearance);
            sheet(&mut scene, Size::new(200.0, 120.0), appearance);
            assert_eq!(scene.open_layers(), 0);
        }
    }

    #[test]
    fn an_empty_sheet_draws_without_panicking() {
        let mut scene = Scene::new(10.0, 10.0);
        sheet(&mut scene, Size::ZERO, Appearance::Light);
        sheet_shadow(&mut scene, Size::ZERO, Appearance::Dark);
        assert_eq!(scene.open_layers(), 0);
    }
}
