//! The widgets the goldens render, shared with the gallery's pages.

use neue_kit::{
    BevelButton, Button, CheckState, Checkbox, MaterialTile, ProgressBar, Radio, Segmented, Slider,
    Specimen, Spinner, Stepper,
};

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

/// Golden window size for one checkbox or radio button (logical pixels).
pub const TOGGLE_SIZE: (u32, u32) = (120, 40);

/// Golden window size for one bevel or recessed button.
pub const BEVEL_SIZE: (u32, u32) = (110, 40);

/// Golden window size for the segmented control.
pub const SEGMENTED_SIZE: (u32, u32) = (190, 40);

/// An unchecked checkbox.
pub fn checkbox<M: 'static>() -> Checkbox<M> {
    Checkbox::new("Checkbox")
}

/// A checked checkbox.
pub fn checkbox_on<M: 'static>() -> Checkbox<M> {
    checkbox().checked(true)
}

/// A checkbox in the mixed state.
pub fn checkbox_mixed<M: 'static>() -> Checkbox<M> {
    checkbox().state(CheckState::Mixed)
}

/// An unselected radio button.
pub fn radio<M: 'static>() -> Radio<M> {
    Radio::new("Radio")
}

/// A selected radio button.
pub fn radio_on<M: 'static>() -> Radio<M> {
    radio().selected(true)
}

/// A bevel button that is off.
pub fn bevel_button<M: 'static>() -> BevelButton<M> {
    BevelButton::new("Bevel")
}

/// A bevel button toggled on.
pub fn bevel_button_on<M: 'static>() -> BevelButton<M> {
    bevel_button().on(true)
}

/// A recessed button that is off.
pub fn recessed_button<M: 'static>() -> BevelButton<M> {
    BevelButton::recessed("Recessed")
}

/// A recessed button toggled on.
pub fn recessed_button_on<M: 'static>() -> BevelButton<M> {
    recessed_button().on(true)
}

/// A segmented control with `selected` chosen.
pub fn segmented<M: 'static>(selected: usize) -> Segmented<M> {
    Segmented::new(["One", "Two", "Three"]).selected(selected)
}

/// Golden window size for the slider (the default width plus the fixture's margin).
pub const SLIDER_SIZE: (u32, u32) = (150, 40);

/// Golden window size for the stepper.
pub const STEPPER_SIZE: (u32, u32) = (50, 50);

/// Golden window size for the progress bar.
pub const PROGRESS_SIZE: (u32, u32) = (180, 40);

/// Golden window size for the spinner.
pub const SPINNER_SIZE: (u32, u32) = (50, 40);

/// A slider over 0 to 1, at `value`.
pub fn slider<M: 'static>(value: f64) -> Slider<M> {
    Slider::new().value(value)
}

/// A stepper from 0 to 10, at 5.
pub fn stepper<M: 'static>() -> Stepper<M> {
    Stepper::new().range(0.0..=10.0).value(5.0)
}

/// A progress bar filled to `value` (0 to 1).
pub fn progress_bar(value: f64) -> ProgressBar {
    ProgressBar::new().value(value)
}

/// A progress bar of unknown length: the barber pole.
pub fn barber_pole() -> ProgressBar {
    ProgressBar::new()
}

/// A turning spinner.
pub fn spinner() -> Spinner {
    Spinner::new()
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
