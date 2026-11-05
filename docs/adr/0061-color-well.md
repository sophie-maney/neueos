# 0061: Colour well: a swatch that opens a panel in a popover; the panel keeps HSB

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.24b. Mountain Lion's colour well is a bevelled button holding a swatch; pressing it
opens the system colour panel. The kit has popovers (ADR 0056) and HUD windows (ADR 0059),
the slider and the text field, and needs a well with a panel that sends colours while the
user drags, works from the keyboard and has AccessKit values.

## Decision
1. **The panel is a popover, not a HUD window** (decided in the 2.24 plan: HUD windows have
   no dark controls yet). The well opens it itself, on its own press, Space or Return,
   through `popover::show` in a `mutate_later` on itself; pressing the well again closes
   it, as for any anchor (ADR 0056). The app sees none of this: only `on_change(Color)`.
2. **The panel is built from kit widgets.** `ColorPanelWidget` holds a new
   `HsAreaWidget` (hue across, saturation down, brightness darkens it), the ordinary
   `SliderWidget` for brightness and opacity and `FieldWidget` for the hex value, and
   three `Label`s, and lays them out in fixed rows. It listens to their actions in
   `on_action`, which Masonry offers to a widget's ancestors before the app, and stops
   them there (`set_handled`).
3. **The panel owns the colour while it is open, as HSB.** A colour alone loses the hue of
   a grey and the hue and saturation of black, so dragging brightness to 0 and back up
   would lose the colour. After each change the panel updates the controls that did not
   cause it and the well (`mutate_later` on each), and the well sets its swatch and emits
   `Changed(Color)`. Typing in the hex field doesn't rewrite the field; Return does (or
   puts the colour back if the text is not a colour).
4. **Hex** is `RGB`, `RRGGBB` or `RRGGBBAA`, with or without `#`. Six digits keep the
   current opacity. The well's AccessKit value is `#RRGGBB`, or `#RRGGBBAA` when not
   opaque. The HSB and hex maths are pure functions in `neue-paint`
   (`materials::{Hsb, color_hex, parse_hex}`) with round-trip unit tests.
5. **Looks.** The well is the bevel button with a swatch inset, over a checkerboard so
   opacity shows; the area is drawn from two gradients (hue stops across, a fade to the
   brightness's grey down), which is exact because hue is piecewise linear between the six
   cube corners. Tokens in `neue_theme::materials::color_well`.
6. **Small additions to existing widgets:** `Slider::label` and `TextField::label` (the
   accessible name), which the panel needs and every other user wants.

## Consequences
- The app can't restyle or replace the panel; one panel for all wells.
- `Handle::set_color` while the panel is open changes the swatch only; the panel keeps
  what it had until it is reopened.
- The well isn't drawn pressed while its panel is open (a popover doesn't tell its owner
  when it closes).
- The area is keyboard operable (arrows) and shows as a slider to assistive technology, with
  hue as its number and both values in its text.
