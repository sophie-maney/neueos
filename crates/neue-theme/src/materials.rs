//! The materials table: colours and metrics each neue-paint material is drawn from.
//!
//! One module per material. Colours are functions of the [`Variant`] (and the material's
//! own state); metrics are constants. Blue accent at standard contrast is the reference
//! look the goldens were drawn with; graphite and increased contrast derive from it.

/// Darkening applied to rims under increased contrast.
const CONTRAST_RIM: f32 = 0.6;

/// The Aqua gel push button.
pub mod aqua_button {
    use super::CONTRAST_RIM;
    use crate::{Accent, Appearance, Color, Variant, black, darken, metrics, radii, rgb, white};

    /// Corner radius of the rim.
    pub const RADIUS: f64 = radii::CONTROL;
    /// Rim thickness.
    pub const RIM: f64 = metrics::control::RIM;
    /// How far the etched highlight sits below the button.
    pub const ETCH_OFFSET: f64 = 1.0;
    /// How far the drop shadow sits below the button.
    pub const SHADOW_OFFSET: f64 = 0.5;
    /// Blur radius (standard deviation) of the drop shadow.
    pub const SHADOW_BLUR: f64 = 0.75;
    /// Inset of the gloss from the sides of the body.
    pub const GLOSS_INSET: f64 = 1.0;
    /// Inset of the gloss from the top of the body.
    pub const GLOSS_TOP: f64 = 0.5;

    /// Colours of one variant × state.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Drop shadow.
        pub shadow: Color,
        /// Etched highlight below the button.
        pub etch: Color,
        /// Rim gradient, top and bottom.
        pub rim: [Color; 2],
        /// Gel body: top, above the midline, below the midline, bottom.
        pub gel: [Color; 4],
        /// Gloss gradient, top and bottom.
        pub gloss: [Color; 2],
    }

    impl Palette {
        /// Colours for `variant`, held down if `pressed`.
        pub fn new(variant: Variant, pressed: bool) -> Self {
            let p = match (variant.accent, variant.appearance, pressed) {
                (Accent::Blue, Appearance::Light, false) => Self {
                    shadow: black(0.28),
                    etch: white(0.55),
                    rim: [rgb(0.24, 0.38, 0.66), rgb(0.14, 0.26, 0.54)],
                    gel: [
                        rgb(0.74, 0.87, 0.99),
                        rgb(0.51, 0.72, 0.96),
                        rgb(0.33, 0.58, 0.93),
                        rgb(0.63, 0.83, 1.00),
                    ],
                    gloss: [white(0.55), white(0.08)],
                },
                (Accent::Blue, Appearance::Light, true) => Self {
                    shadow: black(0.20),
                    etch: white(0.55),
                    rim: [rgb(0.16, 0.28, 0.54), rgb(0.09, 0.19, 0.44)],
                    gel: [
                        rgb(0.52, 0.70, 0.92),
                        rgb(0.32, 0.54, 0.87),
                        rgb(0.18, 0.42, 0.81),
                        rgb(0.43, 0.67, 0.94),
                    ],
                    gloss: [white(0.35), white(0.04)],
                },
                (Accent::Blue, Appearance::Dark, false) => Self {
                    shadow: black(0.60),
                    etch: white(0.07),
                    rim: [rgb(0.09, 0.15, 0.31), rgb(0.04, 0.08, 0.20)],
                    gel: [
                        rgb(0.46, 0.65, 0.91),
                        rgb(0.27, 0.49, 0.83),
                        rgb(0.14, 0.35, 0.75),
                        rgb(0.33, 0.57, 0.89),
                    ],
                    gloss: [white(0.38), white(0.05)],
                },
                (Accent::Blue, Appearance::Dark, true) => Self {
                    shadow: black(0.50),
                    etch: white(0.07),
                    rim: [rgb(0.05, 0.10, 0.24), rgb(0.02, 0.05, 0.15)],
                    gel: [
                        rgb(0.33, 0.50, 0.78),
                        rgb(0.18, 0.36, 0.70),
                        rgb(0.09, 0.25, 0.61),
                        rgb(0.23, 0.44, 0.76),
                    ],
                    gloss: [white(0.22), white(0.03)],
                },
                (Accent::Graphite, Appearance::Light, false) => Self {
                    shadow: black(0.28),
                    etch: white(0.55),
                    rim: [rgb(0.38, 0.41, 0.47), rgb(0.27, 0.30, 0.36)],
                    gel: [
                        rgb(0.85, 0.87, 0.90),
                        rgb(0.70, 0.74, 0.79),
                        rgb(0.56, 0.61, 0.67),
                        rgb(0.78, 0.82, 0.87),
                    ],
                    gloss: [white(0.55), white(0.08)],
                },
                (Accent::Graphite, Appearance::Light, true) => Self {
                    shadow: black(0.20),
                    etch: white(0.55),
                    rim: [rgb(0.29, 0.32, 0.38), rgb(0.20, 0.23, 0.29)],
                    gel: [
                        rgb(0.68, 0.71, 0.76),
                        rgb(0.53, 0.57, 0.63),
                        rgb(0.41, 0.46, 0.53),
                        rgb(0.62, 0.67, 0.73),
                    ],
                    gloss: [white(0.35), white(0.04)],
                },
                (Accent::Graphite, Appearance::Dark, false) => Self {
                    shadow: black(0.60),
                    etch: white(0.07),
                    rim: [rgb(0.13, 0.15, 0.19), rgb(0.07, 0.09, 0.12)],
                    gel: [
                        rgb(0.60, 0.64, 0.70),
                        rgb(0.44, 0.48, 0.55),
                        rgb(0.31, 0.35, 0.42),
                        rgb(0.50, 0.55, 0.62),
                    ],
                    gloss: [white(0.38), white(0.05)],
                },
                (Accent::Graphite, Appearance::Dark, true) => Self {
                    shadow: black(0.50),
                    etch: white(0.07),
                    rim: [rgb(0.09, 0.11, 0.14), rgb(0.04, 0.06, 0.09)],
                    gel: [
                        rgb(0.47, 0.51, 0.57),
                        rgb(0.33, 0.37, 0.44),
                        rgb(0.22, 0.26, 0.32),
                        rgb(0.39, 0.44, 0.51),
                    ],
                    gloss: [white(0.22), white(0.03)],
                },
            };
            if variant.is_increased_contrast() {
                Self {
                    rim: p.rim.map(|c| darken(c, CONTRAST_RIM)),
                    ..p
                }
            } else {
                p
            }
        }
    }
}

/// Procedural linen.
pub mod linen {
    use crate::{Appearance, Variant};

    /// Tile size in logical pixels.
    pub const TILE: u16 = 128;

    /// Colours of one linen variant. Raw sRGB components, because the texture generator
    /// works per channel.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Mean cloth colour, sRGB 0..1.
        pub base: [f32; 3],
        /// How far fibres deviate from `base`.
        pub contrast: f32,
    }

    impl Palette {
        /// Linen for `variant`. Only the appearance matters: linen is a backdrop, so
        /// neither accent nor contrast changes it.
        pub fn new(variant: Variant) -> Self {
            match variant.appearance {
                Appearance::Light => Self {
                    base: [0.785, 0.788, 0.780],
                    contrast: 0.11,
                },
                Appearance::Dark => Self {
                    base: [0.224, 0.236, 0.259],
                    contrast: 0.075,
                },
            }
        }
    }
}

/// The window title bar and its traffic-light buttons.
pub mod title_bar {
    use super::CONTRAST_RIM;
    use crate::{Accent, Appearance, Color, Variant, black, darken, grey, rgb, white};
    use crate::{radii, typography};

    /// Height of the title bar.
    pub const HEIGHT: f64 = 22.0;
    /// Width of the left slice: the rounded corner and the traffic lights.
    pub const LEFT_SLICE_WIDTH: f64 = 68.0;
    /// Width of the right slice: the rounded corner.
    pub const RIGHT_SLICE_WIDTH: f64 = 8.0;
    /// Width of the middle slice, stretched to fill the rest of the bar.
    pub const MIDDLE_SLICE_WIDTH: f64 = 1.0;
    /// Font size of the title.
    pub const TITLE_FONT_SIZE: f32 = typography::WINDOW_TITLE;
    /// Baseline of the title, from the top of the bar.
    pub const TITLE_BASELINE: f64 = 15.0;
    /// Space between the title and the buttons or the right end of the bar.
    pub const TITLE_MARGIN: f64 = 8.0;
    /// How far the title's embossed shadow sits below (and may reach right of) the text.
    pub const TITLE_EMBOSS_OFFSET: f64 = 1.0;
    /// Radius of the top corners.
    pub const CORNER_RADIUS: f64 = radii::WINDOW;
    /// Thickness of the highlight along the top edge and the separator along the bottom.
    pub const EDGE: f64 = 1.0;
    /// Diameter of a traffic-light button.
    pub const BUTTON_DIAMETER: f64 = 12.0;
    /// Rim thickness of a traffic-light button.
    pub const BUTTON_RIM: f64 = 1.0;
    /// Left edge of the first button.
    pub const BUTTON_X: f64 = 8.0;
    /// Distance between the left edges of neighbouring buttons.
    pub const BUTTON_PITCH: f64 = 20.0;
    /// How far a button's etched highlight sits below it.
    pub const BUTTON_ETCH_OFFSET: f64 = 0.75;
    /// Half the length of a glyph stroke, from its centre.
    pub const GLYPH_REACH: f64 = 2.6;
    /// Line width of the glyphs.
    pub const GLYPH_WIDTH: f64 = 1.5;

    /// Colours of the bar.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BarPalette {
        /// Gradient top and bottom.
        pub fill: [Color; 2],
        /// Highlight along the top edge.
        pub highlight: Color,
        /// Line along the bottom edge, between the bar and the window content.
        pub separator: Color,
        /// Title text.
        pub text: Color,
        /// Embossed shadow below the text.
        pub text_shadow: Color,
    }

    impl BarPalette {
        /// Colours for `variant`, in the focused (key) window if `focused`. The accent does
        /// not tint the bar.
        pub fn new(variant: Variant, focused: bool) -> Self {
            let contrast = variant.is_increased_contrast();
            let p = match (variant.appearance, focused) {
                (Appearance::Light, true) => Self {
                    fill: [grey(0.910), grey(0.780)],
                    highlight: white(0.75),
                    separator: grey(0.400),
                    text: grey(0.165),
                    text_shadow: white(0.55),
                },
                (Appearance::Light, false) => Self {
                    fill: [grey(0.960), grey(0.905)],
                    highlight: white(0.85),
                    separator: grey(0.640),
                    text: grey(0.520),
                    text_shadow: white(0.50),
                },
                (Appearance::Dark, true) => Self {
                    fill: [grey(0.300), grey(0.205)],
                    highlight: white(0.16),
                    separator: grey(0.060),
                    text: grey(0.880),
                    text_shadow: black(0.55),
                },
                (Appearance::Dark, false) => Self {
                    fill: [grey(0.235), grey(0.195)],
                    highlight: white(0.09),
                    separator: grey(0.090),
                    text: grey(0.520),
                    text_shadow: black(0.40),
                },
            };
            if !contrast {
                return p;
            }
            match (variant.appearance, focused) {
                (Appearance::Light, true) => Self {
                    separator: grey(0.15),
                    text: grey(0.0),
                    ..p
                },
                (Appearance::Light, false) => Self {
                    separator: grey(0.35),
                    text: grey(0.30),
                    ..p
                },
                (Appearance::Dark, true) => Self {
                    separator: grey(0.0),
                    text: grey(1.0),
                    highlight: white(0.30),
                    ..p
                },
                (Appearance::Dark, false) => Self {
                    separator: grey(0.0),
                    text: grey(0.72),
                    highlight: white(0.20),
                    ..p
                },
            }
        }
    }

    /// Which traffic light, for colouring.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum Light {
        /// Close (red).
        Close,
        /// Minimise (yellow).
        Minimize,
        /// Zoom (green).
        Zoom,
    }

    /// State of a traffic light, for colouring.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct LightState {
        /// The window is the focused (key) window.
        pub focused: bool,
        /// The pointer is over the traffic lights.
        pub hover: bool,
        /// This button is held down.
        pub pressed: bool,
    }

    /// Colours of one traffic-light button.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct LightPalette {
        /// Rim gradient, top and bottom. The bottom also colours the glyph.
        pub rim: [Color; 2],
        /// Body glow: centre (low in the button) and edge.
        pub body: [Color; 2],
        /// Gloss gradient, top and bottom.
        pub gloss: [Color; 2],
        /// Etched highlight below the button.
        pub etch: Color,
    }

    impl LightPalette {
        /// Colours of `light` in `state` for `variant`. Under the graphite accent all three
        /// buttons share a blue-grey gel.
        pub fn new(light: Light, state: LightState, variant: Variant) -> Self {
            let p = Self::unadjusted(light, state, variant);
            if variant.is_increased_contrast() {
                Self {
                    rim: p.rim.map(|c| darken(c, CONTRAST_RIM)),
                    ..p
                }
            } else {
                p
            }
        }

        fn unadjusted(light: Light, state: LightState, variant: Variant) -> Self {
            let dark = variant.is_dark();
            let etch = if dark { white(0.06) } else { white(0.60) };
            // Hovering the buttons lights them even in an unfocused window.
            if !(state.focused || state.hover) {
                return if dark {
                    Self {
                        rim: [grey(0.16), grey(0.12)],
                        body: [grey(0.42), grey(0.30)],
                        gloss: [white(0.22), white(0.02)],
                        etch,
                    }
                } else {
                    Self {
                        rim: [grey(0.66), grey(0.58)],
                        body: [grey(0.90), grey(0.80)],
                        gloss: [white(0.70), white(0.10)],
                        etch,
                    }
                };
            }
            let (rim, body) = match (variant.accent, light) {
                (Accent::Graphite, _) => (
                    [rgb(0.36, 0.39, 0.44), rgb(0.26, 0.29, 0.34)],
                    [rgb(0.80, 0.83, 0.88), rgb(0.52, 0.56, 0.63)],
                ),
                (Accent::Blue, Light::Close) => (
                    [rgb(0.62, 0.13, 0.10), rgb(0.48, 0.07, 0.05)],
                    [rgb(1.00, 0.52, 0.45), rgb(0.88, 0.22, 0.17)],
                ),
                (Accent::Blue, Light::Minimize) => (
                    [rgb(0.66, 0.44, 0.04), rgb(0.52, 0.32, 0.02)],
                    [rgb(1.00, 0.88, 0.45), rgb(0.93, 0.64, 0.10)],
                ),
                (Accent::Blue, Light::Zoom) => (
                    [rgb(0.10, 0.45, 0.10), rgb(0.05, 0.34, 0.06)],
                    [rgb(0.62, 0.92, 0.45), rgb(0.27, 0.70, 0.18)],
                ),
            };
            let (rim, body) = if dark {
                (rim.map(|c| darken(c, 0.75)), body.map(|c| darken(c, 0.85)))
            } else {
                (rim, body)
            };
            let gloss: [f32; 2] = if dark { [0.45, 0.04] } else { [0.70, 0.06] };
            if state.pressed {
                // Darker and flatter while held down.
                Self {
                    rim: rim.map(|c| darken(c, 0.85)),
                    body: body.map(|c| darken(c, 0.72)),
                    gloss: gloss.map(|a| white(a * 0.4)),
                    etch,
                }
            } else {
                Self {
                    rim,
                    body,
                    gloss: gloss.map(white),
                    etch,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::title_bar::{BarPalette, Light, LightPalette, LightState};
    use super::*;
    use crate::{Accent, Appearance, Contrast, Variant, darken};

    fn variants() -> impl Iterator<Item = Variant> {
        [Accent::Blue, Accent::Graphite]
            .into_iter()
            .flat_map(|accent| {
                [Contrast::Standard, Contrast::Increased]
                    .into_iter()
                    .map(move |contrast| Variant {
                        appearance: Appearance::Light,
                        accent,
                        contrast,
                    })
            })
    }

    #[test]
    fn every_material_has_a_distinct_dark_variant() {
        for light in variants() {
            let dark = Variant {
                appearance: Appearance::Dark,
                ..light
            };
            for pressed in [false, true] {
                assert_ne!(
                    aqua_button::Palette::new(light, pressed),
                    aqua_button::Palette::new(dark, pressed)
                );
            }
            assert_ne!(linen::Palette::new(light), linen::Palette::new(dark));
            for focused in [false, true] {
                assert_ne!(
                    BarPalette::new(light, focused),
                    BarPalette::new(dark, focused)
                );
            }
            let state = LightState {
                focused: true,
                ..LightState::default()
            };
            assert_ne!(
                LightPalette::new(Light::Close, state, light),
                LightPalette::new(Light::Close, state, dark)
            );
        }
    }

    #[test]
    fn graphite_retints_the_gel_and_contrast_darkens_rims() {
        let blue = Variant::new(Appearance::Light);
        let graphite = blue.with_accent(Accent::Graphite);
        let contrast = blue.with_contrast(Contrast::Increased);
        let base = aqua_button::Palette::new(blue, false);
        assert_ne!(base.gel, aqua_button::Palette::new(graphite, false).gel);
        let strong = aqua_button::Palette::new(contrast, false);
        assert_eq!(strong.gel, base.gel);
        assert!(strong.rim[0].components[0] < base.rim[0].components[0]);
    }

    #[test]
    fn darken_matches_material_maths() {
        // Materials used `Color::new([r * f, g * f, b * f, a])`; keep it bit-exact.
        let c = crate::rgb(0.62, 0.13, 0.10);
        assert_eq!(
            darken(c, 0.75).components,
            [0.62 * 0.75, 0.13 * 0.75, 0.10 * 0.75, 1.0]
        );
    }
}
