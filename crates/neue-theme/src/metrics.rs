//! Spacing and control sizes, in logical pixels. Values that belong to one material live in
//! [`crate::materials`].

/// Spacing steps for padding and gaps between controls.
pub mod spacing {
    /// Hairline gap: between a control and its etched highlight.
    pub const XXS: f64 = 2.0;
    /// Tight gap, inside compound controls.
    pub const XS: f64 = 4.0;
    /// Between related controls.
    pub const S: f64 = 8.0;
    /// Between groups of controls.
    pub const M: f64 = 12.0;
    /// Window content inset.
    pub const L: f64 = 20.0;
}

/// Standard control heights.
pub mod control {
    /// Regular push buttons, pop-up buttons and text fields.
    pub const REGULAR_HEIGHT: f64 = 22.0;
    /// Small controls.
    pub const SMALL_HEIGHT: f64 = 19.0;
    /// Mini controls.
    pub const MINI_HEIGHT: f64 = 15.0;
    /// Minimum width of a push button with a short label.
    pub const BUTTON_MIN_WIDTH: f64 = 70.0;
    /// Horizontal padding between a push button's rim and its label.
    pub const BUTTON_PADDING: f64 = 12.0;
    /// Width of a control's rim (outer border).
    pub const RIM: f64 = 1.0;
    /// Width of the keyboard focus ring.
    pub const FOCUS_RING: f64 = 3.0;
    /// Side of a checkbox and diameter of a radio button.
    pub const CHECKBOX_SIZE: f64 = 14.0;
    /// Gap between a checkbox or radio button and its label.
    pub const CHECKBOX_GAP: f64 = 6.0;
    /// Horizontal padding between a segment's edges and its label.
    pub const SEGMENT_PADDING: f64 = 12.0;
    /// Minimum width of a segment.
    pub const SEGMENT_MIN_WIDTH: f64 = 40.0;
    /// Minimum width of a bevel or recessed button.
    pub const BEVEL_MIN_WIDTH: f64 = 28.0;
    /// Height of a slider's hit area (the knob is [`crate::materials::slider::KNOB`]).
    pub const SLIDER_HEIGHT: f64 = 19.0;
    /// Default width of a slider.
    pub const SLIDER_WIDTH: f64 = 120.0;
    /// Width of a stepper.
    pub const STEPPER_WIDTH: f64 = 13.0;
    /// Height of a stepper.
    pub const STEPPER_HEIGHT: f64 = 22.0;
    /// Height of a progress bar.
    pub const PROGRESS_HEIGHT: f64 = 12.0;
    /// Default width of a progress bar.
    pub const PROGRESS_WIDTH: f64 = 150.0;
    /// Side of a spinner.
    pub const SPINNER_SIZE: f64 = 16.0;
    /// Default width of a text, search or secure field.
    pub const TEXT_FIELD_WIDTH: f64 = 200.0;
    /// Space between a text field's edge and its text.
    pub const TEXT_FIELD_INSET: f64 = 4.0;
    /// Space between a search field's rounded ends and its text (room for the magnifier
    /// on the left and the cancel button on the right).
    pub const SEARCH_FIELD_INSET: f64 = 22.0;
    /// Width of an overlay scroller across its axis: the strip along a scroll view's edge
    /// that takes the pointer while the scroller shows.
    pub const SCROLLER_WIDTH: f64 = 15.0;
    /// How far one line of a mouse wheel, or an arrow key, scrolls.
    pub const SCROLL_LINE: f64 = 40.0;
}
