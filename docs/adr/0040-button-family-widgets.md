# 0040: The button family: checkbox, radio, bevel, recessed, segmented

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.12 adds five controls on the 2.9 pattern (ADR 0037): bevel and recessed buttons,
checkbox, radio button, segmented control. They are the first widgets to move on springs
(ADR 0036) and the first to hold state the user changes, so the model needs rules for who
owns that state.

## Decision
1. **The app owns the state; the widget shows it.** A widget keeps the state it displays,
   flips it when the user acts, and sends a message with the new value. Changes through a
   `Handle` (`set_checked`, `set_selected`, `set_on`) update the widget and send nothing, so
   no feedback loops.
2. **Radio buttons are independent widgets.** A radio button only turns itself on; the
   app turns the others off through their handles (the gallery's Buttons page does).
   There is no group widget, so there is no arrow-key navigation between radio buttons
   yet: each takes Tab focus and Space.
3. **Checkbox** has `CheckState::{Off, On, Mixed}`. Mixed is set by the app only; a click
   turns it on. The message carries a `bool`.
4. **Bevel and recessed buttons are one builder**, `BevelButton`
   (`new` for bevel, `recessed` for recessed): momentary with `on_press`, or a toggle with
   `on_toggle` / `on(true)`. The recessed button draws no chrome at rest; its dark pill
   appears while held or on.
5. **Segmented control** is always single-choice with one segment selected. Click selects;
   Left and Right move the selection while focused (so no separate focus index); the
   accent highlight slides between segments on `Spring::SNAPPY`. Segments share the width:
   natural widths plus an equal share of any extra room.
6. **Paint.** The checkbox, radio button and selected segment reuse the push button's gel
   (`aqua_button::Palette`, now with `mix_with` and `dimmed`; its drawing split into
   `draw` and `gel_body`, output unchanged). Bevel and recessed buttons have their own
   flat palettes in `materials::bevel_button`; the constants for the others live in
   `materials::toggle` and `materials::segmented`. A pressed control shows at once; only
   the on/off and selection changes spring. Reduced motion snaps them (ADR 0036).
7. **Keyboard:** Tab focuses, the focus ring shows in the active window only, a click
   doesn't take focus; Space toggles or presses on release, as the push button does.
8. **AccessKit:** checkbox `CheckBox` and radio button `RadioButton` with `toggled` and the
   `Click` action; bevel and recessed buttons `Button` (with `toggled` when toggles);
   segmented control `RadioGroup` with the selected label as its value and `Increment` /
   `Decrement` actions.

## Consequences
- Individual segments are not separate AccessKit nodes; assistive technology steps
  through them with Increment and Decrement. Child nodes need a segment widget per
  segment: revisit with the table and tab-view work (2.19, 2.22).
- A radio group widget (exclusivity and arrow keys) is worth adding when a first-party
  app needs more than a few radio buttons. Until then, apps copy the gallery's pattern.
- Goldens: 42 states × light/dark × 1x/2x in `apps/gallery/tests/golden`, rendered from the
  same specimens as the gallery page. There are no neue-paint-level goldens for these
  materials; their unit tests check layer balance only.

## Alternatives considered
- A `RadioGroup` container that owns the choice: more code than this task allows, and it
  needs a way for a widget to change its siblings, which Masonry gives only through the
  app's handles.
- Separate `RecessedButton` and `BevelButton` types: they share every line of behaviour.
