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

/// Items of a toolbar: icon-and-label buttons that show a pill only while pressed, and
/// separators.
pub mod toolbar {
    use crate::{Appearance, Color, Variant, black, radii, white};

    /// Side of an item's icon.
    pub const ICON_SIZE: f64 = 24.0;
    /// Space between a button's edge and its content, left and right.
    pub const PADDING_X: f64 = 8.0;
    /// Space between a button's edge and its content, top and bottom.
    pub const PADDING_Y: f64 = 4.0;
    /// Space between a button's icon and its label.
    pub const LABEL_GAP: f64 = 2.0;
    /// Space between neighbouring items.
    pub const ITEM_SPACING: f64 = 4.0;
    /// Width of a fixed space.
    pub const SPACE_WIDTH: f64 = 16.0;
    /// Height of a separator's line.
    pub const SEPARATOR_HEIGHT: f64 = 24.0;
    /// Width of a separator: its line and the highlight beside it.
    pub const SEPARATOR_WIDTH: f64 = 2.0;
    /// Corner radius of a pressed button's pill.
    pub const RADIUS: f64 = radii::CONTROL;
    /// Opacity of a disabled button.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// Colours of toolbar items.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// The pill behind a button while it is held down.
        pub pressed: Color,
        /// A separator's line.
        pub separator: Color,
        /// The highlight beside a separator.
        pub separator_highlight: Color,
    }

    impl Palette {
        /// Colours for `variant`.
        pub fn new(variant: Variant) -> Self {
            let more = if variant.is_increased_contrast() {
                1.4
            } else {
                1.0
            };
            match variant.appearance {
                Appearance::Light => Self {
                    pressed: black(0.22 * more),
                    separator: black(0.28 * more),
                    separator_highlight: white(0.5),
                },
                Appearance::Dark => Self {
                    pressed: black(0.35 * more),
                    separator: black(0.5 * more),
                    separator_highlight: white(0.10),
                },
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn increased_contrast_darkens_the_pill() {
            let standard = Palette::new(Variant::new(Appearance::Light));
            let increased = Palette::new(
                Variant::new(Appearance::Light).with_contrast(crate::Contrast::Increased),
            );
            assert!(increased.pressed.components[3] > standard.pressed.components[3]);
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

/// The path control (standard style): a row of text segments separated by chevrons. A
/// segment under the pointer gets a soft rounded fill.
pub mod path_control {
    use crate::{Appearance, Color, Variant, black, white};

    /// Corner radius of a segment's hover and pressed fill.
    pub const RADIUS: f64 = crate::radii::CONTROL - 1.0;
    /// Horizontal padding between a segment's fill and its text.
    pub const SEGMENT_PADDING: f64 = 6.0;
    /// Width of the slot between two segments that holds the chevron.
    pub const CHEVRON_SLOT: f64 = 12.0;
    /// Width of the chevron.
    pub const CHEVRON_WIDTH: f64 = 4.0;
    /// Height of the chevron.
    pub const CHEVRON_HEIGHT: f64 = 8.0;
    /// Stroke width of the chevron.
    pub const CHEVRON_STROKE: f64 = 1.5;
    /// Vertical space between a segment's fill and the control's top and bottom edges.
    pub const SEGMENT_INSET: f64 = 2.0;
    /// Opacity of a disabled control.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// The chevron between two segments.
    pub fn chevron(variant: Variant) -> Color {
        let c = match variant.appearance {
            Appearance::Light => black(0.45),
            Appearance::Dark => white(0.55),
        };
        if variant.is_increased_contrast() {
            match variant.appearance {
                Appearance::Light => black(0.7),
                Appearance::Dark => white(0.8),
            }
        } else {
            c
        }
    }

    /// The fill of the segment under the pointer.
    pub fn hover_fill(variant: Variant) -> Color {
        match variant.appearance {
            Appearance::Light => black(0.08),
            Appearance::Dark => white(0.12),
        }
    }

    /// The fill of the segment held down.
    pub fn pressed_fill(variant: Variant) -> Color {
        match variant.appearance {
            Appearance::Light => black(0.18),
            Appearance::Dark => white(0.24),
        }
    }
}

/// The date picker: the textual style (a field of day, month and year segments with a
/// stepper beside it) and the graphical calendar (a month grid with its header).
pub mod date_picker {
    use crate::colors::SemanticColors;
    use crate::{Appearance, Color, Variant, black, white};

    /// Horizontal padding between the field's edge and its first segment, and its last
    /// segment and the edge.
    pub const FIELD_PADDING: f64 = 3.0;
    /// Horizontal padding between a segment's highlight and its text.
    pub const SEGMENT_PADDING: f64 = 2.0;
    /// Vertical space between a segment's highlight and the field's top and bottom edges.
    pub const SEGMENT_INSET: f64 = 2.0;
    /// Corner radius of a segment's highlight.
    pub const SEGMENT_RADIUS: f64 = 2.0;
    /// Space between the field and its stepper.
    pub const STEPPER_GAP: f64 = 4.0;

    /// Width of a day cell in the calendar.
    pub const CELL_WIDTH: f64 = 24.0;
    /// Height of a day cell in the calendar.
    pub const CELL_HEIGHT: f64 = 20.0;
    /// Height of the calendar's header (the month and the buttons).
    pub const HEADER_HEIGHT: f64 = 26.0;
    /// Height of the row of weekday names.
    pub const WEEKDAY_HEIGHT: f64 = 18.0;
    /// Space between the calendar's edge and its content.
    pub const PADDING: f64 = 6.0;
    /// Side of the square hit area of the previous, today and next buttons.
    pub const BUTTON_SIZE: f64 = 18.0;
    /// Corner radius of the calendar's frame.
    pub const RADIUS: f64 = 3.0;
    /// Corner radius of a day's fill and of the buttons' hover fill.
    pub const CELL_RADIUS: f64 = 3.0;
    /// Width of the frame's border.
    pub const BORDER: f64 = 1.0;
    /// Stroke width of the arrows.
    pub const ARROW_STROKE: f64 = 1.5;
    /// Width of an arrow (a chevron pointing left or right).
    pub const ARROW_WIDTH: f64 = 4.0;
    /// Height of an arrow.
    pub const ARROW_HEIGHT: f64 = 8.0;
    /// Radius of the circle of the today button.
    pub const TODAY_RADIUS: f64 = 4.0;
    /// Stroke width of the ring around today's day.
    pub const TODAY_RING: f64 = 1.5;
    /// Opacity of a disabled picker.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// The calendar's paper.
    pub fn background(variant: Variant) -> Color {
        SemanticColors::new(variant).content_background
    }

    /// The frame around the calendar.
    pub fn border(variant: Variant) -> Color {
        let c = match variant.appearance {
            Appearance::Light => black(0.35),
            Appearance::Dark => black(0.7),
        };
        if variant.is_increased_contrast() {
            c.with_alpha((c.components[3] + 0.3).min(1.0))
        } else {
            c
        }
    }

    /// The hairline under the weekday names.
    pub fn rule(variant: Variant) -> Color {
        SemanticColors::new(variant).separator
    }

    /// The fill of a segment, the selected day: the selection colour.
    pub fn selection(variant: Variant) -> Color {
        SemanticColors::new(variant).selection
    }

    /// The arrows and the today button's circle.
    pub fn arrow(variant: Variant) -> Color {
        let c = match variant.appearance {
            Appearance::Light => black(0.55),
            Appearance::Dark => white(0.65),
        };
        if variant.is_increased_contrast() {
            match variant.appearance {
                Appearance::Light => black(0.8),
                Appearance::Dark => white(0.9),
            }
        } else {
            c
        }
    }

    /// The ring around today's day.
    pub fn today_ring(variant: Variant) -> Color {
        SemanticColors::new(variant).accent
    }

    /// The fill of the day or button under the pointer.
    pub fn hover_fill(variant: Variant) -> Color {
        match variant.appearance {
            Appearance::Light => black(0.08),
            Appearance::Dark => white(0.12),
        }
    }

    /// The fill of the day or button held down.
    pub fn pressed_fill(variant: Variant) -> Color {
        match variant.appearance {
            Appearance::Light => black(0.18),
            Appearance::Dark => white(0.24),
        }
    }
}

/// The colour well (a bevel button holding a swatch) and its panel: the hue and
/// saturation area, and the checkerboard that shows opacity.
pub mod color_well {
    use crate::{Appearance, Color, Variant, black, grey, white};

    /// Width of the well.
    pub const WIDTH: f64 = 46.0;
    /// Space between the well's bevel and its swatch.
    pub const SWATCH_INSET: f64 = 3.0;
    /// Corner radius of the swatch.
    pub const SWATCH_RADIUS: f64 = 2.0;
    /// Side of one checkerboard square.
    pub const CHECKER: f64 = 4.0;
    /// Width of the panel's content.
    pub const PANEL_WIDTH: f64 = 220.0;
    /// Height of the hue and saturation area.
    pub const AREA_HEIGHT: f64 = 120.0;
    /// Width of the label column in the panel (labels sit right-aligned in it).
    pub const LABEL_WIDTH: f64 = 72.0;
    /// Height of a row of the panel (label and slider, or label and hex field).
    pub const ROW_HEIGHT: f64 = 22.0;
    /// Radius of the area's knob.
    pub const KNOB_RADIUS: f64 = 5.5;
    /// How far a key moves the area's knob, as a fraction of the area.
    pub const KEY_STEP: f64 = 0.02;
    /// Corner radius of the area.
    pub const AREA_RADIUS: f64 = 2.0;
    /// Opacity of a disabled well.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// The checkerboard's two squares, light and dark.
    pub fn checker(variant: Variant) -> [Color; 2] {
        match variant.appearance {
            Appearance::Light => [grey(1.0), grey(0.8)],
            Appearance::Dark => [grey(0.62), grey(0.42)],
        }
    }

    /// The thin line around the swatch and the area.
    pub fn border(variant: Variant) -> Color {
        let c = match variant.appearance {
            Appearance::Light => black(0.45),
            Appearance::Dark => black(0.7),
        };
        if variant.is_increased_contrast() {
            c.with_alpha((c.components[3] + 0.3).min(1.0))
        } else {
            c
        }
    }

    /// The knob's outer ring (the inner one is white): dark enough to show on light
    /// colours.
    pub fn knob_outline(_variant: Variant) -> Color {
        black(0.6)
    }

    /// The knob's inner ring.
    pub fn knob_ring(_variant: Variant) -> Color {
        white(1.0)
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
    /// Space between a unified toolbar and the window's left and right edges.
    pub const TOOLBAR_INSET: f64 = 8.0;
    /// Space between a unified toolbar's items and the bottom of the chrome.
    pub const TOOLBAR_BOTTOM: f64 = 6.0;

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
    /// Space left and right of a menu title's text, inside its highlight.
    pub const TITLE_PADDING: f64 = 9.0;
    /// Space between the screen's left and right edges and the first and last title.
    pub const EDGE_INSET: f64 = 6.0;
    /// Width and height of the logo mark that opens the logo menu.
    pub const LOGO_SIZE: f64 = 14.0;
    /// Stroke width of the logo mark.
    pub const LOGO_STROKE: f64 = 1.6;
    /// Width of the battery extra's icon, its terminal included.
    pub const BATTERY_WIDTH: f64 = 24.0;
    /// Height of the battery extra's icon.
    pub const BATTERY_HEIGHT: f64 = 10.0;
    /// Width of the Bluetooth extra's icon, the dots of a connection included.
    pub const BLUETOOTH_WIDTH: f64 = 14.0;
    /// Height of the Bluetooth extra's icon.
    pub const BLUETOOTH_HEIGHT: f64 = 13.0;
    /// Stroke width of the Bluetooth rune.
    pub const BLUETOOTH_STROKE: f64 = 1.4;
    /// Opacity of the Bluetooth icon while the adapter is off.
    pub const OFF_OPACITY: f32 = 0.45;

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
        /// Halo around the charging bolt, so it reads over the battery's fill.
        pub knockout: Color,
        /// The same halo over the highlight of an open menu's title.
        pub highlighted_knockout: Color,
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
                    knockout: white(0.90),
                    highlighted_knockout: black(0.35),
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
                    knockout: black(0.80),
                    highlighted_knockout: black(0.45),
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
    /// Height of a HUD window's title bar: the title and the close button.
    pub const TITLE_HEIGHT: f64 = 19.0;
    /// Diameter of the close button.
    pub const CLOSE_SIZE: f64 = 13.0;
    /// Distance from the window's left edge to the close button.
    pub const CLOSE_INSET: f64 = 6.0;
    /// Distance from the top of a HUD window to its title's baseline.
    pub const TITLE_BASELINE: f64 = 13.0;
    /// Room kept clear on each side of the title: the close button and a gap, mirrored on
    /// the right so the title stays centred.
    pub const TITLE_INSET: f64 = CLOSE_INSET + CLOSE_SIZE + 6.0;

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
        /// A HUD window's close button: its disc at rest, and while pressed.
        pub close: [Color; 2],
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
                    close: [white(0.22), white(0.38)],
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
                    close: [white(0.18), white(0.32)],
                },
            };
            if contrast {
                Self {
                    rim: white(0.70),
                    close: [white(0.34), white(0.52)],
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

/// The desktop's default picture (task 3.2c): a sky, a sun (or moon) and three ranges of
/// rolling hills, all vector so it is sharp at any size and scale.
pub mod desktop {
    use crate::{Appearance, Color, Variant, rgb};

    /// Where the glow sits, as fractions of the picture's width and height.
    pub const GLOW_CENTER: (f64, f64) = (0.72, 0.42);
    /// The glow's radius, as a fraction of the picture's width.
    pub const GLOW_RADIUS: f64 = 0.38;
    /// Each range of hills, back to front: the height of its ridge at five evenly spaced
    /// points from the left edge to the right edge, as a fraction of the picture's height
    /// from the top. A smooth curve runs through them.
    pub const HILLS: [[f64; 5]; 3] = [
        [0.62, 0.54, 0.60, 0.50, 0.58],
        [0.74, 0.80, 0.70, 0.76, 0.72],
        [0.90, 0.84, 0.92, 0.86, 0.90],
    ];

    /// Colours of the picture.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Sky gradient from the top to the horizon.
        pub sky: [Color; 3],
        /// The sun's (or moon's) glow; fades to transparent at its edge.
        pub glow: Color,
        /// Each range of hills, back to front, fading into the sky with distance.
        pub hills: [Color; 3],
        /// What shows beside a picture that doesn't fill the screen.
        pub letterbox: Color,
    }

    impl Palette {
        /// Colours for `variant`. The accent doesn't tint the desktop.
        pub fn new(variant: Variant) -> Self {
            match variant.appearance {
                Appearance::Light => Self {
                    sky: [
                        rgb(0.36, 0.58, 0.84),
                        rgb(0.62, 0.78, 0.93),
                        rgb(0.97, 0.89, 0.78),
                    ],
                    glow: rgb(1.0, 0.95, 0.80),
                    hills: [
                        rgb(0.45, 0.62, 0.76),
                        rgb(0.30, 0.47, 0.60),
                        rgb(0.17, 0.31, 0.42),
                    ],
                    letterbox: rgb(0.0, 0.0, 0.0),
                },
                Appearance::Dark => Self {
                    sky: [
                        rgb(0.05, 0.07, 0.17),
                        rgb(0.14, 0.15, 0.32),
                        rgb(0.40, 0.29, 0.45),
                    ],
                    glow: rgb(0.78, 0.80, 0.98),
                    hills: [
                        rgb(0.22, 0.22, 0.40),
                        rgb(0.12, 0.13, 0.27),
                        rgb(0.05, 0.06, 0.15),
                    ],
                    letterbox: rgb(0.0, 0.0, 0.0),
                },
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
                assert_ne!(
                    menu::Palette::new(light, backdrop),
                    menu::Palette::new(dark, backdrop)
                );
                assert_ne!(
                    popover::Palette::new(light, backdrop),
                    popover::Palette::new(dark, backdrop)
                );
                assert_ne!(
                    sheet::Palette::new(light, backdrop),
                    sheet::Palette::new(dark, backdrop)
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
                menu::Palette::new(v, backdrop).fill.to_vec(),
                popover::Palette::new(v, backdrop).fill.to_vec(),
                vec![sheet::Palette::new(v, backdrop).fill],
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

/// The groove of a slider and the trough of a progress bar: a recessed, light (or dark)
/// channel with a dark rim and an etched highlight below.
pub mod groove {
    use super::CONTRAST_RIM;
    use crate::{Appearance, Color, Variant, darken, grey, white};

    /// Rim thickness.
    pub const RIM: f64 = crate::metrics::control::RIM;
    /// How far the etched highlight sits below the groove.
    pub const ETCH_OFFSET: f64 = 1.0;

    /// Colours of the groove.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Etched highlight below the groove.
        pub etch: Color,
        /// Rim gradient, top and bottom.
        pub rim: [Color; 2],
        /// Inside: top and bottom (the top is darker: an inner shadow).
        pub fill: [Color; 2],
    }

    impl Palette {
        /// Colours for `variant`.
        pub fn new(variant: Variant) -> Self {
            let p = match variant.appearance {
                Appearance::Light => Self {
                    etch: white(0.6),
                    rim: [grey(0.46), grey(0.62)],
                    fill: [grey(0.76), grey(0.93)],
                },
                Appearance::Dark => Self {
                    etch: white(0.12),
                    rim: [grey(0.04), grey(0.10)],
                    fill: [grey(0.10), grey(0.22)],
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

/// The slider: a groove with a round gel knob.
pub mod slider {
    /// Height of the groove.
    pub const TRACK_HEIGHT: f64 = 5.0;
    /// Diameter of the knob.
    pub const KNOB: f64 = 15.0;
    /// How much a neutral knob darkens while held down (1: not at all).
    pub const PRESSED_DIM: f32 = 0.86;
    /// Opacity of a disabled slider.
    pub const DISABLED_OPACITY: f32 = 0.5;
}

/// The progress bar: a groove filled with accent gel; indeterminate bars show the barber
/// pole, diagonal stripes that move.
pub mod progress {
    use crate::{Color, white};

    /// Height of the bar.
    pub const HEIGHT: f64 = crate::metrics::control::PROGRESS_HEIGHT;
    /// Horizontal distance after which the stripes repeat (a light and a plain stripe).
    pub const STRIPE_PERIOD: f64 = 20.0;
    /// The light stripes drawn over the gel.
    pub const STRIPE: Color = white(0.32);
    /// Opacity of a disabled bar.
    pub const DISABLED_OPACITY: f32 = 0.5;
}

/// The stepper: two small gel halves, with an up arrow over a down arrow.
pub mod stepper {
    use crate::{Appearance, Color, Variant, black, grey, white};

    /// Corner radius.
    pub const RADIUS: f64 = crate::radii::CONTROL;
    /// Width of the divider between the halves.
    pub const DIVIDER: f64 = 1.0;
    /// Width of an arrow.
    pub const ARROW_WIDTH: f64 = 6.0;
    /// Height of an arrow.
    pub const ARROW_HEIGHT: f64 = 4.0;
    /// How much the half held down darkens (1: not at all).
    pub const PRESSED_DIM: f32 = 0.86;
    /// Opacity of a disabled stepper.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// The arrows.
    pub fn arrow(variant: Variant) -> Color {
        match variant.appearance {
            Appearance::Light => grey(0.18),
            Appearance::Dark => white(0.9),
        }
    }

    /// The divider between the halves.
    pub fn divider(variant: Variant) -> Color {
        match variant.appearance {
            Appearance::Light => black(0.30),
            Appearance::Dark => black(0.55),
        }
    }
}

/// Pop-up and pull-down buttons (neutral gel with an accent cap carrying arrows) and the
/// combo box's arrow button.
pub mod popup {
    use crate::{Color, white};

    /// Width of an arrow.
    pub const ARROW_WIDTH: f64 = 6.0;
    /// Height of an arrow.
    pub const ARROW_HEIGHT: f64 = 4.0;
    /// Gap between the two arrows of a pop-up button.
    pub const ARROW_GAP: f64 = 2.0;
    /// How much the cap or arrow button of an open menu darkens (1: not at all).
    pub const OPEN_DIM: f32 = 0.86;
    /// Opacity of a disabled button.
    pub const DISABLED_OPACITY: f32 = 0.5;
    /// Inset of the combo box's arrow button from the well's inner edge.
    pub const COMBO_INSET: f64 = 1.0;

    /// The arrows on the accent cap.
    pub const CAP_ARROW: Color = white(1.0);
}

/// Text, search and secure fields: a white well with a hairline border, darker along the
/// top, and an inner shadow under it. Search fields are rounded and carry a magnifier and
/// a cancel button.
pub mod text_field {
    use super::CONTRAST_RIM;
    use crate::{Appearance, Color, Variant, black, darken, grey, white};

    /// Border thickness.
    pub const BORDER: f64 = crate::metrics::control::RIM;
    /// Height of the inner shadow under the top border.
    pub const SHADOW: f64 = 2.0;
    /// Width of the text caret.
    pub const CARET_WIDTH: f64 = 1.0;
    /// Opacity of a disabled field's well (its text uses the disabled label colour).
    pub const DISABLED_OPACITY: f32 = 0.5;
    /// Diameter of the magnifier's lens.
    pub const MAGNIFIER_LENS: f64 = 7.0;
    /// Stroke width of the magnifier.
    pub const MAGNIFIER_STROKE: f64 = 1.5;
    /// Diameter of the cancel button's disc.
    pub const CANCEL: f64 = 13.0;
    /// Half the length of each stroke of the cancel button's cross.
    pub const CANCEL_CROSS: f64 = 2.5;
    /// How much the cancel button darkens while held down (1: not at all).
    pub const PRESSED_DIM: f32 = 0.75;

    /// Colours of a field's well.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Inside the field.
        pub fill: Color,
        /// The top border.
        pub border_top: Color,
        /// The side and bottom borders.
        pub border: Color,
        /// The inner shadow at the top edge, fading out over [`SHADOW`].
        pub shadow: Color,
        /// The magnifier.
        pub glyph: Color,
        /// The cancel button's disc (its cross is [`Self::fill`]).
        pub cancel: Color,
    }

    impl Palette {
        /// Colours for `variant`.
        pub fn new(variant: Variant) -> Self {
            let p = match variant.appearance {
                Appearance::Light => Self {
                    fill: grey(1.0),
                    border_top: grey(0.55),
                    border: grey(0.70),
                    shadow: black(0.12),
                    glyph: black(0.55),
                    cancel: black(0.35),
                },
                Appearance::Dark => Self {
                    fill: grey(0.118),
                    border_top: grey(0.03),
                    border: grey(0.08),
                    shadow: black(0.35),
                    glyph: white(0.55),
                    cancel: white(0.40),
                },
            };
            if variant.is_increased_contrast() {
                Self {
                    border_top: darken(p.border_top, CONTRAST_RIM),
                    border: darken(p.border, CONTRAST_RIM),
                    ..p
                }
            } else {
                p
            }
        }
    }
}

/// The spinner: twelve spokes, the newest dark and the rest fading behind it.
pub mod spinner {
    use crate::{Appearance, Color, Variant, black, white};

    /// Number of spokes. The spinner turns in this many steps.
    pub const SPOKES: usize = 12;
    /// Opacity of the faintest spoke.
    pub const MIN_OPACITY: f32 = 0.15;
    /// Length of a spoke, as a fraction of the radius.
    pub const SPOKE_LENGTH: f64 = 0.5;
    /// Width of a spoke, as a fraction of the diameter.
    pub const SPOKE_WIDTH: f64 = 0.12;
    /// Opacity of a disabled spinner.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// The spokes' colour before fading.
    pub fn spoke(variant: Variant) -> Color {
        match variant.appearance {
            Appearance::Light => black(0.75),
            Appearance::Dark => white(0.85),
        }
    }
}

/// Overlay scrollers: a thin translucent knob over the content, which widens and gains a
/// track while the pointer is over it.
pub mod scroller {
    use crate::{Appearance, Color, Variant, black, white};

    /// Width of the knob at rest.
    pub const KNOB_THIN: f64 = 7.0;
    /// Width of the knob while the pointer is over the scroller.
    pub const KNOB_WIDE: f64 = 11.0;
    /// Space between the knob and the scroller's outer edge, and its ends.
    pub const KNOB_INSET: f64 = 2.0;
    /// The knob never gets shorter than this, however long the content.
    pub const KNOB_MIN_LENGTH: f64 = 18.0;
    /// Width of the hairline on the track's inner edge.
    pub const TRACK_EDGE: f64 = 1.0;

    /// Colours of a scroller.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// The knob.
        pub knob: Color,
        /// The track behind a widened knob.
        pub track: Color,
        /// The hairline on the track's inner edge.
        pub track_edge: Color,
    }

    impl Palette {
        /// Colours for `variant`: a dark knob on light content, a light one on dark.
        pub fn new(variant: Variant) -> Self {
            let contrast = variant.is_increased_contrast();
            match variant.appearance {
                Appearance::Light => Self {
                    knob: black(if contrast { 0.7 } else { 0.5 }),
                    track: white(0.8),
                    track_edge: black(if contrast { 0.3 } else { 0.12 }),
                },
                Appearance::Dark => Self {
                    knob: white(if contrast { 0.75 } else { 0.5 }),
                    track: black(0.5),
                    track_edge: white(if contrast { 0.3 } else { 0.1 }),
                },
            }
        }
    }
}

/// Menus: context menus and submenus (task 2.17a), later the menu bar's menus and pop-up
/// buttons' menus. A light (or dark) panel with a soft shadow; the item under the pointer
/// or the keyboard is an accent band with white text.
pub mod menu {
    use super::Backdrop;
    use crate::{Accent, Appearance, Color, Variant, black, grey, rgb, rgba, white};

    /// Corner radius of the panel.
    pub const RADIUS: f64 = crate::radii::MENU;
    /// Rim thickness.
    pub const RIM: f64 = 1.0;
    /// Space above the first item and below the last.
    pub const PADDING_Y: f64 = 4.0;
    /// Height of an item.
    pub const ITEM_HEIGHT: f64 = 19.0;
    /// Height of a separator (the line is centred in it).
    pub const SEPARATOR_HEIGHT: f64 = 10.0;
    /// Thickness of a separator's line.
    pub const SEPARATOR_LINE: f64 = 1.0;
    /// Left edge of an item's label: the column before it holds the check mark.
    pub const LABEL_INSET: f64 = 20.0;
    /// Space after the shortcut, or after the label when an item has none.
    pub const TRAILING_INSET: f64 = 14.0;
    /// Room for the submenu arrow at the right of an item.
    pub const ARROW_WIDTH: f64 = 14.0;
    /// Width and height of the submenu arrow (a triangle pointing right).
    pub const ARROW_SIZE: f64 = 7.0;
    /// Smallest gap between a label and its shortcut.
    pub const SHORTCUT_GAP: f64 = 24.0;
    /// Side of the box the check mark, mixed dash or radio dot is drawn in.
    pub const MARK_SIZE: f64 = 9.0;
    /// Stroke width of the check mark and the dash.
    pub const MARK_STROKE: f64 = 1.75;
    /// A menu is never narrower than this.
    pub const MIN_WIDTH: f64 = 120.0;
    /// How far a submenu overlaps the menu it opens from (it covers the rim).
    pub const SUBMENU_OVERLAP: f64 = 2.0;
    /// How far the drop shadow sits below the panel.
    pub const SHADOW_OFFSET: f64 = 4.0;
    /// Blur radius (standard deviation) of the drop shadow.
    pub const SHADOW_BLUR: f64 = 5.0;
    /// How far the shadow reaches beyond the panel's sides and top.
    pub const SHADOW_REACH: f64 = 3.0 * SHADOW_BLUR;
    /// How far the pointer has to move after a press opened a menu before releasing the
    /// button chooses the item under it (press, drag, release).
    pub const DRAG_THRESHOLD: f64 = 4.0;

    /// Colours of a menu.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Drop shadow.
        pub shadow: Color,
        /// Body, top and bottom.
        pub fill: [Color; 2],
        /// Rim, inside the body's edge.
        pub rim: Color,
        /// Item labels and marks.
        pub label: Color,
        /// Labels and marks of disabled items.
        pub disabled_label: Color,
        /// Shortcuts.
        pub shortcut: Color,
        /// Separator lines.
        pub separator: Color,
        /// The highlighted item's band, top and bottom.
        pub highlight: [Color; 2],
        /// A hairline along the band's top edge.
        pub highlight_edge: Color,
        /// Label, shortcut, mark and arrow on the band.
        pub highlighted_label: Color,
    }

    impl Palette {
        /// Colours for `variant` on `backdrop`. The highlight follows the accent; increased
        /// contrast firms up the rim, the separators and the text, and makes the body
        /// opaque. Only the body's fill depends on the backdrop.
        pub fn new(variant: Variant, backdrop: Backdrop) -> Self {
            let backdrop = backdrop.for_variant(variant);
            let highlight = match (variant.accent, variant.appearance) {
                (Accent::Blue, Appearance::Light) => [rgb(0.36, 0.53, 0.96), rgb(0.12, 0.34, 0.90)],
                (Accent::Blue, Appearance::Dark) => [rgb(0.30, 0.47, 0.92), rgb(0.10, 0.29, 0.80)],
                (Accent::Graphite, Appearance::Light) => [grey(0.58), grey(0.40)],
                (Accent::Graphite, Appearance::Dark) => [grey(0.52), grey(0.36)],
            };
            let p = match variant.appearance {
                Appearance::Light => Self {
                    shadow: black(0.38),
                    fill: match backdrop {
                        Backdrop::Opaque => [grey(0.985), grey(0.955)],
                        Backdrop::Translucent => [
                            rgba(0.985, 0.985, 0.985, 0.84),
                            rgba(0.955, 0.955, 0.955, 0.84),
                        ],
                    },
                    rim: black(0.18),
                    label: black(0.88),
                    disabled_label: black(0.30),
                    shortcut: black(0.70),
                    separator: black(0.10),
                    highlight,
                    highlight_edge: white(0.18),
                    highlighted_label: white(1.0),
                },
                Appearance::Dark => Self {
                    shadow: black(0.65),
                    fill: match backdrop {
                        Backdrop::Opaque => [grey(0.22), grey(0.18)],
                        Backdrop::Translucent => {
                            [rgba(0.22, 0.22, 0.22, 0.80), rgba(0.18, 0.18, 0.18, 0.80)]
                        }
                    },
                    rim: white(0.16),
                    label: white(0.90),
                    disabled_label: white(0.32),
                    shortcut: white(0.70),
                    separator: white(0.12),
                    highlight,
                    highlight_edge: white(0.14),
                    highlighted_label: white(1.0),
                },
            };
            if !variant.is_increased_contrast() {
                return p;
            }
            match variant.appearance {
                Appearance::Light => Self {
                    rim: black(0.55),
                    label: black(1.0),
                    disabled_label: black(0.50),
                    shortcut: black(0.90),
                    separator: black(0.35),
                    ..p
                },
                Appearance::Dark => Self {
                    rim: white(0.50),
                    label: white(1.0),
                    disabled_label: white(0.55),
                    shortcut: white(0.90),
                    separator: white(0.35),
                    ..p
                },
            }
        }
    }
}

/// Popovers (task 2.23a): a rounded panel with an arrow pointing at the control it opened
/// from, a soft drop shadow and a thin rim. Light in the light appearance, dark in the dark
/// one; content draws in the window's usual colours.
pub mod popover {
    use super::Backdrop;
    use crate::{Appearance, Color, Variant, black, grey, rgba, white};

    /// Corner radius of the panel.
    pub const RADIUS: f64 = crate::radii::MENU;
    /// Rim thickness.
    pub const RIM: f64 = 1.0;
    /// Width of the arrow's base.
    pub const ARROW_WIDTH: f64 = 22.0;
    /// How far the arrow's tip stands out from the panel.
    pub const ARROW_HEIGHT: f64 = 11.0;
    /// Space between the panel's edge and its content.
    pub const PADDING: f64 = crate::metrics::spacing::M;
    /// Space between the arrow's tip and the control it points at.
    pub const GAP: f64 = 1.0;
    /// How far the drop shadow sits below the panel.
    pub const SHADOW_OFFSET: f64 = 3.0;
    /// Blur radius (standard deviation) of the drop shadow.
    pub const SHADOW_BLUR: f64 = 6.0;
    /// Room the shadow needs around the popover, the same on every side so that a
    /// popover flipped to the other side of its control keeps its arrow on the control.
    pub const SHADOW_REACH: f64 = 3.0 * SHADOW_BLUR + SHADOW_OFFSET;

    /// Colours of a popover.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// Drop shadow.
        pub shadow: Color,
        /// Body, top and bottom.
        pub fill: [Color; 2],
        /// Rim, inside the outline.
        pub rim: Color,
        /// A light line just inside the rim along the top.
        pub highlight: Color,
    }

    impl Palette {
        /// Colours for `variant` on `backdrop`; increased contrast firms up the rim and
        /// makes the body opaque.
        pub fn new(variant: Variant, backdrop: Backdrop) -> Self {
            let backdrop = backdrop.for_variant(variant);
            let p = match variant.appearance {
                Appearance::Light => Self {
                    shadow: black(0.40),
                    fill: match backdrop {
                        Backdrop::Opaque => [grey(0.975), grey(0.925)],
                        Backdrop::Translucent => [
                            rgba(0.975, 0.975, 0.975, 0.86),
                            rgba(0.925, 0.925, 0.925, 0.86),
                        ],
                    },
                    rim: black(0.22),
                    highlight: white(0.85),
                },
                Appearance::Dark => Self {
                    shadow: black(0.70),
                    fill: match backdrop {
                        Backdrop::Opaque => [grey(0.215), grey(0.165)],
                        Backdrop::Translucent => [
                            rgba(0.215, 0.215, 0.215, 0.82),
                            rgba(0.165, 0.165, 0.165, 0.82),
                        ],
                    },
                    rim: white(0.17),
                    highlight: white(0.08),
                },
            };
            if !variant.is_increased_contrast() {
                return p;
            }
            match variant.appearance {
                Appearance::Light => Self {
                    rim: black(0.60),
                    ..p
                },
                Appearance::Dark => Self {
                    rim: white(0.55),
                    ..p
                },
            }
        }
    }
}

/// Sheets (task 2.23b): a modal panel that slides out from under a window's title bar. It
/// is the window's background with the bar's shadow falling on its top edge, a hairline
/// rim down its sides and along its bottom, and, drawn over the window it belongs to, a
/// soft drop shadow beside and below it.
pub mod sheet {
    use super::Backdrop;
    use crate::colors::SemanticColors;
    use crate::{Appearance, Color, Variant, black, white};

    /// Space between the sheet's edges and its content.
    pub const PADDING: f64 = crate::metrics::spacing::L;
    /// Space between the content and the row of buttons under it.
    pub const BUTTON_ROW_GAP: f64 = crate::metrics::spacing::L;
    /// Space between two buttons in the row.
    pub const BUTTON_GAP: f64 = crate::metrics::spacing::M;
    /// How far down the title bar's shadow reaches into the sheet.
    pub const INNER_SHADOW: f64 = 8.0;
    /// Rim thickness.
    pub const RIM: f64 = 1.0;
    /// How far the drop shadow sits below the sheet.
    pub const SHADOW_OFFSET: f64 = 3.0;
    /// Blur radius (standard deviation) of the drop shadow.
    pub const SHADOW_BLUR: f64 = 7.0;
    /// Room the drop shadow needs beside and below the sheet.
    pub const SHADOW_REACH: f64 = 3.0 * SHADOW_BLUR + SHADOW_OFFSET;
    /// Opacity of the body over a blurred backdrop.
    pub const TRANSLUCENT_ALPHA: f32 = 0.88;

    /// Colours of a sheet.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// The body: the window background.
        pub fill: Color,
        /// The title bar's shadow at the very top; it fades out over [`INNER_SHADOW`].
        pub inner_shadow: Color,
        /// Rim along the sides and the bottom, inside the edge.
        pub rim: Color,
        /// Drop shadow, outside the sheet.
        pub shadow: Color,
    }

    impl Palette {
        /// Colours for `variant` on `backdrop`; increased contrast firms up the rim and
        /// makes the body opaque. Translucent, the body is the window background at
        /// [`TRANSLUCENT_ALPHA`].
        pub fn new(variant: Variant, backdrop: Backdrop) -> Self {
            let backdrop = backdrop.for_variant(variant);
            let fill = SemanticColors::new(variant).window_background;
            let fill = match backdrop {
                Backdrop::Opaque => fill,
                Backdrop::Translucent => fill.with_alpha(TRANSLUCENT_ALPHA),
            };
            let p = match variant.appearance {
                Appearance::Light => Self {
                    fill,
                    inner_shadow: black(0.30),
                    rim: black(0.20),
                    shadow: black(0.38),
                },
                Appearance::Dark => Self {
                    fill,
                    inner_shadow: black(0.55),
                    rim: white(0.14),
                    shadow: black(0.65),
                },
            };
            if !variant.is_increased_contrast() {
                return p;
            }
            match variant.appearance {
                Appearance::Light => Self {
                    rim: black(0.60),
                    ..p
                },
                Appearance::Dark => Self {
                    rim: white(0.55),
                    ..p
                },
            }
        }
    }
}

/// Tables (task 2.19a) and outline views (2.19b): a header bar of column titles over rows of one height, in a scroll
/// view with a hairline border. Row stripes and the selection use the semantic colours
/// ([`crate::colors::SemanticColors::alternate_row`], `selection`, `unfocused_selection`).
pub mod table {
    use crate::{Appearance, Color, Variant, black, grey, white};

    /// Height of a row.
    pub const ROW_HEIGHT: f64 = 19.0;
    /// Height of the header bar, its bottom border included.
    pub const HEADER_HEIGHT: f64 = 17.0;
    /// Space between a cell's edges and its text.
    pub const CELL_INSET: f64 = 4.0;
    /// Space between a column title and the column's edges.
    pub const TITLE_INSET: f64 = 5.0;
    /// Width of a column that doesn't set one.
    pub const COLUMN_WIDTH: f64 = 100.0;
    /// Narrowest a column gets by resizing, unless it sets its own minimum.
    pub const MIN_COLUMN_WIDTH: f64 = 24.0;
    /// How far either side of a divider a press grabs it to resize the column.
    pub const DIVIDER_GRAB: f64 = 3.0;
    /// Width of a column divider in the header.
    pub const DIVIDER: f64 = 1.0;
    /// Height of the header's bottom border.
    pub const HEADER_BORDER: f64 = 1.0;
    /// Width of the scroll view's border.
    pub const BORDER: f64 = 1.0;
    /// Width of the sort indicator (a triangle; up when ascending).
    pub const SORT_ARROW_WIDTH: f64 = 7.0;
    /// Height of the sort indicator.
    pub const SORT_ARROW_HEIGHT: f64 = 5.0;
    /// Space between the sort indicator and the column's right edge.
    pub const SORT_ARROW_INSET: f64 = 6.0;
    /// How far each level of an outline view indents its first column (task 2.19b).
    pub const OUTLINE_INDENT: f64 = 14.0;
    /// Width of the disclosure column of an outline row: the triangle sits in it, and
    /// rows without one keep it empty so their text lines up.
    pub const DISCLOSURE_WIDTH: f64 = 14.0;
    /// The disclosure triangle's long side (its base) and short side (its depth).
    pub const DISCLOSURE_BASE: f64 = 9.0;
    /// The disclosure triangle's depth.
    pub const DISCLOSURE_DEPTH: f64 = 6.0;

    /// Colours of a table's header and frame.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// The header bar, top and bottom.
        pub header: [Color; 2],
        /// A pressed column title, top and bottom.
        pub header_pressed: [Color; 2],
        /// The line under the header.
        pub header_border: Color,
        /// The lines between column titles.
        pub divider: Color,
        /// A highlight one pixel right of each divider.
        pub divider_highlight: Color,
        /// Column titles.
        pub title: Color,
        /// Titles of a disabled table.
        pub disabled_title: Color,
        /// The sort indicator.
        pub sort_arrow: Color,
        /// An outline view's disclosure triangles (white on a focused selection, the
        /// semantic `selected_label`).
        pub disclosure: Color,
        /// The scroll view's border.
        pub border: Color,
    }

    impl Palette {
        /// Colours for `variant`; increased contrast firms up the lines.
        pub fn new(variant: Variant) -> Self {
            let contrast = variant.is_increased_contrast();
            match variant.appearance {
                Appearance::Light => Self {
                    header: [grey(1.0), grey(0.91)],
                    header_pressed: [grey(0.86), grey(0.79)],
                    header_border: if contrast { black(0.6) } else { grey(0.70) },
                    divider: if contrast { black(0.5) } else { grey(0.78) },
                    divider_highlight: white(0.8),
                    title: black(0.85),
                    disabled_title: black(0.30),
                    sort_arrow: black(if contrast { 0.75 } else { 0.50 }),
                    disclosure: black(if contrast { 0.80 } else { 0.55 }),
                    border: if contrast { black(0.6) } else { grey(0.66) },
                },
                Appearance::Dark => Self {
                    header: [grey(0.27), grey(0.22)],
                    header_pressed: [grey(0.20), grey(0.16)],
                    header_border: if contrast { white(0.5) } else { black(0.55) },
                    divider: if contrast { white(0.45) } else { white(0.12) },
                    divider_highlight: black(0.25),
                    title: white(0.88),
                    disabled_title: white(0.32),
                    sort_arrow: white(if contrast { 0.85 } else { 0.60 }),
                    disclosure: white(if contrast { 0.90 } else { 0.65 }),
                    border: if contrast { white(0.5) } else { black(0.6) },
                },
            }
        }
    }
}

/// Source-list sidebars (task 2.19c): a pale tinted background, group headers in small
/// capitals, rows of an icon, a label and a count badge, and the gradient the selected row
/// is drawn with, in the accent while the list has focus and grey without. Text and the
/// disclosure triangle of a selected row use the semantic `selected_label`.
pub mod sidebar {
    use crate::{Accent, Appearance, Color, Variant, black, grey, rgb, white};

    /// Height of every row, group headers included.
    pub const ROW_HEIGHT: f64 = 22.0;
    /// Space between the left edge and a group header's text.
    pub const GROUP_INSET: f64 = 8.0;
    /// Where the icon (or the label, without one) of a first-level item starts.
    pub const ITEM_INSET: f64 = 14.0;
    /// How far each level of nested items indents.
    pub const INDENT: f64 = 14.0;
    /// Width of the disclosure column, just left of an item's icon.
    pub const DISCLOSURE_WIDTH: f64 = 14.0;
    /// Width and height of an icon.
    pub const ICON_SIZE: f64 = 16.0;
    /// Space between an icon and its label.
    pub const ICON_GAP: f64 = 6.0;
    /// Height of a count badge; a short count makes a circle this wide.
    pub const BADGE_HEIGHT: f64 = 15.0;
    /// Space between a badge's edges and its text.
    pub const BADGE_PADDING: f64 = 6.0;
    /// Space between a badge and the right edge.
    pub const BADGE_INSET: f64 = 8.0;
    /// Space between a group header's Show or Hide and the right edge.
    pub const ACTION_INSET: f64 = 8.0;
    /// Width of the press target of Show or Hide, from the right edge.
    pub const ACTION_WIDTH: f64 = 48.0;
    /// Width of the line on the sidebar's right edge.
    pub const EDGE: f64 = 1.0;

    /// Colours of a sidebar.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// The sidebar's background.
        pub background: Color,
        /// The line on its right edge.
        pub edge: Color,
        /// Group headers.
        pub group_label: Color,
        /// A group header's Show or Hide, while the pointer is over it.
        pub group_action: Color,
        /// Icons (white on a focused selection, the semantic `selected_label`).
        pub icon: Color,
        /// The selected row with focus, top and bottom.
        pub selection: [Color; 2],
        /// The lines at the top and the bottom of a focused selection.
        pub selection_edge: [Color; 2],
        /// The selected row without focus, top and bottom.
        pub unfocused_selection: [Color; 2],
        /// The lines at the top and the bottom of an unfocused selection.
        pub unfocused_edge: [Color; 2],
        /// The shadow under the white label of a focused selection (one pixel down).
        pub selected_shadow: Color,
        /// A count badge.
        pub badge: Color,
        /// The count in a badge.
        pub badge_label: Color,
        /// A badge on the selected row.
        pub selected_badge: Color,
        /// The count in a badge on a focused selection.
        pub selected_badge_label: Color,
        /// The count in a badge on an unfocused selection.
        pub unfocused_badge_label: Color,
    }

    impl Palette {
        /// Colours for `variant`; increased contrast firms up the lines and the small text.
        pub fn new(variant: Variant) -> Self {
            let contrast = variant.is_increased_contrast();
            let graphite = variant.accent == Accent::Graphite;
            match variant.appearance {
                Appearance::Light => {
                    let (selection, selection_edge) = if graphite {
                        (
                            [rgb(0.60, 0.65, 0.72), rgb(0.42, 0.47, 0.55)],
                            [rgb(0.36, 0.41, 0.49), rgb(0.33, 0.38, 0.46)],
                        )
                    } else {
                        (
                            [rgb(0.44, 0.62, 0.84), rgb(0.22, 0.43, 0.72)],
                            [rgb(0.28, 0.47, 0.74), rgb(0.19, 0.37, 0.64)],
                        )
                    };
                    Self {
                        background: rgb(0.867, 0.894, 0.925),
                        edge: if contrast {
                            black(0.6)
                        } else {
                            rgb(0.62, 0.65, 0.70)
                        },
                        group_label: if contrast {
                            rgb(0.25, 0.28, 0.33)
                        } else {
                            rgb(0.43, 0.47, 0.53)
                        },
                        group_action: if contrast {
                            rgb(0.10, 0.30, 0.65)
                        } else {
                            rgb(0.30, 0.50, 0.80)
                        },
                        icon: if contrast {
                            rgb(0.22, 0.26, 0.32)
                        } else {
                            rgb(0.38, 0.42, 0.49)
                        },
                        selection,
                        selection_edge,
                        unfocused_selection: [rgb(0.78, 0.81, 0.85), rgb(0.66, 0.70, 0.76)],
                        unfocused_edge: [rgb(0.60, 0.64, 0.70), rgb(0.57, 0.61, 0.67)],
                        selected_shadow: black(0.40),
                        badge: rgb(0.55, 0.60, 0.68),
                        badge_label: white(1.0),
                        selected_badge: white(0.92),
                        selected_badge_label: selection[1],
                        unfocused_badge_label: rgb(0.45, 0.49, 0.56),
                    }
                }
                Appearance::Dark => {
                    let (selection, selection_edge) = if graphite {
                        (
                            [rgb(0.40, 0.44, 0.50), rgb(0.30, 0.34, 0.40)],
                            [rgb(0.52, 0.56, 0.62), rgb(0.22, 0.25, 0.30)],
                        )
                    } else {
                        (
                            [rgb(0.22, 0.43, 0.76), rgb(0.13, 0.30, 0.60)],
                            [rgb(0.38, 0.56, 0.86), rgb(0.08, 0.20, 0.45)],
                        )
                    };
                    Self {
                        background: rgb(0.150, 0.165, 0.185),
                        edge: if contrast { white(0.5) } else { black(0.55) },
                        group_label: if contrast {
                            white(0.80)
                        } else {
                            rgb(0.56, 0.60, 0.67)
                        },
                        group_action: if contrast {
                            rgb(0.55, 0.75, 1.0)
                        } else {
                            rgb(0.42, 0.62, 0.92)
                        },
                        icon: if contrast {
                            white(0.85)
                        } else {
                            rgb(0.62, 0.66, 0.73)
                        },
                        selection,
                        selection_edge,
                        unfocused_selection: [grey(0.36), grey(0.30)],
                        unfocused_edge: [grey(0.46), grey(0.22)],
                        selected_shadow: black(0.55),
                        badge: rgb(0.34, 0.38, 0.45),
                        badge_label: white(0.92),
                        selected_badge: white(0.90),
                        selected_badge_label: selection[1],
                        unfocused_badge_label: grey(0.30),
                    }
                }
            }
        }
    }
}

/// Collection views: items of an icon over a label in a grid of cells.
pub mod collection {
    use crate::{Appearance, Color, Variant, black, white};

    /// The icon size a collection starts with.
    pub const ICON_SIZE: f64 = 48.0;
    /// The smallest and the largest icon size a collection draws.
    pub const ICON_SIZES: (f64, f64) = (16.0, 128.0);
    /// Space between a cell's edge and the icon's backdrop, and between the cells'
    /// contents, at the sides.
    pub const CELL_INSET: f64 = 8.0;
    /// Space around an icon, inside its backdrop.
    pub const ICON_PADDING: f64 = 4.0;
    /// Space between an icon's backdrop and the label.
    pub const LABEL_GAP: f64 = 4.0;
    /// Space between the label's text and the edges of its pill.
    pub const LABEL_PADDING: f64 = 6.0;
    /// Height of the label's pill.
    pub const LABEL_HEIGHT: f64 = 17.0;
    /// Corner radius of an icon's backdrop.
    pub const ICON_RADIUS: f64 = crate::radii::PANEL;
    /// Opacity of an icon in a disabled collection.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// Colours of a collection's selection (labels use the semantic colours).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// The backdrop of a selected item's icon.
        pub icon_backdrop: Color,
        /// The same when the collection doesn't have keyboard focus.
        pub unfocused_icon_backdrop: Color,
    }

    impl Palette {
        /// Colours for `variant`.
        pub fn new(variant: Variant) -> Self {
            let contrast = variant.is_increased_contrast();
            match variant.appearance {
                Appearance::Light => Self {
                    icon_backdrop: black(if contrast { 0.22 } else { 0.14 }),
                    unfocused_icon_backdrop: black(if contrast { 0.14 } else { 0.08 }),
                },
                Appearance::Dark => Self {
                    icon_backdrop: white(if contrast { 0.28 } else { 0.20 }),
                    unfocused_icon_backdrop: white(if contrast { 0.18 } else { 0.12 }),
                },
            }
        }
    }

    /// The width and height of a cell for icons `icon_size` wide: the label gets the
    /// width of the cell less its insets and may be wider than the icon.
    pub fn cell_size(icon_size: f64) -> (f64, f64) {
        let icon = icon_size.clamp(ICON_SIZES.0, ICON_SIZES.1);
        let backdrop = icon + 2.0 * ICON_PADDING;
        let width = (backdrop + 2.0 * CELL_INSET).max(72.0);
        let height = CELL_INSET + backdrop + LABEL_GAP + LABEL_HEIGHT + CELL_INSET;
        (width, height)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn light_and_dark_differ_and_cells_grow_with_the_icon() {
            let light = Variant::default();
            let dark = Variant {
                appearance: Appearance::Dark,
                ..light
            };
            assert_ne!(Palette::new(light), Palette::new(dark));
            let (small, large) = (cell_size(32.0), cell_size(64.0));
            assert!(large.0 > small.0 && large.1 > small.1);
            assert_eq!(cell_size(1000.0), cell_size(ICON_SIZES.1), "clamped");
        }
    }
}

/// The split view: a divider between panes, thin (a line) or thick (a gradient bar with a
/// grip of dots), as in `NSSplitView` (task 2.21a).
pub mod split {
    use crate::{Appearance, Color, Variant, black, grey, white};

    /// Width of a thin divider.
    pub const THIN: f64 = 1.0;
    /// Width of a thick divider.
    pub const THICK: f64 = 9.0;
    /// How far either side of a thin divider a press grabs it, and the resize cursor
    /// shows. (A thick divider is grabbed by the bar itself.)
    pub const THIN_GRAB: f64 = 3.0;
    /// Diameter of one dot of the grip.
    pub const GRIP_DOT: f64 = 2.0;
    /// Distance between the centres of two grip dots.
    pub const GRIP_PITCH: f64 = 4.0;
    /// Number of dots in the grip.
    pub const GRIP_DOTS: usize = 3;
    /// How far the arrow keys move a focused divider.
    pub const KEY_STEP: f64 = 10.0;
    /// How far a drag may go beyond a collapsible pane's minimum before it collapses, as
    /// a fraction of that minimum.
    pub const COLLAPSE_AT: f64 = 0.5;
    /// The narrowest a pane gets by dragging, unless it sets its own minimum.
    pub const MIN_PANE: f64 = 32.0;
    /// Opacity of the dividers of a disabled split view.
    pub const DISABLED_OPACITY: f32 = 0.5;

    /// Colours of a divider.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// A thin divider's line.
        pub line: Color,
        /// A thick divider's gradient across its width (from the first pane to the second).
        pub bar: [Color; 2],
        /// The lines along both edges of a thick divider.
        pub edge: Color,
        /// The grip's dots.
        pub grip: Color,
        /// The highlight under each grip dot.
        pub grip_highlight: Color,
    }

    impl Palette {
        /// Colours for `variant`.
        pub fn new(variant: Variant) -> Self {
            let p = match variant.appearance {
                Appearance::Light => Self {
                    line: black(0.30),
                    bar: [grey(0.96), grey(0.82)],
                    edge: black(0.34),
                    grip: black(0.55),
                    grip_highlight: white(0.9),
                },
                Appearance::Dark => Self {
                    line: black(0.70),
                    bar: [grey(0.30), grey(0.20)],
                    edge: black(0.75),
                    grip: black(0.85),
                    grip_highlight: white(0.16),
                },
            };
            if variant.is_increased_contrast() {
                Self {
                    line: black((p.line.components[3] + 0.3).min(1.0)),
                    edge: black((p.edge.components[3] + 0.3).min(1.0)),
                    ..p
                }
            } else {
                p
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn light_and_dark_differ() {
            let light = Variant::default();
            let dark = Variant {
                appearance: Appearance::Dark,
                ..light
            };
            assert_ne!(Palette::new(light), Palette::new(dark));
            const { assert!(THICK > THIN) };
        }
    }
}

/// The tab view: a strip of tabs over a bordered box, as in `NSTabView` (task 2.21b). The
/// strip's gel is the segmented control's ([`segmented`]); this holds the box and the
/// strip's inactive look.
pub mod tabs {
    use crate::{Appearance, Color, Variant, black, white};

    /// Corner radius of the box.
    pub const BOX_RADIUS: f64 = crate::radii::CONTROL;
    /// Space between the box's border and the selected tab's content, at the sides and
    /// the bottom.
    pub const PADDING: f64 = 12.0;
    /// Space between the strip's bottom edge and the content.
    pub const PADDING_TOP: f64 = 8.0;
    /// Opacity of a disabled tab view's strip and box.
    pub const DISABLED_OPACITY: f32 = 0.5;
    /// How much the selected tab darkens while the window is inactive (1: not at all).
    pub const INACTIVE_DIM: f32 = 0.92;

    /// Colours of the box.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Palette {
        /// The box's fill.
        pub fill: Color,
        /// The border.
        pub border: Color,
        /// The etched highlight along the bottom, just outside the border.
        pub etch: Color,
    }

    impl Palette {
        /// Colours for `variant`.
        pub fn new(variant: Variant) -> Self {
            let p = match variant.appearance {
                Appearance::Light => Self {
                    fill: black(0.05),
                    border: black(0.26),
                    etch: white(0.7),
                },
                Appearance::Dark => Self {
                    fill: white(0.05),
                    border: black(0.65),
                    etch: white(0.08),
                },
            };
            if variant.is_increased_contrast() {
                Self {
                    border: black((p.border.components[3] + 0.25).min(1.0)),
                    ..p
                }
            } else {
                p
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn light_and_dark_differ() {
            let light = Variant::default();
            let dark = Variant {
                appearance: Appearance::Dark,
                ..light
            };
            assert_ne!(Palette::new(light), Palette::new(dark));
        }
    }
}
