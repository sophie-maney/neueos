# 0016: Input protocols (task 1.13)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Games and 3D tools need pointer lock and relative motion, apps want touchpad gestures, and
CJK users need an input method. Smithay implements the protocols; the decisions are policy.

## Decision
1. **Globals:** `pointer-constraints-v1`, `relative-pointer-v1`, `pointer-gestures-v1`
   (v3, with hold), `text-input-v3` and `input-method-v2`. Any client may be the input
   method (fcitx5 and ibus are ordinary clients); a second one gets `unavailable`.
   Code: `input_protocols.rs`.
2. **When a constraint is active.** At most one constraint is active, on the surface
   under the pointer, when it belongs to the window with keyboard focus, the session is not
   locked and the pointer is inside the constraint's region. A background window cannot
   hold the pointer. It is activated when it is created, when the pointer enters, and when
   focus changes; it is deactivated when any of that stops being true. One-shot constraints
   end on deactivation (Smithay).
3. **Motion under a constraint.** `State::move_pointer_by` (relative: mice and
   touchpads) always sends relative motion to the focused client first. A lock then keeps
   the pointer where it is and ignores absolute motion (tablets, VM pointers). A
   confinement moves the pointer as far as the surface and region allow (bisection), then
   slides along the edge one axis at a time. `move_pointer` itself is unconstrained: it is
   the explicit "put the pointer here" call. Absolute backends use `move_pointer_towards`.
4. **Cursor position hint.** When a client destroys an active lock after sending a hint,
   the pointer goes to the hint (toplevels only), so it does not jump back.
5. **Gestures** (swipe, pinch, hold) go to the focused client through `State::gesture`.
   The compositor does not consume any yet; Spaces and Overview (Phase 4) will take
   three- and four-finger swipes before they reach clients.
6. **Touchpad config** (`[touchpad]` in `compositor.toml`): `tap-to-click`,
   `tap-and-drag`, `drag-lock`, `disable-while-typing`, `click-method` and `gestures`
   (forward gestures to clients). Applied to each touchpad when libinput reports it and to
   plugged-in touchpads on reload. Tap to click defaults on, unlike libinput; click method
   defaults to clickfinger. Settings a device does not support are skipped (debug log).
   Pointer acceleration and scroll method stay at libinput's defaults.
7. **Input method popups** are tracked with `PopupManager` and drawn and hit-tested with
   their window, like xdg popups. Their placement is Smithay's (caret rectangle relative to
   the focused window's geometry); they are not kept on screen yet.

## Consequences
- `State` gains the five protocol states, `constrained_surface`, `constraint_count` and
  `pointer_pos`.
- **Smithay quirks that shaped the code:** `focus_changed` is not called when keyboard
  focus is dropped (`set_focus(None)`), so `clear_focus` and the lock code refresh the
  constraint themselves; and Smithay holds the keyboard and pointer locks while it calls
  `focus_changed` or runs a grab, so constraint checks use the focus passed in and the
  cached `pointer_pos`, never `current_focus()` or `current_location()` (these deadlocked
  the chrome tests).
- Text inputs only get `enter` while an input method is bound (Smithay); binding one later
  sends it.
- Test clients bind the new protocols (`tests/harness/client/input.rs`); no new
  dependencies (`wayland-protocols-misc` was a dev-dependency already).

## Not done
- No escape hatch from a lock besides a key binding or switching windows; no on-screen
  hint that a lock is active.
- Locks and confinements do not follow a window that moves under the pointer until the
  next pointer event.
- Cursor position hint for subsurfaces and layer surfaces.
- Input method popups are not constrained to the output; no `virtual-keyboard`; no
  compositor-side input method; `input-method` keyboard grab is Smithay's and key
  bindings still run before it.
- Touchpad acceleration, scroll method, tablets and touch screens (later settings tasks).
- Real hardware: gestures and touchpad config were not tried (`docs/dev-setup.md` §12).

## Alternatives considered
- **Deferring the constraint refresh to an idle callback** (to avoid the lock problem):
  events could arrive after a client's roundtrip returns, which makes tests and clients
  racy. Passing the focus and caching the position is deterministic.
- **Activating constraints without keyboard focus:** a hung or background app could pin
  the pointer. Requiring focus is what other compositors do.

## Revisit when
- Overview or Spaces take gestures (Phase 4).
- The settings app exposes touchpad options (Phase 5).
