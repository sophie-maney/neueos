# 0063: A window carries its own theme properties; a keyboard leave is never dropped

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.33. Two bugs from the Phase 2 runbook (11.13, 11.16).
- A sheet opened in Dark drew light. Both drivers build every window with
  `Options::default_properties`, which is fixed when the driver starts. `Cx::set_theme`
  updated the windows that were open, but a window opened afterwards got the old theme.
- With a HUD window open, focusing another client left the parent's title bar active. The
  Wayland driver defers a keyboard `leave` until the batch ends, so that a window and its own
  popup don't flicker. `enter` on a *different* window took the pending leave out of the
  slot and then flushed an empty slot, so the window the keyboard came from never got
  `WindowFocusChange(false)`. A parent that gave the keyboard to its HUD stayed "focused" to
  Masonry for good; the runner's `set_keep_active` (ADR 0059 §3) was never the cause.

## Decision
1. **`WindowSpec::default_properties: Option<Arc<DefaultProperties>>`** (neue-platform,
   additive; `None` keeps the driver's options). neue-kit sets it from the current theme
   for every window it opens, sheet windows included. The fake host in the tests honours it.
2. **`enter` clears a pending leave only for the same window.** For another window the leave
   is reported first, then the enter.

## Consequences
- Every window opened through the kit follows the theme at the time it opens, and
  `Cx::set_theme` still updates the open ones.
- The Wayland fix is only covered by the VM runbook (11.16): the driver has no headless test.
- Windows opened by raw `neue-platform` users (the demo) are unchanged.
