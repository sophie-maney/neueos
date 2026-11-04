# 0015: Shell-facing protocols (task 1.12)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
The shell (menu bar, dock, launcher, notifications), a screen locker and apps need protocols
the compositor did not offer: panels and wallpapers, a lock screen, idle detection, a
window list for the dock, and a few app-to-shell hints. Smithay implements all of them; the
decisions are the policy around them.

## Decision
1. **`wlr-layer-shell`** (`layer.rs`). Smithay's per-output `LayerMap` does the layout.
   Surfaces map into the output the client named, else the one under the pointer; with no
   output the client gets `closed`. Each commit re-arranges, sends the first configure, and
   refits zoomed windows only when the work area changed. Layers above windows (`overlay`,
   `top`) are hit-tested before window chrome, `bottom` and `background` after the windows,
   and drawn in the same order (`render.rs`).
2. **Keyboard focus for layers.** A surface on `overlay` or `top` with `exclusive`
   interactivity and a buffer takes the keyboard (`overlay` first); windows cannot take it
   back (`focus_window` only raises) until it is gone, then the topmost window gets it.
   `on_demand` surfaces, and `exclusive` ones on `bottom`/`background`, get it when clicked.
   A click on an above-window layer surface never reaches the window behind it.
3. **`ext-session-lock-v1`** (`lock.rs`). `locked` is sent at once: the next frame shows
   lock surfaces only (the clear colour on an output without one), so there is nothing
   to wait for. Locking ends pointer grabs and drags, drops keyboard focus and gives the
   first lock surface the keyboard (others when clicked). While locked, `surface_under`
   returns lock surfaces only, key bindings are off (only the VT switch stays), and
   `focus_window`/`clear_focus` do nothing. A second locker is refused (`finished`) while
   the first is alive. If the locker dies the session stays locked and a new client can
   lock again and unlock, as the protocol asks. Unlocking restores the previous keyboard
   focus if it is still there. Renderers see no `State`, so the per-output lock state and
   lock surface live in the output's user data (`lock::OutputLock`).
4. **Idle.** `ext-idle-notify` gets activity from every scripted or real key, motion,
   button and axis event. `idle-inhibit` pauses it while any surface holds an inhibitor.
5. **`ext-foreign-toplevel-list`.** A window is announced when it maps, with the
   identifier `window-<id>`, so the shell can match it to `neuectl`'s window id (ids are
   never reused). Title and app id changes and closing are sent. Minimised windows stay.
6. **`xdg-foreign`** is Smithay's: an imported parent is set as the toplevel's xdg parent,
   so our existing parent handling (stacking, `neuectl` `parent`) applies.
7. **`xdg-dialog`: modal flag only.** Recorded (`State::dialog_hint`) and nothing more;
   sheets and modal behaviour come with Phase 4.
8. **`xdg-toplevel-icon`.** Sizes 64 and 128 are announced; the icon is kept on the
   surface for the dock (Phase 3).
9. **`xdg-system-bell`.** A bell for a window asks for its attention, like a dock bounce.
   There is no sound until the audio service exists.
10. **`keyboard-shortcuts-inhibit`.** Accepted at once if the surface has the keyboard,
    else when it gets it; deactivated when focus leaves. While active, key bindings are
    off, except the VT switch (the escape hatch).

## Consequences
- Tests need client bindings for these protocols: `wayland-protocols` gains the
  `unstable` feature and `wayland-protocols-wlr` (both dev-dependencies, MIT) is added.
  Test code only; the shipped binary uses Smithay's server side.
- `State` gains the protocol states and `session_lock`; `State::is_locked` is checked on
  every path that gives out focus.
- Smithay's `Dispatch2` makes `delegate_dispatch2!(State)` enough for all of them.

## Not done
- Idle inhibit ignores visibility (a minimised or covered window still inhibits).
- Layer surfaces of a removed output are closed, not moved to another output.
- Several outputs: the exclusive layer surface is global, not per output.
- Lock surfaces are not re-sent a configure when an output changes mode.
- Popups of a layer surface are tracked and drawn but not unconstrained.
- Real-session checks (swaylock, a panel, a launcher) are the owner's: `docs/checks/phase-1.md` §11.

## Alternatives considered
- **Wait for a rendered frame before `locked`:** stricter, but needs a hook in both
  renderers for no visible gain while the lock only ever shows the clear colour or the
  locker's own surface.
- **Keep lock state in `State` and pass it to the renderers:** changes the signature of
  `output_elements` and the tty render path; output user data (as Smithay's layer map
  does) needs none.

## Revisit when
- The dock and menu bar exist (Phase 3): per-output exclusive focus, icon use.
- Overview or Spaces (Phase 4) change what "above windows" means.
