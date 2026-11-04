//! One line of text laid out with Masonry's font collection (the driver registers the
//! bundled UI font there), shared by the label and the button.

use std::borrow::Cow;

use masonry_core::core::{BrushIndex, render_text};
use masonry_core::imaging::Painter;
use masonry_core::kurbo::{Affine, Size};
use masonry_core::parley::{FontContext, FontFamily, Layout, LayoutContext, StyleProperty};
use masonry_core::peniko::Color;
use neue_theme::typography;

/// Text sizes from the theme's type scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextStyle {
    /// Controls and body text.
    #[default]
    Body,
    /// Captions and secondary information.
    Small,
    /// Section headings.
    Headline,
    /// Window and pane titles.
    Title,
    /// Large figures, such as a counter's value.
    Display,
}

impl TextStyle {
    pub(crate) fn size(self) -> f32 {
        match self {
            Self::Body => typography::BODY,
            Self::Small => typography::SMALL,
            Self::Headline => typography::HEADLINE,
            Self::Title => typography::TITLE,
            Self::Display => typography::DISPLAY,
        }
    }
}

/// A single unwrapped line, rebuilt when its text or style changes.
pub(crate) struct TextLine {
    text: String,
    style: TextStyle,
    family: Option<&'static str>,
    layout: Layout<BrushIndex>,
    stale: bool,
}

impl std::fmt::Debug for TextLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextLine")
            .field("text", &self.text)
            .field("style", &self.style)
            .finish_non_exhaustive()
    }
}

impl TextLine {
    pub(crate) fn new(text: String, style: TextStyle) -> Self {
        Self {
            text,
            style,
            family: None,
            layout: Layout::new(),
            stale: true,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Sets the text; true if it changed (the caller then requests layout).
    pub(crate) fn set_text(&mut self, text: String) -> bool {
        if self.text == text {
            return false;
        }
        self.text = text;
        self.stale = true;
        true
    }

    pub(crate) fn set_style(&mut self, style: TextStyle) -> bool {
        if self.style == style {
            return false;
        }
        self.style = style;
        self.stale = true;
        true
    }

    /// Overrides the UI family (a font from `Settings::extra_fonts`).
    pub(crate) fn set_family(&mut self, family: &'static str) {
        self.family = Some(family);
        self.stale = true;
    }

    /// Forces a new layout (the font collection changed).
    pub(crate) fn invalidate(&mut self) {
        self.stale = true;
    }

    /// Lays the text out if it changed and returns its size.
    pub(crate) fn size(
        &mut self,
        (fonts, layouts): (&mut FontContext, &mut LayoutContext<BrushIndex>),
    ) -> Size {
        if self.stale {
            let family = format!(
                "\"{}\", sans-serif",
                self.family.unwrap_or(typography::UI_FAMILY)
            );
            let mut builder = layouts.ranged_builder(fonts, &self.text, 1.0, true);
            builder.push_default(StyleProperty::FontSize(self.style.size()));
            builder.push_default(StyleProperty::FontFamily(FontFamily::Source(Cow::Owned(
                family,
            ))));
            builder.build_into(&mut self.layout, &self.text);
            self.layout.break_all_lines(None);
            self.stale = false;
        }
        Size::new(
            f64::from(self.layout.width()),
            f64::from(self.layout.height()),
        )
    }

    /// The first line's baseline, from the top of the layout.
    pub(crate) fn baseline(&self) -> f64 {
        self.layout
            .get(0)
            .map_or(0.0, |line| f64::from(line.metrics().baseline))
    }

    /// Paints the text with its top-left corner at `transform`'s origin.
    pub(crate) fn paint(&self, painter: &mut Painter<'_>, transform: Affine, color: Color) {
        render_text(painter, transform, &self.layout, &[color.into()], true);
    }
}
