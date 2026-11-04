# 0051: Icon type

- **Status:** Accepted
- **Date:** 2026-11-04
- **Supersedes:** 0050 §6 (icons are an enum for now)

## Context
ADR 0050 gave the source list a closed enum of six glyphs and left "a general icon type
(themes, SVG, colour)" to task 2.20. The collection view (2.20c) shows icons over labels,
and apps need icons of their own (a document type, a service), so the toolkit needs one
type that holds the built-in glyphs and app-supplied vector art.

## Decision
1. **`neue_paint::icon::Icon`** (re-exported from `neue_kit`) is an opaque value: a named
   `Glyph` or a `VectorIcon` behind an `Arc`, so clones are cheap. `From<Glyph>` and
   `From<VectorIcon>` build one. `Glyph` is the old `SourceIcon`, renamed, with the same
   six shapes (the source-list goldens are unchanged); it gains `name()` for assistive
   technology. `Icon` compares by value for glyphs and by identity for vector icons, so
   `Entry` keeps its `PartialEq`.
2. **Vector icons are SVG path data, filled.** `VectorIcon::new(width, height)` sets a view
   box from the origin; `.shape(d, paint)` parses a path's `d` attribute once, with
   `kurbo`'s `BezPath::from_svg`, and keeps the paths. No new dependency: no SVG document
   parser, no strokes, gradients or groups. An app flattens its art to filled paths; richer
   formats can come later as a loader that produces a `VectorIcon`. Bad data is an
   `IconError`, not a panic.
3. **Paints are either "current" or fixed.** `IconPaint::Current { alpha }` takes the
   colour the icon is drawn in (the text colour, the selection's white) with its alpha
   scaled; `IconPaint::Fixed { light, dark }` has its own colour for each appearance, so
   multi-coloured icons have both variants as the design rules ask. Glyphs are always
   "current".
4. **`neue_paint::icon::icon(scene, rect, &Icon, color, variant)`** draws an icon centred in
   a rectangle at the largest size that fits (the view box's aspect is kept). Everything is
   vector, so it is sharp at any scale.
5. **Disabled.** A caller passes the disabled colour as `color`; a glyph follows it. A vector
   icon's fixed colours don't, so neue-kit draws a disabled vector icon at half opacity
   (`widgets::icon::draw`, used by both users below).
6. **`neue_kit::IconImage`** draws an icon at a size (`size`, 16 by default; `label`;
   `Handle::set_icon`), in `colors.label`. For AccessKit it is an `Image` named by its label,
   else by a glyph's name, else unnamed (decorative). Like every widget, it is registered
   for the theme property in `theme.rs`.
7. **`Entry::icon` takes `impl Into<Icon>`.** `SourceIcon` and `source_icon` are removed.
   Colours are still decided by the row (grey, white on the selection), so a source list
   with a `Current` icon looks as before.

## Consequences
- The icon type is in neue-paint, so the compositor and shell can use it without the toolkit.
- Not yet: icon themes and lookup by name (freedesktop or otherwise), raster images,
  strokes, gradients and groups in vector icons, tinting a `Fixed` icon on a selection,
  more glyphs. The collection view (2.20c) adds the icon-over-label item that uses all of
  this.
