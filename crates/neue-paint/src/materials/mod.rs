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
mod menu;
pub mod menu_bar;
mod popup;
mod progress;
mod scroller;
mod segmented;
mod slider;
mod spinner;
mod stepper;
mod table;
mod text_field;
pub mod title_bar;
mod toggle;

pub use aqua_button::{ButtonState, PushButton, aqua_button, push_button, push_button_outline};
pub use bevel_button::{
    BevelButton, RecessedButton, bevel_button, bevel_button_outline, recessed_button,
    recessed_button_outline,
};
pub use focus_ring::focus_ring;
pub use linen::{Linen, fill_linen};
pub use menu::{
    MenuMark, menu_highlight, menu_mark, menu_outline, menu_panel, menu_separator, submenu_arrow,
};
pub use popup::{
    PopUpButton, PopUpKind, combo_arrow, combo_arrow_rect, popup_button, popup_button_outline,
    popup_cap_rect,
};
pub use progress::{Progress, progress_bar};
pub use scroller::{Scroller, scroller, scroller_knob, scroller_travel};
pub use segmented::{Segmented, segment_edges, segmented, segmented_outline};
pub use slider::{Slider, slider, slider_knob, slider_value_at};
pub use spinner::{Spinner, spinner};
pub use stepper::{Stepper, StepperPart, stepper, stepper_outline, stepper_part_at};
pub use table::{TableHeader, column_edges, sort_arrow_rect, table_border, table_header};
pub use text_field::{
    FieldShape, TextField, cancel_button, cancel_button_rect, magnifier, text_field,
    text_field_inside, text_field_outline,
};
pub use toggle::{Mark, Toggle, ToggleKind, toggle, toggle_outline};
