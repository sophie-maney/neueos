//! Semantic colours: what a colour is for, not what it looks like. Widgets use these;
//! materials use [`crate::materials`].

use crate::{Accent, Appearance, Color, Variant, black, grey, rgb, rgba, white};

/// The semantic palette for one [`Variant`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticColors {
    /// Window and sheet background behind controls.
    pub window_background: Color,
    /// Background of text fields, tables and other editable content.
    pub content_background: Color,
    /// Alternate row stripe in tables.
    pub alternate_row: Color,
    /// Primary text.
    pub label: Color,
    /// Secondary text: captions, subtitles.
    pub secondary_label: Color,
    /// Tertiary text: placeholders.
    pub tertiary_label: Color,
    /// Text of disabled controls.
    pub disabled_label: Color,
    /// Embossed shadow under labels on gradients (one pixel below the text).
    pub label_emboss: Color,
    /// Hairlines between content areas.
    pub separator: Color,
    /// The accent colour itself (links, toggles, the default button's tint).
    pub accent: Color,
    /// Selection background in a focused view.
    pub selection: Color,
    /// Selection background in an unfocused view.
    pub unfocused_selection: Color,
    /// Text on [`Self::selection`].
    pub selected_label: Color,
    /// Selected text in a focused text field or text view (the text keeps its colour).
    pub text_selection: Color,
    /// Keyboard focus ring.
    pub focus_ring: Color,
    /// Drop shadows of windows, menus and popovers.
    pub shadow: Color,
}

impl SemanticColors {
    /// The palette for `variant`.
    pub fn new(variant: Variant) -> Self {
        let base = match variant.appearance {
            Appearance::Light => Self::LIGHT,
            Appearance::Dark => Self::DARK,
        };
        let accented = base.with_accent(variant);
        if variant.is_increased_contrast() {
            accented.with_increased_contrast(variant.appearance)
        } else {
            accented
        }
    }

    const LIGHT: Self = Self {
        window_background: grey(0.929),
        content_background: grey(1.0),
        alternate_row: rgb(0.929, 0.953, 0.996),
        label: black(0.85),
        secondary_label: black(0.50),
        tertiary_label: black(0.26),
        disabled_label: black(0.25),
        label_emboss: white(0.55),
        separator: black(0.10),
        accent: rgb(0.20, 0.47, 0.88),
        selection: rgb(0.22, 0.46, 0.84),
        unfocused_selection: grey(0.83),
        selected_label: grey(1.0),
        text_selection: rgb(0.70, 0.84, 1.0),
        focus_ring: rgba(0.30, 0.56, 0.96, 0.75),
        shadow: black(0.35),
    };

    const DARK: Self = Self {
        window_background: grey(0.196),
        content_background: grey(0.118),
        alternate_row: white(0.05),
        label: white(0.85),
        secondary_label: white(0.55),
        tertiary_label: white(0.25),
        disabled_label: white(0.25),
        label_emboss: black(0.55),
        separator: white(0.10),
        accent: rgb(0.30, 0.56, 0.96),
        selection: rgb(0.16, 0.38, 0.75),
        unfocused_selection: grey(0.30),
        selected_label: grey(1.0),
        text_selection: rgb(0.20, 0.34, 0.56),
        focus_ring: rgba(0.36, 0.60, 1.0, 0.75),
        shadow: black(0.60),
    };

    fn with_accent(self, variant: Variant) -> Self {
        match (variant.accent, variant.appearance) {
            (Accent::Blue, _) => self,
            (Accent::Graphite, Appearance::Light) => Self {
                accent: rgb(0.45, 0.50, 0.57),
                selection: rgb(0.47, 0.52, 0.59),
                text_selection: rgb(0.80, 0.82, 0.86),
                focus_ring: rgba(0.50, 0.56, 0.65, 0.75),
                alternate_row: rgb(0.941, 0.945, 0.953),
                ..self
            },
            (Accent::Graphite, Appearance::Dark) => Self {
                accent: rgb(0.58, 0.62, 0.69),
                selection: rgb(0.36, 0.40, 0.46),
                text_selection: rgb(0.30, 0.33, 0.38),
                focus_ring: rgba(0.60, 0.65, 0.73, 0.75),
                ..self
            },
        }
    }

    fn with_increased_contrast(self, appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self {
                label: black(1.0),
                secondary_label: black(0.70),
                tertiary_label: black(0.50),
                disabled_label: black(0.40),
                separator: black(0.45),
                ..self
            },
            Appearance::Dark => Self {
                label: white(1.0),
                secondary_label: white(0.75),
                tertiary_label: white(0.55),
                disabled_label: white(0.45),
                separator: white(0.45),
                ..self
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Contrast;

    #[test]
    fn variants_differ_where_they_should() {
        let light = SemanticColors::new(Variant::new(Appearance::Light));
        let dark = SemanticColors::new(Variant::new(Appearance::Dark));
        assert_ne!(light.window_background, dark.window_background);

        let graphite =
            SemanticColors::new(Variant::new(Appearance::Light).with_accent(Accent::Graphite));
        assert_ne!(light.selection, graphite.selection);
        assert_eq!(light.label, graphite.label);

        let contrast =
            SemanticColors::new(Variant::new(Appearance::Light).with_contrast(Contrast::Increased));
        assert!(contrast.separator.components[3] > light.separator.components[3]);
        assert_eq!(contrast.selection, light.selection);
    }
}
