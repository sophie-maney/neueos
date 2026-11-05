//! Single-line text: parley layout over bundled fonts, drawn with [`Scene::draw_text`].
//!
//! Only bundled fonts are used (no system font discovery), so text renders the same on
//! every host and golden tests stay deterministic. The UI font is DejaVu Sans Book (PLAN §4,
//! open question 2), under the Bitstream Vera licence (`fonts/LICENSE-DejaVuSans.txt`,
//! ADR 0038).
//!
//! [`Scene::draw_text`]: crate::Scene::draw_text

use std::fmt;
use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use vello_common::peniko::FontData;

/// Family name of the bundled UI font.
pub const UI_FAMILY: &str = neue_theme::typography::UI_FAMILY;
/// The bundled UI font (DejaVu Sans Book, Bitstream Vera licence; ADR 0038).
static UI_FONT: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
/// The bundled UI font's data, for text stacks other than [`TextEngine`] (neue-kit's
/// widgets, through Masonry's font collection).
pub fn ui_font_data() -> &'static [u8] {
    UI_FONT
}

/// The bundled UI font's bold face (DejaVu Sans Bold 2.37, same licence; ADR 0068), for
/// the menu bar's app name and other bold text.
static UI_BOLD_FONT: &[u8] = include_bytes!("../fonts/DejaVuSans-Bold.ttf");
/// The bold face's data, registered next to [`ui_font_data`].
pub fn ui_bold_font_data() -> &'static [u8] {
    UI_BOLD_FONT
}

/// Appended to truncated text.
const ELLIPSIS: char = '…';

/// One shaped run of glyphs in a single font.
#[derive(Clone, Debug)]
pub struct GlyphRun {
    /// The font the glyph ids index into.
    pub font: FontData,
    /// Font size in logical pixels.
    pub size: f32,
    /// Glyphs, positioned relative to the line's baseline origin.
    pub glyphs: Arc<[vello_cpu::Glyph]>,
}

/// A laid-out line of text, in logical pixels. Its origin is the left end of the baseline.
#[derive(Clone, Debug, Default)]
pub struct TextLine {
    runs: Vec<GlyphRun>,
    width: f64,
    ascent: f64,
    descent: f64,
    truncated: bool,
}

impl TextLine {
    /// Shaped runs, in visual order.
    pub fn runs(&self) -> &[GlyphRun] {
        &self.runs
    }

    /// Advance width.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// Distance from the baseline up to the top of the line box.
    pub fn ascent(&self) -> f64 {
        self.ascent
    }

    /// Distance from the baseline down to the bottom of the line box.
    pub fn descent(&self) -> f64 {
        self.descent
    }

    /// Whether the text was cut short and ends in an ellipsis.
    pub fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// Lays out text with the bundled fonts. Create one and keep it: it owns the font
/// collection and layout scratch space.
pub struct TextEngine {
    fonts: FontContext,
    layouts: LayoutContext<()>,
}

impl fmt::Debug for TextEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextEngine").finish_non_exhaustive()
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    /// An engine with the bundled fonts registered and nothing else.
    pub fn new() -> Self {
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        collection.register_fonts(Blob::new(Arc::new(UI_FONT)), None);
        collection.register_fonts(Blob::new(Arc::new(UI_BOLD_FONT)), None);
        Self {
            fonts: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            layouts: LayoutContext::new(),
        }
    }

    /// Lay out `text` on one line at `size` logical pixels, in the UI font.
    pub fn line(&mut self, text: &str, size: f32) -> TextLine {
        let layout = self.layout(text, size);
        line_from_layout(&layout, false)
    }

    /// Like [`Self::line`], but if the text is wider than `max_width` it is cut at a
    /// character boundary and ends in an ellipsis. If even the ellipsis doesn't fit, the
    /// line is empty.
    pub fn line_truncated(&mut self, text: &str, size: f32, max_width: f64) -> TextLine {
        let full = self.line(text, size);
        if full.width <= max_width {
            return full;
        }
        // Binary search for the longest prefix that fits with the ellipsis. `cuts` are the
        // byte offsets of char boundaries, so every prefix is valid UTF-8.
        let cuts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let (mut lo, mut hi) = (0usize, cuts.len());
        let mut best = None;
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate = format!("{}{ELLIPSIS}", text[..cuts[mid - 1]].trim_end());
            let layout = self.layout(&candidate, size);
            if f64::from(layout.width()) <= max_width {
                best = Some(layout);
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let best = best.or_else(|| {
            let layout = self.layout(&ELLIPSIS.to_string(), size);
            (f64::from(layout.width()) <= max_width).then_some(layout)
        });
        match best {
            Some(layout) => line_from_layout(&layout, true),
            None => TextLine {
                truncated: true,
                ..TextLine::default()
            },
        }
    }

    fn layout(&mut self, text: &str, size: f32) -> Layout<()> {
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(UI_FAMILY)));
        builder.push_default(StyleProperty::FontSize(size));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }
}

fn line_from_layout(layout: &Layout<()>, truncated: bool) -> TextLine {
    let Some(line) = layout.lines().next() else {
        return TextLine::default();
    };
    let metrics = line.metrics();
    let baseline = metrics.baseline;
    let mut runs = Vec::new();
    for item in line.items() {
        let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
            continue;
        };
        let run = glyph_run.run();
        let glyphs: Arc<[vello_cpu::Glyph]> = glyph_run
            .positioned_glyphs()
            .map(|g| vello_cpu::Glyph {
                id: g.id,
                x: g.x,
                y: g.y - baseline,
            })
            .collect();
        runs.push(GlyphRun {
            font: run.font().clone(),
            size: run.font_size(),
            glyphs,
        });
    }
    TextLine {
        runs,
        width: f64::from(layout.width()),
        // The first line's box starts at y = 0, so the baseline is the ascent.
        ascent: f64::from(baseline),
        descent: f64::from(metrics.line_height - baseline),
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_shapes_text() {
        let mut engine = TextEngine::new();
        let line = engine.line("Hello", 13.0);
        assert!(
            line.width() > 20.0 && line.width() < 60.0,
            "{}",
            line.width()
        );
        let glyphs: usize = line.runs().iter().map(|r| r.glyphs.len()).sum();
        assert_eq!(glyphs, 5);
        assert!(
            line.runs()
                .iter()
                .all(|r| r.glyphs.iter().all(|g| g.id != 0))
        );
        assert!(line.ascent() > 0.0);
    }

    #[test]
    fn truncation_fits_and_ends_in_an_ellipsis() {
        let mut engine = TextEngine::new();
        let text = "A rather long window title that will not fit";
        let full = engine.line(text, 13.0);
        let cut = engine.line_truncated(text, 13.0, full.width() / 2.0);
        assert!(cut.is_truncated());
        assert!(cut.width() <= full.width() / 2.0);
        assert!(cut.width() > full.width() / 4.0);
        let same = engine.line_truncated(text, 13.0, full.width() + 1.0);
        assert!(!same.is_truncated());
        assert_eq!(same.width(), full.width());
        let empty = engine.line_truncated(text, 13.0, 1.0);
        assert!(empty.runs().is_empty());
    }
}
