//! The proof-of-concept sheet: linen with a normal and a pressed Aqua button. Shared by the
//! golden test and the `poc` example.

use vello_common::kurbo::Rect;

use crate::materials::{ButtonState, aqua_button, fill_linen};
use crate::{Appearance, Scene};

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
    let y = (SHEET_HEIGHT - 22.0) / 2.0;
    for (x, state) in [(24.0, ButtonState::Normal), (132.0, ButtonState::Pressed)] {
        aqua_button(
            scene,
            Rect::new(x, y, x + 84.0, y + 22.0),
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
