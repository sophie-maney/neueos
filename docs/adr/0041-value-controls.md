# 0041: Value controls: slider, stepper, progress bar, spinner

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.13 adds four controls on the 2.12 pattern (ADR 0040): the slider, the stepper, the
progress bar with its barber pole, and the spinner. Two of them keep animating for as
long as they are on screen, which no spring does (ADR 0036), and the reduced-motion
setting has to decide what they do.

## Decision
1. **The app owns the value; the widget shows it** (ADR 0040 §1). `Slider` and `Stepper`
   send `on_change(f64)` when the user changes the value, and nothing when the app does
   (`Handle::set_value`). Values are kept inside the range and snapped to the step.
2. **Slider.** Horizontal only, 120 px wide unless `.width()` says otherwise. Range 0 to 1
   by default (`.range(min..=max)`); `.step(s)` snaps to multiples of `s` from the
   minimum, and 0 is continuous. A press jumps the knob to the pointer and a drag follows
   it, clamped at the ends. Arrow keys move one step (a hundredth of the range when
   continuous), Page Up and Page Down a tenth, Home and End to the ends. No tick marks
   yet. AccessKit: `Slider` with numeric value, minimum, maximum and step, and the
   `Increment`, `Decrement` and `SetValue` actions.
3. **Stepper.** It draws two arrows and no number; the app shows the number beside it
   (a text field after 2.14). Range 0 to 100 and step 1 by default. The upper half adds a
   step, the lower half subtracts one, on release over the half the press began on; Up and
   Down do the same while focused. At an end of the range it sends nothing. Holding a
   half does not repeat yet. AccessKit: `SpinButton`, with `Increment` and `Decrement`.
4. **Progress bar.** `ProgressBar::new()` is indeterminate; `.value(0..1)` makes it
   determinate, and the fill springs (`Spring::STANDARD`) to each new value. Indeterminate
   bars are accent gel with diagonal stripes moving right, one period per
   `BARBER_POLE_PERIOD` (600 ms). AccessKit: `ProgressIndicator`, with a numeric value
   only when determinate, and an optional `.label()`.
5. **Spinner.** Twelve spokes turning clockwise in twelve steps, one turn per
   `SPINNER_PERIOD` (1 s). `Handle::set_spinning(false)` stops it; a stopped spinner draws
   nothing, keeps its space and is hidden from assistive technology.
6. **Ambient animation uses a `Ticker`, not a spring.** It counts elapsed time modulo the
   period and asks for frames while it is wanted. Like the default button's pulse
   (`Motion::ambient`), it **stops where it is under reduced motion**, and when the
   widget is disabled. It restarts when reduced motion is switched off or the widget is
   enabled. Both periods are tokens in `neue_theme::motion`.
7. **Paint.** The groove (slider track and progress trough) is shared, in
   `neue_paint::materials::groove` with its palette in `neue_theme::materials::groove`.
   The slider knob and the stepper reuse the push button's neutral gel
   (`aqua_button::Palette::plain`), and the progress fill its accent gel. A held knob
   and a held stepper half darken by `PRESSED_DIM`.
8. **Every new Masonry widget type must be registered in `theme::default_properties`**,
   or it reads the static default theme (light, full motion) instead of the app's. The
   new types are, and the reduced-motion test in `tests/values.rs` covers it.

## Consequences
- A spinner or barber pole that stops under reduced motion is a static picture, which tells
  the user less than a moving one. If testing shows that matters, the alternative is to
  keep these two moving at a reduced rate; that is a change to `Motion::ambient`'s
  contract, so it needs its own decision.
- Stepper hold-to-repeat, slider tick marks and a vertical slider are not here. Add them
  when a first-party app needs them.
- Goldens: 88 files in `apps/gallery/tests/golden` (slider, stepper, progress bar at
  0, 50% and 100%, barber pole at rest and a quarter period on, spinner at rest and three
  steps on), states × light/dark × 1x/2x. The stepper's pressed golden holds the lower
  half, because the fixture presses on the widget's centre. The upper half is covered
  by a behaviour test.

## Alternatives considered
- Animating the stripes and the spinner with a spring that loops: springs settle, so
  looping means retargeting them every frame, which gains nothing over a counter.
- Letting the slider spring its knob to keyboard and app changes: dragging needs the knob
  to follow the pointer exactly, and mixing the two makes the knob lag the pointer.
