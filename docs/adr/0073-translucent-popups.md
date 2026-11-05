# 0073: Translucent menus and popovers: popup blur regions, sheets stay opaque in the kit

- **Status:** Accepted
- **Date:** 2026-11-05
- **Builds on:** 0039 (translucent materials), 0056 (popovers), 0071 (compositor blur), 0072 (client blur)
- **Amends:** 0072 §4 (popups stay opaque)

## Context
Task 3.5b. ADR 0072 made windows ask for blur and left menus, popovers and sheets opaque,
because they had no translucent palettes and no way to ask. Their panels are xdg-popups
whose surface is bigger than the panel (the shadow lies outside the window geometry), and
a popover has an arrow that must not be blurred.

## Decision
1. **Palettes take a `Backdrop`.** `menu`, `popover` and `sheet` palettes in `neue-theme`
   (and `menu_panel`, `popover`, `sheet` in `neue-paint`) take a `Backdrop`, like the
   HUD's. Opaque is the existing colour, bit for bit (existing goldens unchanged);
   translucent is the same grey at alpha 0.80 to 0.88, and increased contrast is always
   opaque (`Backdrop::for_variant`). Only the body's fill changes: the shadow already
   lies outside the panel, the rim and highlight are unchanged.
2. **Widgets read the backdrop from a default property.** `BackdropProp` joins `ThemeProp`
   as a default property of `MenuWidget` and `PopoverWidget`: translucent when
   `Cx::has_blur()`, else opaque. `Cx` builds it with the theme (window open, theme
   change), so a menu needs no state of its own.
3. **A layer asks for blur with a `PopupBlur` property** on its root
   (`neue-platform::PopupBlur(Option<PopupBlurRequest { radius, arrow }>)`), next to
   `PopoverAnchor`. The kit puts it on every menu (radius, no arrow) and popover (radius,
   arrow depth) unconditionally; the driver acts only when the compositor offers blur. The
   kit cannot know whether the popup will be blurred at the time the widget is made, and
   asking costs nothing when blur is unavailable.
4. **The driver blurs the box, not the surface.** A popup's surface is its box plus the
   shadow. The region is the window geometry (`Placement::blur_box`): the box at the
   insets' offset, corners cut round with the same one-row strips as windows (ADR 0072
   §2). For a popover the arrow strip, `arrow` deep on the side facing the anchor, is left
   out; the side comes from where the compositor put the popup relative to its anchor, so
   a flip moves the cut. The region is set when the popup opens and again on every
   configure (size or flip). `WindowBlur` gained `set_region` with an origin and releases
   its effect object on drop, so popups clean up with their surface.
5. **Sheets get the palette and goldens, not a client change.** An in-window sheet is
   painted over its own window's content with nothing blurred behind it, so the kit's
   `SheetWidget` keeps painting `Backdrop::Opaque`. A sheet that is its own modal window
   could ask like a HUD does; no app needs it yet.
6. **Goldens:** `MaterialTile` gains `Menu`, `Popover` and `Sheet`, so the existing
   material golden test covers them: opaque, translucent and increased contrast, light
   and dark, 1x and 2x (36 images), plus the gallery's Specimen goldens (24).

## Consequences
- On neueOS, menus and popovers are translucent over a blur; on macOS, under other
  compositors and under increased contrast they keep the opaque tint.
- As with windows (ADR 0072), a popup opened before contrast became increased keeps its
  blur request and paints opaque; popups are short-lived, so this is rarely visible.
- Menus opened by Masonry itself (not neue-kit's `MenuWidget`) carry no `PopupBlur` and
  stay as they were.

## Alternatives considered
- **The kit decides per popup** (reads `has_blur` when it makes the layer): needs `Cx` in
  the menu code paths that Masonry calls (selector, submenus), which have none.
- **Blur the whole popup surface:** blurs under the shadow and, for popovers, the arrow's
  empty corner; the arrow would show a rectangular halo.
