//! The materials table: colours and metrics each neue-paint material is drawn from.
//!
//! One module per material. Colours are functions of the [`Variant`] (and the material's
//! own state); metrics are constants. Blue accent at standard contrast is the reference
//! look the goldens were drawn with; graphite and increased contrast derive from it.

/// Darkening applied to rims under increased contrast.
const CONTRAST_RIM: f32 = 0.6;

/// What a translucent material sits on.
///
/// The menu bar, the dock shelf and HUD panels are translucent. Their colours are defined
/// with alpha, for the compositor to draw over a blurred copy of what is behind them
/// (Phase 3: the backdrop-blur protocol, ADR 0002 §7). Until a surface has a blur behind
/// it, [`Backdrop::Opaque`] draws the same tint flattened over a nominal backdrop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Backdrop {
    /// Nothing is blurred behind the material: draw the tint flattened onto a nominal
    /// backdrop, so it is fully opaque. The default, and the fallback.
    #[default]
    Opaque,
    /// The compositor blurs the content behind the material: draw the translucent tint.
    Translucent,
}

impl Backdrop {
    /// What to draw for `variant`: increased contrast reduces transparency, so it is
    /// always opaque.
    pub fn for_variant(self, variant: crate::Variant) -> Self {
        if variant.is_increased_contrast() {
            Self::Opaque
        } else {
            self
        }
    }
}

/// `color` flattened onto the opaque grey `base`: the colour it shows over a backdrop of
/// that grey.
fn flatten(color: crate::Color, base: f32) -> crate::Color {
    let a = color.components[3];
    let [r, g, b, _] = color.components;
    crate::rgb(
        base * (1.0 - a) + r * a,
        base * (1.0 - a) + g * a,
        base * (1.0 - a) + b * a,
    )
}

/// `colors` as drawn on `backdrop`: unchanged when translucent, flattened onto the grey
/// `base` when opaque.
fn on_backdrop<const N: usize>(
    colors: [crate::Color; N],
    backdrop: Backdrop,
    base: f32,
) -> [crate::Color; N] {
    match backdrop {
        Backdrop::Translucent => colors,
        Backdrop::Opaque => colors.map(|c| flatten(c, base)),
    }
}

/// The Aqua gel push button.
pub mod aqua_button {
    use super::CONTRAST_RIM;
    use crate::{
        Accent, Appearance, Color, Variant, black, darken, grey, metrics, mix, radii, rgb, white,
    };

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
    /// How far the default button's gel moves toward white at the top of its pulse
    /// (0: not at all, 1: white).
    pub const PULSE_LIGHTEN: f32 = 0.3;
    /// Opacity of a disabled push button.
    pub const DISABLED_OPACITY: f32 = 0.5;

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
            p.with_contrast(variant)
        }

        /// A push button that isn't the default button, at rest: neutral gel. Held down,
        /// every push button takes the accent ([`Palette::new`] with `pressed`).
        pub fn plain(variant: Variant) -> Self {
            let p = match variant.appearance {
                Appearance::Light => Self {
                    shadow: black(0.28),
                    etch: white(0.55),
                    rim: [grey(0.60), grey(0.46)],
                    gel: [grey(1.0), grey(0.97), grey(0.91), grey(0.98)],
                    gloss: [white(0.60), white(0.10)],
                },
                Appearance::Dark => Self {
                    shadow: black(0.60),
                    etch: white(0.07),
                    rim: [grey(0.10), grey(0.05)],
                    gel: [grey(0.42), grey(0.36), grey(0.29), grey(0.34)],
                    gloss: [white(0.25), white(0.04)],
                },
            };
            p.with_contrast(variant)
        }

        /// This palette at `amount` (0 at rest, 1 at the peak) of the default button's
        /// pulse: the gel brightens toward white by up to [`PULSE_LIGHTEN`].
        #[must_use]
        pub fn pulsed(self, amount: f32) -> Self {
            let t = amount.clamp(0.0, 1.0) * PULSE_LIGHTEN;
            Self {
                gel: self.gel.map(|c| mix(c, white(1.0), t)),
                ..self
            }
        }

        /// The colour `t` of the way from this palette to `other` (0: this, 1: `other`):
        /// checkboxes, radio buttons and segments fade from neutral to accent gel.
        #[must_use]
        pub fn mix_with(self, other: Self, t: f32) -> Self {
            let t = t.clamp(0.0, 1.0);
            let m = |a: Color, b: Color| mix(a, b, t);
            Self {
                shadow: m(self.shadow, other.shadow),
                etch: m(self.etch, other.etch),
                rim: [m(self.rim[0], other.rim[0]), m(self.rim[1], other.rim[1])],
                gel: std::array::from_fn(|i| m(self.gel[i], other.gel[i])),
                gloss: [
                    m(self.gloss[0], other.gloss[0]),
                    m(self.gloss[1], other.gloss[1]),
                ],
            }
        }

        /// The gel darkened by `factor` (1: unchanged): a neutral control held down.
        #[must_use]
        pub fn dimmed(self, factor: f32) -> Self {
            Self {
                gel: self.gel.map(|c| darken(c, factor)),
                gloss: self.gloss.map(|c| mix(c, black(c.components[3]), 0.5)),
                ..self
            }
        }

        fn with_contrast(self, variant: Variant) -> Self {
            if variant.is_increased_contrast() {
                Self {
                    rim: self.rim.map(|c| darken(c, CONTRAST_RIM)),
                    ..self
                }
            } else {
                self
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn pulse_brightens_only_the_gel() {
            let rest = Palette::new(Variant::default(), false);
            assert_eq!(rest.pulsed(0.0), rest);
            let peak = rest.pulsed(1.0);
            assert_eq!(peak.rim, rest.rim);
            for (p, r) in peak.gel.iter().zip(rest.gel) {
                assert!(p.components[2] >= r.components[2]);
                assert!(p.components[0] > r.components[0]);
            }
            assert_eq!(rest.pulsed(5.0), peak, "amount is clamped");
        }

        #[test]
        fn increased_contrast_darkens_plain_rims() {
            let v = Variant::default();
            let standard = Palette::plain(v);
            let increased = Palette::plain(v.with_contrast(crate::Contrast::Increased));
            assert!(increased.rim[0].components[0] < standard.rim[0].components[0]);
            assert_eq!(increased.gel, standard.gel);
        }
    }
}

/// Bevel and recessed buttons: flat, square-ish buttons for toolbars and inspectors.
pub mod bevel_button {
    use super::CONTRAST_RIM;
    use crate::{Appearance, Color, Variant, black, darken, grey, metrics, mix, radii, white};

    /// Corner radius.
    pub const RADIUS: f64 = radii::CONTROL;
    /// Rim thickness.
    pub const RIM: f64 = metrics::control::RIM;
    /// How far the etched highlight sits below the button.
    pub const ETCH_OFFSET: f64 = 1.0;
    /// How far the drop shadow sits below the button.
    pub const SHADOW_OFFSET: f64 = 0.5;
    /// Blur radius (standard deviation) of the drop shadow.
    pub const SHADOW_BLUR: f64 = 0.75;
    /// Height of the inner shadow along the top of a pressed or on button.
    pub const INNER_SHADOW: f64 = 3.0;
    /// Opacity of a disabled button.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// Colours of a bevel button, between rest and pressed.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Drop shadow.
        pub shadow: Color,
        /// Etched highlight below the button.
        pub etch: Color,
        /// Rim gradient, top and bottom.
        pub rim: [Color; 2],
        /// Body gradient, top and bottom.
        pub body: [Color; 2],
        /// Inner shadow along the top edge (fully transparent at rest).
        pub inner_shadow: Color,
    }

    impl Palette {
        /// Colours for `variant`, `depth` of the way from rest (0) to pressed in (1).
        pub fn new(variant: Variant, depth: f32) -> Self {
            let (rest, held) = match variant.appearance {
                Appearance::Light => (
                    Self {
                        shadow: black(0.25),
                        etch: white(0.55),
                        rim: [grey(0.62), grey(0.46)],
                        body: [grey(0.99), grey(0.88)],
                        inner_shadow: black(0.0),
                    },
                    Self {
                        shadow: black(0.12),
                        etch: white(0.55),
                        rim: [grey(0.46), grey(0.40)],
                        body: [grey(0.74), grey(0.84)],
                        inner_shadow: black(0.28),
                    },
                ),
                Appearance::Dark => (
                    Self {
                        shadow: black(0.60),
                        etch: white(0.07),
                        rim: [grey(0.10), grey(0.05)],
                        body: [grey(0.45), grey(0.31)],
                        inner_shadow: black(0.0),
                    },
                    Self {
                        shadow: black(0.40),
                        etch: white(0.07),
                        rim: [grey(0.05), grey(0.03)],
                        body: [grey(0.22), grey(0.30)],
                        inner_shadow: black(0.45),
                    },
                ),
            };
            let t = depth.clamp(0.0, 1.0);
            let p = Self {
                shadow: mix(rest.shadow, held.shadow, t),
                etch: mix(rest.etch, held.etch, t),
                rim: [
                    mix(rest.rim[0], held.rim[0], t),
                    mix(rest.rim[1], held.rim[1], t),
                ],
                body: [
                    mix(rest.body[0], held.body[0], t),
                    mix(rest.body[1], held.body[1], t),
                ],
                inner_shadow: mix(rest.inner_shadow, held.inner_shadow, t),
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

    /// The recessed button: no chrome at rest, a dark pill with an inner shadow when on
    /// (or held down).
    pub mod recessed {
        use crate::{Appearance, Color, Variant, black, grey, metrics, mix, white};

        /// Corner radius of a regular-height pill (half its height); shorter ones use half
        /// their height.
        pub const RADIUS: f64 = metrics::control::REGULAR_HEIGHT / 2.0;
        /// Rim thickness.
        pub const RIM: f64 = metrics::control::RIM;
        /// Height of the inner shadow along the top of the pill.
        pub const INNER_SHADOW: f64 = 3.0;
        /// Opacity of a disabled button.
        pub const DISABLED_OPACITY: f32 = 0.5;

        /// Colours of the on pill, scaled by how far on it is.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Palette {
            /// Highlight under the pill.
            pub etch: Color,
            /// Rim gradient, top and bottom.
            pub rim: [Color; 2],
            /// Body gradient, top and bottom.
            pub body: [Color; 2],
            /// Inner shadow along the top edge.
            pub inner_shadow: Color,
            /// The label while on.
            pub label_on: Color,
        }

        impl Palette {
            /// The pill for `variant`, `amount` (0: invisible, 1: fully on).
            pub fn new(variant: Variant, amount: f32) -> Self {
                let full = match variant.appearance {
                    Appearance::Light => Self {
                        etch: white(0.55),
                        rim: [grey(0.30), grey(0.42)],
                        body: [grey(0.46), grey(0.58)],
                        inner_shadow: black(0.40),
                        label_on: white(0.95),
                    },
                    Appearance::Dark => Self {
                        etch: white(0.08),
                        rim: [grey(0.04), grey(0.10)],
                        body: [grey(0.13), grey(0.20)],
                        inner_shadow: black(0.55),
                        label_on: white(0.95),
                    },
                };
                let a = amount.clamp(0.0, 1.0);
                let fade = |c: Color| {
                    let [r, g, b, alpha] = c.components;
                    Color::new([r, g, b, alpha * a])
                };
                Self {
                    etch: fade(full.etch),
                    rim: full.rim.map(fade),
                    body: full.body.map(fade),
                    inner_shadow: fade(full.inner_shadow),
                    label_on: full.label_on,
                }
            }

            /// The label colour: `rest` fades to [`Self::label_on`] as `amount` goes 0 to 1.
            pub fn label(&self, rest: Color, amount: f32) -> Color {
                mix(rest, self.label_on, amount.clamp(0.0, 1.0))
            }
        }
    }
}

/// Checkboxes and radio buttons: a small gel box or disc, accent gel when on, with a
/// white mark.
pub mod toggle {
    use crate::{Color, black, metrics, radii, white};

    /// Side of the box (checkbox) or diameter of the disc (radio button).
    pub const SIZE: f64 = metrics::control::CHECKBOX_SIZE;
    /// Corner radius of a checkbox.
    pub const RADIUS: f64 = radii::CONTROL_SMALL;
    /// Stroke width of the check mark and the dash.
    pub const MARK_WIDTH: f64 = 2.0;
    /// Diameter of a radio button's dot, as a fraction of [`SIZE`].
    pub const DOT: f64 = 0.4;
    /// How far the mark's shadow sits below it.
    pub const MARK_SHADOW_OFFSET: f64 = 1.0;
    /// How much a neutral box darkens while held down (1: not at all).
    pub const PRESSED_DIM: f32 = 0.86;
    /// Opacity of a disabled checkbox or radio button.
    pub const DISABLED_OPACITY: f32 = 0.5;
    /// The check mark, dash and dot.
    pub const MARK: Color = white(1.0);
    /// The mark's shadow.
    pub const MARK_SHADOW: Color = black(0.45);
}

/// The segmented control: a row of segments in one rounded body; the selected one is
/// accent gel.
pub mod segmented {
    use crate::{Color, Variant, black, white};

    /// Corner radius of the whole control.
    pub const RADIUS: f64 = crate::radii::CONTROL;
    /// Width of the divider between segments.
    pub const DIVIDER: f64 = 1.0;
    /// Opacity of a disabled control.
    pub const DISABLED_OPACITY: f32 = 0.5;
    /// How much a neutral segment darkens while held down (1: not at all).
    pub const PRESSED_DIM: f32 = 0.86;

    /// The divider between two unselected segments.
    pub fn divider(variant: Variant) -> Color {
        let c = match variant.appearance {
            crate::Appearance::Light => black(0.30),
            crate::Appearance::Dark => black(0.55),
        };
        if variant.is_increased_contrast() {
            black(c.components[3] + 0.2)
        } else {
            c
        }
    }

    /// The label on a selected segment (on accent gel).
    pub const SELECTED_LABEL: Color = white(1.0);
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

/// The menu bar: a light (or dark) gradient strip across the top of the screen.
pub mod menu_bar {
    use super::{Backdrop, on_backdrop};
    use crate::{Appearance, Color, Variant, black, rgba, white};

    /// Height of the bar.
    pub const HEIGHT: f64 = 22.0;
    /// Thickness of the highlight along the top edge and the line along the bottom.
    pub const EDGE: f64 = 1.0;
    /// Font size of the menu titles.
    pub const FONT_SIZE: f32 = crate::typography::BODY;
    /// Baseline of the labels, from the top of the bar.
    pub const LABEL_BASELINE: f64 = 15.0;
    /// How far the label's embossed shadow sits below the text.
    pub const LABEL_EMBOSS_OFFSET: f64 = 1.0;

    /// Colours of the bar.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Gradient top and bottom.
        pub fill: [Color; 2],
        /// Highlight along the top edge.
        pub highlight: Color,
        /// Line along the bottom edge, between the bar and what is below.
        pub separator: Color,
        /// Menu titles and menu extras.
        pub label: Color,
        /// Embossed shadow below the labels.
        pub label_shadow: Color,
    }

    impl Palette {
        /// Colours for `variant` on `backdrop`. The accent does not tint the bar.
        /// Increased contrast makes the bar opaque with a firmer line and text.
        pub fn new(variant: Variant, backdrop: Backdrop) -> Self {
            let backdrop = backdrop.for_variant(variant);
            let contrast = variant.is_increased_contrast();
            let p = match variant.appearance {
                Appearance::Light => Self {
                    fill: on_backdrop(
                        [rgba(0.97, 0.97, 0.97, 0.80), rgba(0.84, 0.84, 0.85, 0.80)],
                        backdrop,
                        0.62,
                    ),
                    highlight: white(0.60),
                    separator: black(0.45),
                    label: black(0.85),
                    label_shadow: white(0.45),
                },
                Appearance::Dark => Self {
                    fill: on_backdrop(
                        [rgba(0.27, 0.27, 0.29, 0.76), rgba(0.14, 0.14, 0.16, 0.76)],
                        backdrop,
                        0.15,
                    ),
                    highlight: white(0.14),
                    separator: black(0.80),
                    label: white(0.88),
                    label_shadow: black(0.50),
                },
            };
            match (contrast, variant.appearance) {
                (false, _) => p,
                (true, Appearance::Light) => Self {
                    separator: black(0.80),
                    label: black(1.0),
                    ..p
                },
                (true, Appearance::Dark) => Self {
                    separator: black(1.0),
                    label: white(1.0),
                    highlight: white(0.30),
                    ..p
                },
            }
        }
    }
}

/// The dock shelf: translucent glass, seen in perspective or flat.
pub mod dock_shelf {
    use super::{Backdrop, on_backdrop};
    use crate::{Appearance, Color, Variant, black, rgba, white};

    /// Height of the shelf in perspective.
    pub const DEPTH: f64 = 24.0;
    /// How far the far (top) edge of the shelf in perspective is inset from the near edge
    /// at each end.
    pub const PERSPECTIVE_INSET: f64 = 18.0;
    /// Corner radius of the flat shelf.
    pub const FLAT_RADIUS: f64 = 8.0;
    /// Thickness of the edge highlights.
    pub const EDGE: f64 = 1.0;
    /// Share of the shelf's height that the sheen covers, from its far edge.
    pub const SHEEN_HEIGHT: f64 = 0.45;

    /// Colours of the shelf.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Glass body, far edge and near edge.
        pub fill: [Color; 2],
        /// Sheen over the far part of the glass, far edge and where it ends.
        pub sheen: [Color; 2],
        /// Highlight along the far edge (the top of the glass).
        pub top_edge: Color,
        /// Highlight along the slanted (or rounded) sides.
        pub side_edge: Color,
        /// Line along the near edge, where the glass meets the screen.
        pub bottom_edge: Color,
    }

    impl Palette {
        /// Colours for `variant` on `backdrop`. The accent does not tint the glass.
        /// Increased contrast makes it opaque with firmer edges.
        pub fn new(variant: Variant, backdrop: Backdrop) -> Self {
            let backdrop = backdrop.for_variant(variant);
            let contrast = variant.is_increased_contrast();
            let edge_boost = if contrast { 0.35 } else { 0.0 };
            match variant.appearance {
                Appearance::Light => Self {
                    fill: on_backdrop(
                        [rgba(1.0, 1.0, 1.0, 0.20), rgba(1.0, 1.0, 1.0, 0.52)],
                        backdrop,
                        0.55,
                    ),
                    sheen: [white(0.38), white(0.04)],
                    top_edge: white(0.70 + edge_boost),
                    side_edge: white(0.45 + edge_boost),
                    bottom_edge: black(0.22 + edge_boost),
                },
                Appearance::Dark => Self {
                    fill: on_backdrop(
                        [rgba(1.0, 1.0, 1.0, 0.07), rgba(1.0, 1.0, 1.0, 0.20)],
                        backdrop,
                        0.14,
                    ),
                    sheen: [white(0.20), white(0.02)],
                    top_edge: white(0.32 + edge_boost),
                    side_edge: white(0.20 + edge_boost),
                    bottom_edge: black(0.60 + edge_boost),
                },
            }
        }
    }
}

/// The dark HUD panel: volume and brightness indicators, HUD windows.
pub mod hud {
    use super::{Backdrop, on_backdrop};
    use crate::{Appearance, Color, Variant, black, radii, rgba, white};

    /// Corner radius.
    pub const RADIUS: f64 = radii::PANEL;
    /// Rim thickness.
    pub const RIM: f64 = 1.0;
    /// How far the drop shadow sits below the panel.
    pub const SHADOW_OFFSET: f64 = 3.0;
    /// Blur radius (standard deviation) of the drop shadow.
    pub const SHADOW_BLUR: f64 = 5.0;
    /// How far the shadow reaches beyond the panel's sides and top, for callers that
    /// leave room for it.
    pub const SHADOW_REACH: f64 = 3.0 * SHADOW_BLUR;
    /// Share of the panel's height that the gloss covers, from the top.
    pub const GLOSS_HEIGHT: f64 = 0.45;

    /// Colours of the panel.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Drop shadow.
        pub shadow: Color,
        /// Body, top and bottom.
        pub fill: [Color; 2],
        /// Gloss over the upper part of the body, top and bottom.
        pub gloss: [Color; 2],
        /// Rim, inside the body's edge.
        pub rim: Color,
        /// Text and glyphs.
        pub label: Color,
        /// Secondary text.
        pub secondary_label: Color,
    }

    impl Palette {
        /// Colours for `variant` on `backdrop`. The HUD is dark in both appearances: in
        /// the dark one it is deeper and has a brighter rim and shadow, so it stays
        /// separate from dark windows. The accent does not tint it. Increased contrast
        /// makes it opaque with a bright rim.
        pub fn new(variant: Variant, backdrop: Backdrop) -> Self {
            let backdrop = backdrop.for_variant(variant);
            let contrast = variant.is_increased_contrast();
            let p = match variant.appearance {
                Appearance::Light => Self {
                    shadow: black(0.45),
                    fill: on_backdrop(
                        [rgba(0.20, 0.20, 0.21, 0.80), rgba(0.06, 0.06, 0.07, 0.82)],
                        backdrop,
                        0.50,
                    ),
                    gloss: [white(0.24), white(0.03)],
                    rim: white(0.28),
                    label: white(0.95),
                    secondary_label: white(0.60),
                },
                Appearance::Dark => Self {
                    shadow: black(0.75),
                    fill: on_backdrop(
                        [rgba(0.10, 0.10, 0.11, 0.86), rgba(0.02, 0.02, 0.03, 0.88)],
                        backdrop,
                        0.15,
                    ),
                    gloss: [white(0.14), white(0.02)],
                    rim: white(0.36),
                    label: white(0.92),
                    secondary_label: white(0.55),
                },
            };
            if contrast {
                Self {
                    rim: white(0.70),
                    label: white(1.0),
                    secondary_label: white(0.80),
                    ..p
                }
            } else {
                p
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
    fn translucent_materials_have_distinct_dark_variants() {
        for light in variants() {
            let dark = Variant {
                appearance: Appearance::Dark,
                ..light
            };
            for backdrop in [Backdrop::Opaque, Backdrop::Translucent] {
                assert_ne!(
                    menu_bar::Palette::new(light, backdrop),
                    menu_bar::Palette::new(dark, backdrop)
                );
                assert_ne!(
                    dock_shelf::Palette::new(light, backdrop),
                    dock_shelf::Palette::new(dark, backdrop)
                );
                assert_ne!(
                    hud::Palette::new(light, backdrop),
                    hud::Palette::new(dark, backdrop)
                );
            }
        }
    }

    #[test]
    fn opaque_fills_are_opaque_and_translucent_ones_are_not() {
        let alphas = |v: Variant, backdrop: Backdrop| {
            [
                menu_bar::Palette::new(v, backdrop).fill.to_vec(),
                dock_shelf::Palette::new(v, backdrop).fill.to_vec(),
                hud::Palette::new(v, backdrop).fill.to_vec(),
            ]
            .concat()
            .into_iter()
            .map(|c| c.components[3])
            .collect::<Vec<_>>()
        };
        for appearance in [Appearance::Light, Appearance::Dark] {
            let v = Variant::new(appearance);
            assert!(alphas(v, Backdrop::Opaque).iter().all(|&a| a == 1.0));
            assert!(alphas(v, Backdrop::Translucent).iter().all(|&a| a < 1.0));
            // Increased contrast reduces transparency.
            let strong = v.with_contrast(Contrast::Increased);
            assert!(
                alphas(strong, Backdrop::Translucent)
                    .iter()
                    .all(|&a| a == 1.0)
            );
        }
    }

    #[test]
    fn flattening_blends_toward_the_base() {
        let c = flatten(crate::rgba(1.0, 1.0, 1.0, 0.5), 0.0);
        assert_eq!(c.components, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(
            flatten(crate::rgb(0.2, 0.3, 0.4), 0.9).components,
            [0.2, 0.3, 0.4, 1.0]
        );
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
