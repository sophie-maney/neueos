//! The widgets the goldens render, shared with the gallery's pages.

use neue_kit::{Button, MaterialTile, Specimen};

/// Golden window size for one push button (logical pixels): the button with the fixture's
/// margin, room for its focus ring.
pub const PUSH_BUTTON_SIZE: (u32, u32) = (90, 40);

/// An ordinary push button.
pub fn push_button<M: 'static>() -> Button<M> {
    Button::new("Button")
}

/// The window's default button (Return presses it).
pub fn default_button<M: 'static>() -> Button<M> {
    Button::new("OK").as_default()
}

/// A material over a stand-in for a blurred desktop: opaque, or translucent if
/// `translucent`. Sized by the tile.
pub fn material(tile: MaterialTile, translucent: bool) -> Specimen {
    let specimen = Specimen::new(tile);
    if translucent {
        specimen.translucent()
    } else {
        specimen
    }
}

/// Golden window size for `tile`: the tile with the fixture's margin.
pub fn material_window_size(tile: MaterialTile) -> (u32, u32) {
    let (width, height) = tile.size();
    // Tile sizes are small positive whole numbers.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "tile sizes are small"
    )]
    let size = ((width.ceil() as u32) + 16, (height.ceil() as u32) + 16);
    size
}
