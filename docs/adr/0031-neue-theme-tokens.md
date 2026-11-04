# 0031: `neue-theme`: design tokens selected by a `Variant`

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.3 moves every colour and metric out of neue-paint's materials into a token crate,
so widgets (2.9 onward) and the compositor's chrome draw from one source, with light and
dark, an accent colour (blue, graphite), increased contrast, springs and reduced motion.
The compositor caches rendered title-bar slices keyed on the style, so whatever selects
tokens must be cheap to copy and hash.

## Decision
1. New portable crate `neue-theme`, depending only on `color` (0.3, the crate behind
   `peniko::Color`, so tokens are `peniko::Color` with no conversion). neue-paint depends
   on it and re-exports it as `neue_paint::theme`, plus `Appearance`, `Accent`, `Contrast`
   and `Variant`.
2. **`Variant`** = appearance × accent × contrast: `Copy + Eq + Hash`, and the only input
   to colour tokens. `From<Appearance>` gives blue at standard contrast, so material
   functions take `impl Into<Variant>` and existing `Appearance` callers keep compiling.
   **`Theme`** = `Variant` + the reduced-motion flag.
3. Token kinds:
   - semantic colours (`SemanticColors::new(variant)`) for widgets;
   - **materials table** (`materials::<material>`): per-material palettes as functions of
     `Variant` and the material's own state, plus the material's metrics as constants;
   - metrics, radii and the type scale are `const`s: they do not vary by variant, and
     neue-comp uses title-bar metrics in `const` context;
   - motion: fixed `Duration`s and `Spring` presets (response + damping ratio, with
     unit-mass stiffness/damping). `Motion { reduced }` returns zero durations and `None`
     springs under reduced motion, so callers snap to the end value (task 2.8).
4. What counts as a token: colours, lengths, sizes and durations a designer would tune.
   Constants of an algorithm (noise periods, hash seeds, gradient stop positions,
   proportions of a shape relative to its own radius) stay in the material.
5. Blue at standard contrast is the reference look; its values moved unchanged (existing
   goldens are bit-for-bit identical). Graphite has its own gel and traffic-light palettes
   (all three lights share a blue-grey gel). Increased contrast darkens rims (×0.6) and
   strengthens title-bar separators and title text; linen does not change with accent or
   contrast.
6. `TitleBarStyle.appearance` becomes `TitleBarStyle.variant`; neue-comp's `Chrome` holds
   a `Variant` (default until the appearance settings land in Phase 3).

## Consequences
- New materials and widgets read `neue_theme` tokens; adding a variant axis means
  extending `Variant` and each palette's `match`, which the compiler enforces.
- Goldens cover graphite and increased contrast for the button and the title bar.
- No serde on the theme types yet: `neue-config` gains the appearance settings in Phase 3
  and will map onto `Variant` there.
- The `docs/design/` token reference (task 2.25) documents these values for humans.
