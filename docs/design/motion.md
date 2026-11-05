# Motion spec

Source: `crates/neue-theme/src/motion.rs` (tokens and the spring solver, ADR 0036) and
`crates/neue-kit/src/anim.rs` (the widget glue). The compositor will use the same tokens and
solver from its own frame clock (window open and close, Overview, Spaces).

## 1. Principles

1. **Quick and physical.** Controls answer in about a tenth of a second, panels settle in
   about a third, whole-screen transitions in under half a second. Nothing waits for an
   animation: input is handled immediately and the motion catches up.
2. **Springs for things that move, durations for things that fade or loop.** A spring keeps
   its velocity when retargeted, so an interrupted move turns smoothly instead of
   restarting. Fixed durations are for fades, delays and loops.
3. **Same motion at any refresh rate.** Springs are solved in closed form, so the result
   depends on elapsed time, never on how it is cut into frames: 60 Hz and 120 Hz agree, and
   tests can step with any interval.
4. **Reduced motion snaps.** One switch (`Theme::reduced_motion`) turns movement into
   instant changes (§5).
5. **No hard-coded timings.** Widgets and shell code take every duration and spring from
   `neue_theme::motion`.

## 2. Spring presets

A spring is `(response, damping_ratio)`. *Response* is the period of the undamped
oscillation in seconds (smaller is faster); a damping ratio of 1 is critically damped (no
overshoot), below 1 it bounces. For a unit mass, stiffness k = (2π / response)² and damping
c = 4π ζ / response (`Spring::stiffness`, `Spring::damping`).

| Preset | Response | Damping ratio | Settles to 98% | Rests (±0.001) | Overshoot | Use |
|---|---|---|---|---|---|---|
| `SNAPPY` | 0.15 s | 1.0 | 139 ms | 220 ms | none | Controls: switches, checkboxes, segmented selection, bevel on/off, scroller widening |
| `STANDARD` | 0.30 s | 1.0 | 278 ms | 441 ms | none | Sheets, popovers, menus, window open and close; fades such as the scroller's, progress fill |
| `GENTLE` | 0.45 s | 0.95 | 377 ms | 545 ms | negligible | Whole-screen transitions: Overview, Spaces, Notification Center |
| `BOUNCY` | 0.35 s | 0.70 | 333 ms | 573 ms | about 4.6% | Playful moves: dock icons, a shaken login field. Use sparingly |

The settle times were computed from the closed-form solution (step from 0 to 1, no initial
velocity). Pick the preset by *what is moving*, not by how long it should take: controls
`SNAPPY`, surfaces `STANDARD`, the whole screen `GENTLE`.

### Running a spring: `SpringValue`
A scalar `f64` that moves toward a target. A point or colour animates as several values.

- `animate_to(target, motion.spring(PRESET))` starts or retargets; it keeps the current
  velocity. It returns whether the value now animates (the caller then requests frames).
- `step(dt)` advances; it returns whether the value still moves.
- `snap_to(v)` and `finish()` jump and stop.
- **Rest:** within `DEFAULT_REST_THRESHOLD` (0.001 units, and 0.001 units per response
  period for velocity) of the target, the value lands *exactly* on the target and stops, so
  the last painted frame is exact and frame requests end. Change it with
  `with_rest_threshold` for values with large or small units.
- Opacities (0 to 1) and logical pixels both suit the default threshold.

### In the toolkit: `anim::Animated` (crate-private)
Pairs a `SpringValue` with its preset. A widget holds one per moving quantity and:
1. calls `animate_to(ctx, theme, target)` when the quantity should change;
2. forwards `Widget::on_anim_frame` to `on_anim_frame(ctx, theme, interval)`;
3. forwards the theme property change to `theme_changed(ctx)`.

It paints from `value()`. A moving value always asks for one more frame so it notices a
changed theme and snaps under reduced motion.

## 3. Fixed durations

| Token | Value | Use |
|---|---|---|
| `FAST` | 100 ms | Highlight and hover fades |
| `NORMAL` | 200 ms | Most fades and small moves |
| `SLOW` | 350 ms | Sheets, popovers, notification banners when not sprung |

No widget uses these yet; the springs cover today's motion. They exist so a fade that
cannot be a spring (a cross-fade with a fixed end time) still has a token.

## 4. Ambient loops and timers

Ambient motion runs on its own, not toward a target. It stops under reduced motion where
noted.

| Token | Value | Behaviour | Reduced motion |
|---|---|---|---|
| `DEFAULT_BUTTON_PULSE` | 1400 ms | The default push button's gel pulses: amount = 0.5 − 0.5·cos(2πt/period), 0 at rest, 1 at the half period, lightening the gel toward white by up to `PULSE_LIGHTEN` (0.3). Runs only while the button is the default, enabled, and its window is focused. | Stops (`Motion::ambient()`) |
| `BARBER_POLE_PERIOD` | 600 ms | Indeterminate progress bar: stripes (20 px period) move right one period in this time. | Stops |
| `SPINNER_PERIOD` | 1000 ms | Spinner turns once, clockwise, in twelve discrete steps. | Stops |
| `CARET_BLINK` | 1000 ms | Text caret shown for the first half, hidden for the second. | Not affected: a timer, not movement |
| `CARET_BLINK_TIMEOUT` | 10 s | After this long without typing the caret stays solid. | |
| `SCROLLER_HIDE_DELAY` | 1000 ms | Overlay scrollers start to fade after this long without scrolling, unless the pointer is over one or one is held. The fade is a `STANDARD` spring; widening on hover is `SNAPPY`. | The fade snaps |
| `TOOLTIP_DELAY` | 1000 ms | Delay before a tooltip appears. | Not affected |
| `TYPE_SELECT_RESET` | 1000 ms | Type-select in a menu or list starts over after this long without a key. | Not affected |

The last three rows are timing constants of interaction rather than motion, kept here
because they are durations a designer may tune.

## 5. Reduced motion

`Theme::reduced_motion` (via `Motion { reduced }`):
- `Motion::duration(d)` returns zero;
- `Motion::spring(s)` returns `None`: pass it straight to `SpringValue::animate_to`, which
  then jumps to the target and requests no frames;
- `Motion::ambient()` returns `false`: stop pulses, barber poles and spinners (they are
  still drawn, holding their current phase, so a busy spinner still shows); they also stop
  while the control is disabled;
- if reduced motion is switched on while a value moves, it jumps to its end on the next
  frame.

State is still shown. Reduced motion removes the *movement*, never the information: a
toggled checkbox is checked at once, an indeterminate bar still looks busy, a default button
is still the accent colour.

## 6. Rules for new motion
1. Choose the preset by what moves (§2). Do not invent a spring; a new preset is a design
   change and goes in `motion.rs` and here.
2. Never animate layout during input. Move, fade and recolour painted state; let size and
   position change instantly (a segmented control's highlight slides, but the control does
   not resize).
3. Gate every ambient loop on `Motion::ambient()` and request frames only while it runs.
   A resting window requests none.
4. Test by stepping frames in `neue-kit`'s test harness (`animate_ms`; any interval gives
   the same result), and
   test the reduced-motion path: the value must be at its target after one frame.
5. Compositor and shell motion (Overview, Spaces, notification banners, dock) use the same
   `SpringValue`, stepped from the frame clock. They need `neue-theme` only.
