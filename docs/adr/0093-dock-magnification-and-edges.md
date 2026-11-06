# 0093: Dock magnification and the left/right edges: a fisheye layout, room kept at the ends, an input region of the dock at rest, icons reported by the widget

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.17a: hover magnification and the dock on the left and right edges, with `[dock]`
config for the edge, the icon size and magnification. ADR 0092 made the layout a pure
function of the tile count (`dock_size`, `dock_icon_rect`), the shell work out minimise
rectangles from it, and the surface let the pointer through a strip at its top
(`LayerSpec::pass_through_top`). Magnification makes the layout depend on the pointer, and
the side edges need the pass-through room on another side.

## Decision
1. **`neue_kit::desktop::DockLayout`** (`edge`, `icon`, `magnified: Option<f64>`) replaces
   `dock_size` and `dock_icon_rect`. It works along the dock (`u`) and away from the screen
   edge (`v`) and turns that into surface coordinates per `DockEdge` (Bottom, Left, Right).
   `size(count)`, `band()` (the dock at rest from the screen edge: exclusive zone and input
   depth), `pass_through()`, `rest(count)` and `place(count, pointer, amount)` →
   `DockPlacement { icons, … }`. The default is the 3.16 dock pixel for pixel.
2. **The curve.** An icon grows by `amount × (magnified − icon) × bell(d)`, `d` its rest
   distance from the pointer in tiles, `bell` a raised cosine reaching `MAGNIFY_REACH` (3)
   tiles. With a whole reach, the bells over a long row sum to the reach wherever the
   pointer is, so the dock grows by at most `MAGNIFY_REACH` icons' growth.
3. **A fisheye of the dock at rest.** The tiles are laid out end to end at their sizes and
   shifted so the pointer keeps its share of the tile whose rest place it is in (at an end,
   that end stays put). The layout is continuous in the pointer; the pointer is over the
   magnified dock exactly where it is over the dock at rest, and over the tile whose rest
   place it is in. Hit-testing uses the magnified stretches, which agrees.
4. **The surface keeps room** for the dock to grow at both ends (the growth can all go one
   way) and beyond the icons for a magnified icon and the bounce. It does not resize while
   the pointer moves. **`LayerSpec::pass_through: Margin`** replaces `pass_through_top`:
   the input region is the content less those margins, here the dock at rest. That is all
   the pointer needs, by (3).
5. **Smoothing.** Only the amount is sprung (`MAGNIFY` = `Spring::SNAPPY`): in on entering,
   out on leaving, the curve shrinking where the pointer left it. Along the dock the icons
   follow the pointer at once. Reduced motion turns magnification off. Leaving the window
   reaches the dock as `Update::HoveredChanged(false)` (Masonry delivers `Leave` to no
   widget).
6. **The widget reports where the icons are** (`Dock::on_placed`: key and rect per tile)
   from its layout pass whenever they change; the shell sends minimise rectangles from
   that, falling back to `rest()` until the first report. The tile focus children follow
   the magnified rects, so the focus ring and AccessKit bounds do too.
7. **Left and right.** A flat shelf (`ShelfStyle::Flat`) as deep as the icon plus
   `FLAT_INSET` (8 px) on both sides, its outer corners past the screen edge. Shelf and
   running lights are drawn as at the bottom and put in place by a quarter turn (left) or
   its mirror (right); icons stay upright. No reflections on a flat shelf. The icons bounce
   away from the edge. The surface is anchored to that edge only (centred in the work
   area), with the band as exclusive zone.
8. **Size.** The bounce height, still lift and bounce room are for 48 px icons and scale
   with the icon. Icons are loaded at the largest size they grow to.
9. **Config** (`shell.toml` `[dock]`): `edge` (`bottom`, `left`, `right`), `size` (16–128,
   default 48), `magnification` (default `false`), `magnified-size` (16–256, default 96).
   Read when the shell starts.

## Alternatives considered
- **Resizing the surface as icons grow** (`fit_content` following the magnified dock): a
  configure round trip per frame, and a centred surface would shift under the pointer.
- **Growing about the dock's centre:** icons slide under a still pointer, which then sits
  over another icon; the fisheye keeps the pointer where it was.
- **An input region following the magnified dock:** unneeded by (3), and would need the
  region updated per frame.
- **Rotating the icons with the shelf:** icons must stay upright.
- **Sending minimise rectangles only at rest:** the task asks that they follow
  magnification; Phase 4's genie targets the icon where it is.

## Consequences
- `LayerSpec::pass_through_top` is gone; `Anchor` gains `LEFT` and `RIGHT`.
- `neue_kit::desktop` re-exports kurbo's `Rect`, `Size` and `Insets` for the dock's API.
- While the pointer moves over a magnified dock the shell sends a minimise rectangle per
  window of each moving tile per frame (a handful of small requests).
- With magnification on, the bottom dock's surface is wider than the shelf by
  `2 × 3 × (magnified − icon)` (288 px at the defaults), all of it pass-through.

## Not done
- Changing the edge or size while the shell runs (it reads `[dock]` at start).
- Name labels over hovered icons; the dock's keyboard focus (ADR 0092).
