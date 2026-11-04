//! Materials: procedural textures and vector-drawn controls. Each has a light and a dark
//! variant. The translucent ones (menu bar, dock shelf, HUD) also take a
//! [`crate::Backdrop`].

mod aqua_button;
mod bevel_button;
pub mod dock_shelf;
mod focus_ring;
mod groove;
pub mod hud;
mod linen;
pub mod menu_bar;
mod progress;
mod segmented;
mod slider;
mod spinner;
mod stepper;
pub mod title_bar;
mod toggle;

pub use aqua_button::{ButtonState, PushButton, aqua_button, push_button, push_button_outline};
pub use bevel_button::{
    BevelButton, RecessedButton, bevel_button, bevel_button_outline, recessed_button,
    recessed_button_outline,
};
pub use focus_ring::focus_ring;
pub use linen::{Linen, fill_linen};
pub use progress::{Progress, progress_bar};
pub use segmented::{Segmented, segment_edges, segmented, segmented_outline};
pub use slider::{Slider, slider, slider_knob, slider_value_at};
pub use spinner::{Spinner, spinner};
pub use stepper::{Stepper, StepperPart, stepper, stepper_outline, stepper_part_at};
pub use toggle::{Mark, Toggle, ToggleKind, toggle, toggle_outline};
