# 0036: Spring animation: a closed-form solver in neue-theme, stepped on Masonry's frames

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.8 gives widgets spring motion. `neue-theme` already holds the presets
(`Spring::SNAPPY`, `STANDARD`, `GENTLE`, `BOUNCY`, given as response and damping ratio)
and the reduced-motion switch (`Motion::spring` returns `None` when motion is reduced).
Masonry calls `Widget::on_anim_frame` with the time since the last frame. Both platform
drivers already drive these frames (ADRs 0032, 0033). The compositor will need the same
springs later (window open/close, Overview, Spaces), and it does not depend on neue-kit.
Tests must be deterministic, and motion must look the same at 60 Hz and 120 Hz.

## Decision
1. **The solver lives in `neue-theme::motion` as `SpringValue`**, next to the presets.
   It is pure and portable, so the compositor can step it from its own frame clock
   without depending on the toolkit.
2. **It is solved in closed form** (underdamped, critically damped and overdamped cases
   of x'' + c x' + k x = 0 for a unit mass), not integrated numerically. A step depends
   only on the time elapsed, so splitting time into frames differently gives the same
   result (to float precision). Dropped frames and refresh rates therefore don't change
   the motion, and tests can step with any interval.
3. **Rest:** within a threshold of the target (default 0.001 units, and the same for the
   velocity per response period), the value lands exactly on the target and stops, so
   the last painted frame is exact and frames stop.
4. **Retargeting keeps the velocity**, so an interrupted move turns smoothly.
5. **Reduced motion snaps.** `animate_to(target, motion.spring(preset))` with `None`
   jumps to the target and requests no frames. If reduced motion is switched on during a
   move, the value jumps to its end on the next frame.
6. **neue-kit glue (`anim::Animated`, crate-private):** it pairs a `SpringValue` with
   its preset. Widgets call `animate_to(ctx, theme, target)`, then
   `on_anim_frame(ctx, theme, interval)`, and `theme_changed(ctx)` from
   `property_changed`. Masonry gives `property_changed` no properties, so a moving value
   asks for one more frame and checks the theme there. The glue stays crate-private
   because its signatures take Masonry contexts (ADR 0002 §4).

## Consequences
- Values are scalar `f64`. A point or a colour animates as several `SpringValue`s; add a
  generic version only if that proves clumsy.
- No widget moves on springs yet. The first users are the checkbox and segmented control
  (2.12) and the slider (2.13). Until then `anim` carries an `expect(dead_code)` with that
  reason.
- Fixed-duration fades (`motion::FAST`, `NORMAL`, `SLOW`) and ambient loops (the default
  button's pulse, task 2.9) are not springs and are not covered here.
