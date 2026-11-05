# Materials spec

A **material** is a drawn surface: a recipe of vector layers (or a procedural texture) that
widgets and the compositor paint. Palettes and metrics live in
`neue_theme::materials::<name>` (`crates/neue-theme/src/materials.rs`); the drawing lives in
`neue_paint::materials` (`crates/neue-paint/src/materials/<name>.rs`). Widgets and the shell
get materials through widgets; apps never draw them directly. This document gives the recipe,
the rules every material follows, and the per-material notes. Exact colours are in the code
(each palette field is documented there); the reference values below are for orientation.

## 1. Principles

1. **Vector or procedural, never bitmap.** Layers are fills, gradients, strokes and blurred
   shadows; textures are generated per device scale. Everything stays sharp at scale 1, 1.5, 2
   and beyond. Linen is the only procedural texture today; it uses integer hashing and smooth
   value noise (only `+`, `*`, `floor`), so it is identical on every machine.
2. **Light and dark are both drawn.** Dark is a separate palette, not an inversion. A test
   (`every_material_has_a_distinct_dark_variant`) fails if one is missing.
3. **Light comes from above.** Gradients run top to bottom; highlights sit on the top edge,
   shadows and etched lines below; gloss covers the upper half.
4. **Edges do the work.** A rim (1 px, darker than the body), an *etched highlight* (a light
   line 1 px below the control, as if the surface were engraved) and a soft drop shadow
   separate a control from its background. Increased contrast darkens the rim ×0.6 and
   firms up lines.
5. **State changes colour, not layout.** Pressed darkens the gel (`PRESSED_DIM` 0.86, 0.75
   for the text-field cancel button); disabled draws the whole control at 50% opacity
   (`DISABLED_OPACITY`); an inactive window draws bars and tab strips in neutral, lighter
   tones. Controls keep their layout size between states (the overlay scroller's hover widening is drawn inside its fixed 15 px strip).
6. **The accent marks what is active:** default button, selected segment or tab, checked
   toggle, progress fill, focused selection. Accent gel fades in over the neutral gel
   (`Palette::mix_with`), so toggling is a cross-fade, not a swap.
7. **Geometry is shared.** Where hit-testing must agree with painting (menus, sidebars,
   steppers, sliders, popovers, calendars, tables), `neue-paint` exports the geometry
   functions (`segment_edges`, `popover_origin`, `calendar_layout`, ...) and the toolkit uses
   them. The caller lays out and draws text; the material draws chrome.
8. **Tokens, not literals.** Colours, lengths, durations and opacities come from the
   materials table. What stays in the drawing code is algorithm: gradient stop positions,
   noise periods, hash seeds, shape proportions relative to the shape's own size.

## 2. Gel: the shared control recipe

Push buttons, pop-up caps, segmented controls, tabs, toggles, steppers and slider knobs share
one recipe, `aqua_button::Palette`:

| Layer (bottom to top) | Light blue reference | Notes |
|---|---|---|
| Drop shadow | black @ 28%, 0.5 px below, blur 0.75 | Pressed: 20% |
| Etched highlight | white @ 55%, 1 px below | Dark: white @ 7% |
| Rim | gradient `#3D61A8` to `#24428A` | 1 px; darker in pressed |
| Gel body | 4 stops: top, above midline, below midline, bottom (`#BDDEFC`, `#82B8F5`, `#5494ED`, `#A1D4FF`) | Hard midline between stops 2 and 3 |
| Gloss | white @ 55% to 8% over the upper half | Inset 1 px sides, 0.5 px top |

- **Palettes:** blue and graphite × light and dark × rest and pressed (`Palette::new`), the
  neutral `Palette::plain` (light: `#FFFFFF`, `#F7F7F7`, `#E8E8E8`, `#FAFAFA` gel with grey
  rim; dark: greys 0.42 to 0.29), and derived forms: `pulsed` (default button, see motion.md),
  `mix_with` (neutral to accent), `dimmed` (neutral control held down).
- **Neutral at rest, accent when held:** every push button takes the accent while pressed;
  only the default button is accent at rest and pulses.
- The accent gel is the same family in graphite, blue-grey instead of blue.

## 3. Translucency and backdrops

Menu bar, dock shelf and HUD are translucent (ADR 0039). Palettes are defined once, with
alpha. `Backdrop` chooses how they are drawn:

| `Backdrop` | Meaning |
|---|---|
| `Opaque` (default, fallback) | The tint is flattened onto a nominal grey, so the surface is opaque. Nominal greys: menu bar 0.62 light, 0.15 dark; dock shelf 0.55, 0.14; HUD 0.50, 0.15. |
| `Translucent` | The compositor blurs what is behind the surface and the tint is drawn over it (Phase 3, backdrop-blur protocol, ADR 0002 §7). |

- **Increased contrast forces `Opaque`** (`Backdrop::for_variant`), whatever was requested,
  and firms up edges, rims and text.
- Accent never tints these three materials.
- Known limit: the opaque fallback can look slightly off over very light or dark wallpapers.

## 4. Per-material notes

Metrics are logical pixels. "Ref" lists the reference (light, blue, standard) values only
where they carry the design.

### Window chrome and bars
| Material | Recipe and rules |
|---|---|
| **Linen** (`linen`) | Woven cloth backdrop (planned for Notification Center, login and the Overview background, PLAN.md §4). 128 px tile, repeats seamlessly. Weft and warp threads each get a hash tone plus slow (16 px period) and fast (4 px period) value noise. Light: base `(0.785, 0.788, 0.780)` contrast 0.11; dark: `(0.224, 0.236, 0.259)` contrast 0.075. Ignores accent and contrast. The seed (`"neue"`) varies the weave. |
| **Title bar** (`title_bar`) | Brushed-grey strip, 22 px, top corners 5 px. Gradient fill, 1 px top highlight, 1 px bottom separator, centred embossed 13 pt title (baseline 15, shadow 1 px lower). Three traffic lights, 12 px, 20 px pitch from x = 8, each a small gel disc with rim and etch; while the pointer is over the lights all three show a glyph (2.6 px reach, 1.5 px stroke). Focused ref: fill `#E8E8E8` to `#C7C7C7`, title `#2A2A2A`; unfocused: lighter fill, title `#858585`. Graphite recolours all three lights blue-grey. The compositor draws it from three slices (left 68 px with corner and lights, a 1 px stretchable middle, right 8 px) so resize never re-renders. Toolkit windows draw `window_chrome`: the same bar at any height (a unified title bar and toolbar under one gradient, items inset 8, bottom margin 6), ADR 0055. |
| **Menu bar** (`menu_bar`) | 22 px translucent gradient strip, 1 px top highlight, 1 px bottom line, 13 pt embossed titles (baseline 15). Light fill `#F7F7F7` to `#D6D6D9` @ 80%. Dark fill `#45454A` to `#242429` @ 76%. |
| **Dock shelf** (`dock_shelf`) | A pane of glass. In perspective a trapezoid 24 px deep whose far edge is inset 18 px at each end; flat (left and right docks) a rounded rectangle, radius 8, which callers rotate. Layers: glass gradient (far to near), sheen over the far 45%, 1 px edge highlights on top and sides, dark 1 px line on the near edge. A white tint, far to near edge: light 20% to 52%, dark 7% to 20%. |
| **HUD** (`hud`) | Dark rounded panel (radius 8): mostly opaque black gradient (86% to 88% in dark; 80% to 82% in light), gloss over the top 45%, 1 px light rim, drop shadow 3 px down with blur 5 (clipped outside the panel so it does not darken the translucent body). **Dark in both appearances**; the dark variant is deeper with a brighter rim (white @ 36%) and stronger shadow. HUD windows add a 19 px title with a 13 px close disc 6 px from the edge. |
| **Toolbar** (`toolbar`) | Items: 24 px icons, 8 x 4 padding, 2 px label gap, 4 px spacing, 16 px fixed space. Pressed item: a pill (radius 4). Group separator: 24 px tall, 2 px wide. |

### Buttons and toggles
| Material | Recipe and rules |
|---|---|
| **Push button** (`aqua_button`) | The gel in §2. Radius 4. Regular height 22. |
| **Bevel button** (`bevel_button`) | Square-ish grey button; sinks in (3 px inner shadow) when pressed or on. Toolbars and inspectors. |
| **Recessed button** (`bevel_button::recessed`) | Pill (radius 11). No chrome at rest; a dark pill with a 3 px inner shadow when on. |
| **Toggle** (`toggle`) | Checkbox (14 px box, radius 3) and radio (14 px disc): neutral gel fading to accent gel with a white mark, 2 px stroke (check, dash) or a dot of 0.4 of the size, with a 1 px shadow under the mark. Pressed dim 0.86. |
| **Segmented** (`segmented`) | Segments in one rounded gel body (radius 4), 1 px dividers. The selected segment is accent gel and moves between segments with the (fractional, animated) selection. |
| **Pop-up / pull-down** (`popup`) | Neutral gel with a 20 px accent cap on the right carrying the arrows (up and down for pop-up, down for pull-down); 6 x 4 arrows. Combo box: a small gel button inset 1 px inside the field's right end (18 px). Open dim 0.86. |
| **Stepper** (`stepper`) | Two gel halves 13 x 22, 1 px divider, 6 x 4 arrows. |
| **Tabs** (`tabs`) | A bordered box (radius 4, padding 12, top 8) and a gel strip of tabs over its top edge, selected tab accent (neutral and 0.92 dimmed while the window is inactive). |
| **Path control** (`path_control`) | Text segments separated by 4 x 8 chevrons (1.5 px stroke, 12 px slot). Hover gives a soft rounded fill (radius 3, inset 2), darker while held; keyboard focus draws the focus ring. |
| **Colour well** (`color_well`) | A bevel button (46 wide) holding a swatch inset 3 px (radius 2) over a 4 px checkerboard that shows opacity. The panel: 220 wide, 120 high hue and saturation area (radius 2) with a knob of radius 5.5, 72 px labels, 22 px rows. |

### Fields, ranges and progress
| Material | Recipe and rules |
|---|---|
| **Text field well** (`text_field`) | 1 px border (darker along the top), content background, 2 px inner shadow under the top edge. Search fields are pills with a magnifier (7 px lens, 1.5 px stroke) and a 13 px cancel button while they hold text. Caret 1 px. |
| **Date picker** (`date_picker`) | Textual: a field well with day, month, year segments (radius 2) and a stepper 4 px away. Graphical: a framed (radius 3) month grid, header 26 px with previous, today, next buttons (18 px), weekday row 18 px, cells 24 x 20 (radius 3), today ringed (radius 4, 1.5 px). |
| **Groove** (`groove`) | A recessed channel with 1 px rim and 1 px etched highlight. The trough of sliders and progress bars. |
| **Slider** (`slider`) | Groove 5 px tall, round neutral gel knob 15 px. |
| **Progress** (`progress`) | 12 px groove filled with accent gel. Indeterminate: full, with diagonal barber-pole stripes (20 px period) moving right. |
| **Spinner** (`spinner`) | 12 spokes (length 0.5 of the radius, width 0.12 of the diameter), newest dark and the rest fading to 15% opacity, turning clockwise in twelve steps. |
| **Scroller** (`scroller`) | Overlay: a thin translucent knob (7 px, inset 2, min length 18) over the content; with the pointer over it, it widens to 11 px and a track with a 1 px edge appears. Fades out after idle (motion.md). |

### Containers and lists
| Material | Recipe and rules |
|---|---|
| **Menu** (`menu`) | Rounded panel (radius 5) with 1 px rim and soft drop shadow (4 px down, blur 5), 4 px vertical padding, 19 px items, 10 px separators (1 px line), accent band behind the highlighted item, check marks (9 px, 1.75 stroke) and submenu arrows. Label inset 20, trailing 14, min width 120, submenu overlap 2. |
| **Popover** (`popover`) | Menu-like panel (radius 5, rim 1) with a 22 x 11 arrow pointing at the anchor, padding 12, shadow 3 px down with blur 6. The box is the panel plus the arrow strip; it sits on an `Edge` of the anchor with its tip 1 px away, lined up with the anchor's centre where the corners allow. |
| **Sheet** (`sheet`) | Square-cornered panel in the window's background colour that comes out under the title bar: the bar's shadow falls on its top edge (8 px inner shadow), a hairline rim runs down its sides and bottom. In-window sheets cast a drop shadow (3 px down, blur 7); as a window of its own the compositor draws it. Padding 20, button row gap 20, buttons 12 apart. |
| **Table** (`table`) | Header bar 17 px (light gradient, 1 px dividers, darker pressed title, 7 x 5 sort triangle); rows 19 px; hairline border. Rows, stripes and selection are flat fills in the semantic colours. Outline indent 14, disclosure triangle 9 x 6. |
| **Sidebar / source list** (`sidebar`) | Tinted background, 22 px rows, selected row a gradient (accent when focused, grey otherwise), 15 px count badges, 16 px icons, 14 px indent, 1 px edge. |
| **Collection** (`collection`) | An icon over a label in a grid of cells (icon 48 px by default, cell inset 8). A selected item's icon sits on a rounded backdrop (radius 8, 4 px padding; black @ 14% in light, white @ 20% in dark, fainter without keyboard focus) and its label on a 17 px pill (6 px padding). |
| **Split divider** (`split`) | Thin: 1 px line. Thick: 9 px bar with a grip of 3 dots (2 px, 4 px pitch). |
| **Focus ring** (`focus_ring`) | A 3 px band of `SemanticColors::focus_ring` hugging the control's outline from outside. Every focusable control uses it; nothing else signals focus. |

## 5. Adding a material
1. Add `pub mod <name>` to `neue-theme/src/materials.rs`: metrics as documented consts,
   a `Palette::new(variant, ...)` with an arm for every appearance, accent and contrast
   (translucent ones also take a `Backdrop` and go through `on_backdrop`).
2. Add the drawing to `neue-paint/src/materials/<name>.rs` and export it in `mod.rs`. Take
   a `Scene` and a `Variant`; read tokens only from `neue_theme`.
3. Add goldens: states × light/dark × scale 1 and 2 (translucent ones also opaque,
   translucent and increased contrast). Review the diffs before blessing.
4. Show it in the widget gallery (a `Specimen` tile for pure materials).
5. If it is a new surface type, extend this document and write an ADR for the decision.
6. Check it against §1: no bitmap, no literal colour, no state that changes the control's
   layout size, no light-only palette.
