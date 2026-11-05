# Token reference

Source: `crates/neue-theme` (ADR 0031). Colours depend on the **variant**; metrics, radii and
the type scale are constants. Lengths are logical pixels (one logical pixel is 1 device pixel
at scale 1, 2 at scale 2). Colours are sRGB with straight alpha; hex values below are the
rounded 8-bit form of the 0..1 components in the code.

## 1. The theme: variant and reduced motion

`Theme` = `Variant` + `reduced_motion`. It is the only input to colour and motion tokens.

| Axis | Values | Effect |
|---|---|---|
| `Appearance` | `Light` (default), `Dark` | Selects the whole palette. |
| `Accent` | `Blue` (default), `Graphite` | Tints default buttons, selection, focus rings, links, progress fill, and the traffic lights (all three become blue-grey in graphite). |
| `Contrast` | `Standard` (default), `Increased` | Stronger edges, separators and secondary text; rims darken (×0.6); translucent materials become opaque. |
| `reduced_motion` | `false` (default), `true` | Durations become zero, springs `None`, ambient loops stop (see motion.md). |

`Variant` is `Copy + Eq + Hash` (renderers key caches on it). `Appearance` converts into a
`Variant` (blue, standard). 2 appearances × 2 accents × 2 contrasts = 8 variants.
**Blue, standard contrast, light is the reference look**; everything else derives from it.

What each axis does *not* change: linen ignores accent and contrast; the menu bar, dock
shelf and HUD ignore the accent; the HUD is dark in both appearances.

## 2. Semantic colours (`SemanticColors::new(variant)`)

Widgets use these, never a literal. Alpha colours are written as `colour @ alpha`.

| Token | Use | Light | Dark |
|---|---|---|---|
| `window_background` | Window and sheet background | `#EDEDED` | `#323232` |
| `content_background` | Text fields, tables, editable content | `#FFFFFF` | `#1E1E1E` |
| `alternate_row` | Table row stripe | `#EDF3FE` | white @ 5% |
| `label` | Primary text | black @ 85% | white @ 85% |
| `secondary_label` | Captions, subtitles | black @ 50% | white @ 55% |
| `tertiary_label` | Placeholders | black @ 26% | white @ 25% |
| `disabled_label` | Disabled text | black @ 25% | white @ 25% |
| `label_emboss` | Shadow one pixel below text on gradients | white @ 55% | black @ 55% |
| `separator` | Hairlines between content areas | black @ 10% | white @ 10% |
| `accent` | Links, toggles, default button tint | `#3378E0` | `#4C8FF5` |
| `selection` | Selection in a focused view | `#3875D6` | `#2961BF` |
| `unfocused_selection` | Selection in an unfocused view | `#D4D4D4` | `#4C4C4C` |
| `selected_label` | Text on `selection` | white | white |
| `text_selection` | Selected text in a focused field (text keeps its colour) | `#B2D6FF` | `#33578F` |
| `focus_ring` | Keyboard focus ring | `#4C8FF5` @ 75% | `#5C99FF` @ 75% |
| `shadow` | Drop shadows of windows, menus, popovers | black @ 35% | black @ 60% |

### Graphite accent (replaces these tokens; the rest are unchanged)

| Token | Light | Dark |
|---|---|---|
| `accent` | `#738091` | `#949EB0` |
| `selection` | `#788596` | `#5C6675` |
| `text_selection` | `#CCD1DB` | `#4C5461` |
| `focus_ring` | `#808FA6` @ 75% | `#99A6BA` @ 75% |
| `alternate_row` | `#F0F1F3` | (unchanged) |

### Increased contrast (replaces these tokens)

| Token | Light | Dark |
|---|---|---|
| `label` | black @ 100% | white @ 100% |
| `secondary_label` | black @ 70% | white @ 75% |
| `tertiary_label` | black @ 50% | white @ 55% |
| `disabled_label` | black @ 40% | white @ 45% |
| `separator` | black @ 45% | white @ 45% |

Selection, accent and backgrounds do not change. Materials add their own contrast rules
(darker rims, firmer edges, opaque translucency; see materials.md §3).

### Using colour
- Text is always one of the four label tokens. Do not dim text with opacity.
- Disabled controls draw at `DISABLED_OPACITY` (0.5 for every control material) with
  `disabled_label` for text.
- Selection follows focus: `selection` while the view is focused, `unfocused_selection`
  otherwise (tables, text fields, text views and collection views do this; source lists
  have their own gradients, see materials.md).
- Never use the accent as a fill for large areas. It marks *the thing to act on*: the
  default button, the selected segment, the progress fill, the focused selection.

## 3. Spacing (`metrics::spacing`)

| Token | px | Use |
|---|---|---|
| `XXS` | 2 | Hairline gap, e.g. between a control and its etched highlight |
| `XS` | 4 | Tight gap inside compound controls |
| `S` | 8 | Between related controls |
| `M` | 12 | Between groups of controls; popover padding; sheet button gap |
| `L` | 20 | Window content inset; sheet padding |

## 4. Control sizes (`metrics::control`)

Sizes follow three scales: regular (22), small (19), mini (15) heights.

| Token | px | Meaning |
|---|---|---|
| `REGULAR_HEIGHT` / `SMALL_HEIGHT` / `MINI_HEIGHT` | 22 / 19 / 15 | Push buttons, pop-ups, text fields / small / mini |
| `BUTTON_MIN_WIDTH` | 70 | Push button with a short label |
| `BUTTON_PADDING` | 12 | Rim to label |
| `RIM` | 1 | Outer border of a control |
| `FOCUS_RING` | 3 | Width of the keyboard focus ring |
| `CHECKBOX_SIZE` / `CHECKBOX_GAP` | 14 / 6 | Checkbox side and radio diameter; gap to label |
| `SEGMENT_PADDING` / `SEGMENT_MIN_WIDTH` | 12 / 40 | Segmented control |
| `BEVEL_MIN_WIDTH` | 28 | Bevel and recessed buttons |
| `SLIDER_HEIGHT` / `SLIDER_WIDTH` | 19 / 120 | Slider hit area / default width (knob is 15) |
| `STEPPER_WIDTH` / `STEPPER_HEIGHT` | 13 / 22 | Stepper |
| `PROGRESS_HEIGHT` / `PROGRESS_WIDTH` | 12 / 150 | Progress bar |
| `SPINNER_SIZE` | 16 | Spinner |
| `TEXT_FIELD_WIDTH` / `TEXT_FIELD_INSET` | 200 / 4 | Text, search and secure fields |
| `SEARCH_FIELD_INSET` | 22 | Room for magnifier and cancel button |
| `TEXT_VIEW_WIDTH` / `HEIGHT` / `INSET` | 300 / 120 / 4 | Text view |
| `POPUP_LABEL_INSET` / `POPUP_CAP_WIDTH` | 10 / 20 | Pop-up and pull-down buttons |
| `COMBO_ARROW_WIDTH` | 18 | Combo box arrow button |
| `SCROLLER_WIDTH` | 15 | Overlay scroller strip that takes the pointer |
| `SCROLL_LINE` | 40 | One mouse-wheel line or arrow key |

Per-material metrics (toolbar item size, menu item height, table row height, and so on) are
in the materials table and listed in materials.md.

## 5. Corner radii (`radii`)

| Token | px | Use |
|---|---|---|
| `CONTROL` | 4 | Push buttons, text fields, regular controls |
| `CONTROL_SMALL` | 3 | Small and mini controls, checkboxes |
| `WINDOW` | 5 | Top corners of a window |
| `MENU` | 5 | Menus and popovers |
| `PANEL` | 8 | HUD panels, notification banners, icon tiles |

Sheets, split dividers and table borders are square. Recessed buttons and search fields are
pills (radius = half the height). Nested rounded shapes inset by their gap: an inner radius
is the outer one minus the inset (the path control's is `CONTROL - 1`).

## 6. Typography (`typography`)

UI family: **DejaVu Sans**, bundled by neue-paint (ADR 0013, 0038). Sizes are in points,
which equal logical pixels.

| Token | Size | Use |
|---|---|---|
| `MINI` | 9 | Mini controls, captions |
| `SMALL` | 11 | Secondary labels, small controls |
| `BODY` | 13 | Body text, control labels, menu items |
| `WINDOW_TITLE` | 13 (= `BODY`) | Window titles |
| `HEADLINE` | 15 | Section headings in panes and sheets |
| `TITLE` | 20 | Large titles: onboarding, empty states |
| `DISPLAY` | 28 | Clock and widget numerals |

Labels on gradients (title bar, menu bar, toolbar) carry an **emboss**: the same text drawn
one pixel lower in `label_emboss`, underneath. Plain content text has none.

## 7. Adding or changing a token
1. Colours go in `colors.rs` (semantic) or the material's module in `materials.rs`, as a
   function of `Variant` so the compiler forces every arm; lengths go in `metrics.rs`,
   `radii.rs` or the material.
2. Document the field in the code (every token has a doc comment) **and** in this file.
3. Check light, dark, graphite and increased contrast; update goldens only after reviewing
   the diff (`cargo xtask golden --bless`).
4. A new variant axis is an ADR.
