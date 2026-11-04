# 0019: Draw only when needed (task 1.15b)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Until now the tty backend rendered every output once per refresh, forever: after a vblank
it rendered again at once, and a frame without damage retried one refresh later (anvil's
loop). An idle desktop therefore woke the CPU 60+ times a second per output, which costs
power on laptops and hides real activity in traces. Frame callbacks of empty frames went
out right after the render, so a client committing without damage was not throttled to
the refresh rate.

## Decision
1. **A per-output state machine.** `redraw.rs` holds the bookkeeping, free of DRM so it is
   unit-tested: *idle → scheduled → waiting for vblank* (a frame was queued) or *waiting
   for an estimated vblank* (the render had no damage) → *scheduled* again if anything
   changed meanwhile, else *idle*. One render per refresh at most, none while idle.
2. **Estimated vblank.** After an empty render a timer fires at the next refresh boundary
   counted from the output's last real vblank (one refresh from now if there was none).
   That is when the empty frame's frame callbacks go out. Frames with damage keep sending
   their callbacks right after queueing, as before.
3. **Coarse redraw requests.** `State::queue_redraw_all` marks every output. It is called
   when the Wayland socket has client traffic (commits, destroyed surfaces, new titles,
   layer and lock surfaces, hang-ups: one hook instead of one per handler), on every
   commit (fence-blocked commits land outside client dispatch), on every input event, on
   `neuectl` traffic, when a config is applied, and on hotplug and session resume. A
   render that finds no damage is CPU-only (element walk and damage compare) and capped at
   the refresh rate by point 2, so marking too much is cheap and marking too little (a
   missed redraw) is the failure mode to avoid. Per-output requests can come later if
   profiling with several busy outputs asks for them.
4. **Animated cursors** arm a per-output timer for their next image
   (`cursor::Themes::until_next_frame`); still cursors arm nothing.
5. **Observability.** Each render logs a `trace` event (`neue_comp::tty`) with the output's
   running render and frame counts: an idle desktop logs nothing.
6. Only the tty backend changes. winit (development only) keeps redrawing continuously;
   headless keeps its frame timer.

## Consequences
- An idle desktop sleeps; frame callbacks of no-damage commits arrive at the refresh rate.
- New compositor-side visual state that changes without client traffic, input, IPC or
  config (animations, Phase 2+) must call `State::queue_redraw_all` (or a per-output
  variant added then), or it will not show until something else wakes the output.
- VRR (task 1.15c) will need the estimated vblank to follow the actual refresh, not the
  nominal one.
