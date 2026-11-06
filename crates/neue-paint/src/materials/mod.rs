//! Materials: procedural textures and vector-drawn controls. Each has a light and a dark
//! variant. The translucent ones (menu bar, dock shelf, HUD) also take a
//! [`crate::Backdrop`].

mod aqua_button;
mod bevel_button;
mod color_well;
mod date_picker;
pub mod desktop;
pub mod dock_shelf;
mod focus_ring;
mod groove;
pub mod hud;
mod linen;
mod menu;
pub mod menu_bar;
pub mod notification;
pub mod osd;
mod path_control;
mod popover;
mod popup;
mod progress;
mod scroller;
mod segmented;
mod sheet;
mod sidebar;
mod slider;
mod spinner;
mod split;
mod stepper;
mod table;
mod tabs;
mod text_field;
pub mod title_bar;
mod toggle;
mod toolbar;

pub use aqua_button::{ButtonState, PushButton, aqua_button, push_button, push_button_outline};
pub use bevel_button::{
    BevelButton, RecessedButton, bevel_button, bevel_button_outline, recessed_button,
    recessed_button_outline,
};
pub use color_well::{
    ColorArea, ColorWell, Hsb, checkerboard, color_area, color_area_knob, color_area_value_at,
    color_hex, color_well, parse_hex,
};
pub use date_picker::{
    Calendar, CalendarLayout, CalendarPart, DateField, calendar, calendar_cell_outline,
    calendar_layout, calendar_size, date_field, date_field_outline,
};
pub use focus_ring::focus_ring;
pub use linen::{Linen, fill_linen};
pub use menu::{
    MenuMark, menu_highlight, menu_lock, menu_mark, menu_outline, menu_panel, menu_separator,
    menu_signal, submenu_arrow,
};
pub use path_control::{PathControl, path_control, path_segment_rects};
pub use popover::{
    Edge, popover, popover_body, popover_landing, popover_origin, popover_outline, popover_size,
};
pub use popup::{
    PopUpButton, PopUpKind, combo_arrow, combo_arrow_rect, popup_button, popup_button_outline,
    popup_cap_rect,
};
pub use progress::{Progress, progress_bar};
pub use scroller::{Scroller, scroller, scroller_knob, scroller_travel};
pub use segmented::{Segmented, segment_edges, segmented, segmented_outline};
pub use sheet::{sheet, sheet_shadow};
pub use sidebar::{
    SourceItem, SourceState, sidebar, source_badge, source_badge_rect, source_group_action,
    source_group_x, source_item, source_list_selection,
};
pub use slider::{Slider, slider, slider_knob, slider_value_at};
pub use spinner::{Spinner, spinner};
pub use split::{DividerStyle, split_divider};
pub use stepper::{Stepper, StepperPart, stepper, stepper_outline, stepper_part_at};
pub use table::{
    TableHeader, column_edges, disclosure_rect, disclosure_triangle, outline_text_inset,
    sort_arrow_rect, table_border, table_header,
};
pub use tabs::{TabStrip, tab_box, tab_strip, tab_strip_outline};
pub use text_field::{
    FieldShape, TextField, cancel_button, cancel_button_rect, magnifier, text_field,
    text_field_inside, text_field_outline,
};
pub use title_bar::{window_chrome, window_chrome_titled};
pub use toggle::{Mark, Toggle, ToggleKind, toggle, toggle_outline};
pub use toolbar::{ToolbarItem, toolbar_item, toolbar_item_outline, toolbar_separator};
