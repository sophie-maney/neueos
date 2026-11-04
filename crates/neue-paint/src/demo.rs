//! The proof-of-concept sheet: linen with a normal and a pressed Aqua button. Shared by the
//! golden test and the `poc` example.

use vello_common::kurbo::Rect;

use crate::materials::{ButtonState, aqua_button, fill_linen};
use crate::{Appearance, Scene, Variant};
use neue_theme::metrics::control::REGULAR_HEIGHT;

/// Sheet width in logical pixels.
pub const SHEET_WIDTH: f64 = 240.0;
/// Sheet height in logical pixels.
pub const SHEET_HEIGHT: f64 = 80.0;

/// Draw the sheet for `appearance` with its top-left corner at the current origin.
pub fn draw_sheet(scene: &mut Scene, appearance: Appearance) {
    fill_linen(
        scene,
        &Rect::new(0.0, 0.0, SHEET_WIDTH, SHEET_HEIGHT),
        appearance,
    );
    let y = (SHEET_HEIGHT - REGULAR_HEIGHT) / 2.0;
    for (x, state) in [(24.0, ButtonState::Normal), (132.0, ButtonState::Pressed)] {
        aqua_button(
            scene,
            Rect::new(x, y, x + 84.0, y + REGULAR_HEIGHT),
            state,
            appearance,
        );
    }
}

/// A scene containing just the sheet for `appearance`.
pub fn sheet(appearance: Appearance) -> Scene {
    let mut scene = Scene::new(SHEET_WIDTH, SHEET_HEIGHT);
    draw_sheet(&mut scene, appearance);
    scene
}

/// Button tile width in logical pixels: one Aqua button on linen.
pub const BUTTON_TILE_WIDTH: f64 = 108.0;
/// Button tile height in logical pixels.
pub const BUTTON_TILE_HEIGHT: f64 = 38.0;

/// Draw one button in `state` on linen, with the tile's top-left corner at the current
/// origin. Shared by the golden tests and the Masonry spike (task 2.1), so a Masonry
/// widget's render can be compared against neue-paint's own.
pub fn draw_button_tile(scene: &mut Scene, variant: impl Into<Variant>, state: ButtonState) {
    let variant = variant.into();
    fill_linen(
        scene,
        &Rect::new(0.0, 0.0, BUTTON_TILE_WIDTH, BUTTON_TILE_HEIGHT),
        variant,
    );
    aqua_button(scene, Rect::new(12.0, 8.0, 96.0, 30.0), state, variant);
}

/// A scene containing just the button tile.
pub fn button_tile(variant: impl Into<Variant>, state: ButtonState) -> Scene {
    let mut scene = Scene::new(BUTTON_TILE_WIDTH, BUTTON_TILE_HEIGHT);
    draw_button_tile(&mut scene, variant, state);
    scene
}
