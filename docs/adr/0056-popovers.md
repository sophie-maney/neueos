# 0056: Popovers: a kit layer that tells the Wayland driver its anchor

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.23a adds popovers: a panel with an arrow pointing at the control it opened from,
which closes when the user clicks elsewhere. PLAN §3.2 maps them to xdg-popups. The kit
already shows menus as Masonry layers, which the Wayland driver turns into xdg-popups
(ADR 0034 §1, ADR 0045). The driver derives a menu's `xdg_positioner` from Masonry's
`LayerType` and from where the layer was put. Neither says "this is a popover, on that
side of that control". The compositor may also flip or slide a popup, and the popover
then has to point its arrow back at the control. Masonry's `LayerType` has no popover
variant, and the Masonry fork is bumped only in its own PR (Cargo.toml).

## Decision
1. **API.** `Cx::show_popover(&anchor_handle, Popover::new(content).edge(Edge::Bottom)
   .on_close(msg))` and `Cx::close_popover(&anchor_handle)`. A popover is a Masonry layer
   attached to the anchor widget, so there is at most one per anchor, and it goes away
   with the anchor. `on_close` is sent when the popover closes by itself (Escape, a press
   outside it, the window losing focus, the compositor dismissing it, its anchor pressed
   again), not for `close_popover`. `Edge` is re-exported from `neue_paint::materials`.
2. **Contract through Masonry properties, not a new `LayerType`.** neue-platform defines
   two properties (`neue_platform::{PopoverAnchor, PopoverLanded}`):
   - The kit puts `PopoverAnchor(Some(PopoverRequest { anchor, side, reach }))` on the
     layer's root. `anchor` is the anchor's rectangle in window coordinates, grown by the
     arrow's gap. `side` is the side the popover wants. `reach` is how far its shadow
     paints beyond its box (the same on every side).
   - The Wayland driver reads it when it opens the popup. The window geometry is the box
     without the shadow (`xdg_surface.set_window_geometry`). The positioner anchors to
     the middle of `side` of the anchor rectangle, with gravity away from it and **no
     offset**. It allows flipping on that axis and sliding on both. A popup with
     a window geometry is the standard way; the offset is not used because Smithay flips
     a positioner without inverting its offset, so an offset would push a flipped popover
     off its anchor. Popups nested on a popover (a menu from a pop-up button inside it)
     are placed relative to its window geometry.
   - After each configure, the driver moves the Masonry layer to where the compositor put
     the popover and sets `PopoverLanded(Some(origin))` on the root. The popover then
     works out the side it is actually on and where along that side its arrow points
     (`neue_paint::materials::popover_landing`). It re-lays out its content clear of the
     arrow's strip.
   - Drivers that paint layers into the window (winit; the test harness) do neither. The
     popover places itself on the side it asked for, centred on the anchor, once it
     knows its size.
   neue-platform does not depend on neue-paint, so the driver contract has its own
   `Side`, which the kit maps from `Edge`.
3. **Closing on the anchor.** With the seat grab on Wayland, the compositor dismisses a
   popover on a press outside it, including on its anchor, and that press never reaches
   the window. In a window (winit), Masonry delivers every press to the window too, so a
   button that shows a popover would reopen it on release. Instead, a press on the
   anchor is remembered, and the next `show_popover` for that anchor closes the popover.
   A popover ignores presses outside it while keyboard focus is outside it, because
   then a menu opened from its content owns those presses.
4. **Material.** `neue_paint::materials::popover` draws a rounded panel with the arrow
   spliced into one side of the outline. It has a drop shadow (outside the outline only),
   a top highlight and a rim, in light and dark variants (tokens in
   `neue_theme::materials::popover`). The panel's and the window's geometry
   (`popover_body`, `popover_origin`, `popover_landing`) live with the material, shared
   by the kit and its tests.

## Consequences
- No change to the Masonry fork, the protocols or the compositor.
- A popover is told where it landed only after the first configure. Until then it is drawn
  on the side it asked for. The first frame of a flipped popover is drawn before that
  configure, so it can show the arrow on the wrong side for one frame.
- On macOS a popover is clipped to the window, like menus; it does not flip there.
- Tab moves from the popover into its content and on into the window after it; focus is
  not trapped in the popover yet.

## Revisit when
- The Masonry fork gains a popover `LayerType`, or a way to pass placement hints with a
  layer.
- Popovers should follow their anchor when the window re-lays out while one is open
  (today it stays where it opened).
- Detachable popovers or a transient/semi-transient behaviour choice are needed.
