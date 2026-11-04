//! Typeface and type scale. Sizes are in points, which equal logical pixels.

/// Family name of the UI font (bundled by neue-paint; ADR 0013).
pub const UI_FAMILY: &str = "DejaVu Sans";

/// Body text, control labels, menu items and window titles.
pub const BODY: f32 = 13.0;
/// Secondary labels and small controls.
pub const SMALL: f32 = 11.0;
/// Mini controls and captions.
pub const MINI: f32 = 9.0;
/// Section headings in panes and sheets.
pub const HEADLINE: f32 = 15.0;
/// Large titles (onboarding, empty states).
pub const TITLE: f32 = 20.0;
/// Clock and widget numerals.
pub const DISPLAY: f32 = 28.0;
/// Window title text.
pub const WINDOW_TITLE: f32 = BODY;
