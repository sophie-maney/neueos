//! Design tokens for neueOS.
//!
//! Everything a designer tunes lives here, so widgets and materials never hard-code it:
//! - [`colors`]: semantic colours ([`SemanticColors`]) for every [`Variant`];
//! - [`metrics`] and [`radii`]: spacing, control sizes and corner radii (logical pixels);
//! - [`typography`]: the UI family and the type scale (points = logical pixels);
//! - [`motion`]: durations, spring presets and the reduced-motion switch;
//! - [`materials`]: the values each neue-paint material is drawn from.
//!
//! Colours depend on the [`Variant`] (appearance × accent × contrast). Metrics, radii and
//! the type scale do not, so they are plain constants. Constants that belong to an
//! algorithm rather than to the look (noise periods, hash seeds, proportions of a shape
//! relative to its own size) stay with the material that uses them.
//!
//! Design decisions: ADR 0031. The human-readable token reference arrives with task 2.25
//! (`docs/design/`).
//!
//! Portable crate: must build and pass tests on macOS and Linux.

pub mod colors;
pub mod materials;
pub mod metrics;
pub mod motion;
pub mod radii;
pub mod typography;

pub use colors::SemanticColors;
pub use materials::Backdrop;
pub use motion::{Motion, Spring, SpringValue};

/// An sRGB colour with straight (unpremultiplied) alpha; the same type as `peniko::Color`.
pub type Color = color::AlphaColor<color::Srgb>;

/// Light or dark appearance. Every material has both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Appearance {
    /// The default, light appearance.
    #[default]
    Light,
    /// The dark appearance.
    Dark,
}

/// The accent colour: tints default buttons, selection, focus rings and (in graphite) the
/// window buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Accent {
    /// The classic blue gel.
    #[default]
    Blue,
    /// Blue-grey, for a quieter desktop.
    Graphite,
}

/// Contrast level of edges and secondary text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Contrast {
    /// The default look.
    #[default]
    Standard,
    /// Darker (light) or brighter (dark) edges, separators and secondary text.
    Increased,
}

/// The discrete settings that select colour tokens. Small, `Copy` and `Hash`, so renderers
/// can key caches on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Variant {
    /// Light or dark.
    pub appearance: Appearance,
    /// Accent colour.
    pub accent: Accent,
    /// Contrast level.
    pub contrast: Contrast,
}

impl Variant {
    /// The variant for `appearance` with the default accent and contrast.
    pub const fn new(appearance: Appearance) -> Self {
        Self {
            appearance,
            accent: Accent::Blue,
            contrast: Contrast::Standard,
        }
    }

    /// The same variant with `accent`.
    #[must_use]
    pub const fn with_accent(self, accent: Accent) -> Self {
        Self { accent, ..self }
    }

    /// The same variant with `contrast`.
    #[must_use]
    pub const fn with_contrast(self, contrast: Contrast) -> Self {
        Self { contrast, ..self }
    }

    /// Whether this is the dark appearance.
    pub const fn is_dark(self) -> bool {
        matches!(self.appearance, Appearance::Dark)
    }

    /// Whether increased contrast is on.
    pub const fn is_increased_contrast(self) -> bool {
        matches!(self.contrast, Contrast::Increased)
    }
}

impl From<Appearance> for Variant {
    fn from(appearance: Appearance) -> Self {
        Self::new(appearance)
    }
}

/// The user's theme: the colour [`Variant`] plus the reduced-motion preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Theme {
    /// Selects colour tokens.
    pub variant: Variant,
    /// Replace movement with instant changes (see [`Motion`]).
    pub reduced_motion: bool,
}

impl Theme {
    /// Semantic colours for this theme.
    pub fn colors(&self) -> SemanticColors {
        SemanticColors::new(self.variant)
    }

    /// Motion tokens, honouring reduced motion.
    pub fn motion(&self) -> Motion {
        Motion {
            reduced: self.reduced_motion,
        }
    }
}

/// An opaque colour from sRGB components in 0..1.
pub(crate) const fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::new([r, g, b, 1.0])
}

/// A colour from sRGB components and alpha in 0..1.
pub(crate) const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new([r, g, b, a])
}

/// An opaque grey.
pub(crate) const fn grey(v: f32) -> Color {
    Color::new([v, v, v, 1.0])
}

/// White at alpha `a`.
pub(crate) const fn white(a: f32) -> Color {
    rgba(1.0, 1.0, 1.0, a)
}

/// Black at alpha `a`.
pub(crate) const fn black(a: f32) -> Color {
    rgba(0.0, 0.0, 0.0, a)
}

/// `color` with its RGB components multiplied by `factor` (alpha kept).
pub(crate) fn darken(color: Color, factor: f32) -> Color {
    let [r, g, b, a] = color.components;
    Color::new([r * factor, g * factor, b * factor, a])
}

/// The colour `t` of the way from `a` to `b` (0: `a`, 1: `b`), alpha included.
pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.components, b.components);
    Color::new(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_interpolates_every_component() {
        let c = mix(rgba(0.0, 1.0, 0.5, 0.0), rgba(1.0, 0.0, 0.5, 1.0), 0.25);
        assert_eq!(c.components, [0.25, 0.75, 0.5, 0.25]);
    }

    #[test]
    fn appearance_converts_to_the_default_variant() {
        let v = Variant::from(Appearance::Dark);
        assert!(v.is_dark());
        assert_eq!(v.accent, Accent::Blue);
        assert!(!v.is_increased_contrast());
        assert_eq!(Variant::default(), Variant::new(Appearance::Light));
    }

    #[test]
    fn darken_keeps_alpha() {
        let c = darken(rgba(0.5, 1.0, 0.2, 0.4), 0.5);
        assert_eq!(c.components, [0.25, 0.5, 0.1, 0.4]);
    }
}
