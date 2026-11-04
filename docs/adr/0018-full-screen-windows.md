# 0018: Full-screen windows (task 1.15a)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Games, video players and presentation apps ask for the `xdg_toplevel` fullscreen state;
direct scanout (task 1.15c) needs such a window to scan out. Until now the request was
ignored (ADR 0008). The OS X design puts full-screen apps on their own Space, which is
Phase 4 work. Something simpler has to work now and be easy to replace then.

## Decision
1. **A stopgap state on the window, not a Space.** `WmData` keeps the output a window is
   full screen on and the geometry to restore. Code: `fullscreen.rs`. Full-screen Spaces
   (Phase 4) will replace the placement rules below, not the protocol side.
2. **Which output:** the one the client names in `set_fullscreen`, if it is in the layout;
   else the one the window is already full screen on; else the one it overlaps most; else
   the output under the pointer. A request before the first commit works the same way and
   the window maps at the output's origin.
3. **Geometry:** the configure carries the output's logical size; exclusive zones don't
   apply. A window that commits a smaller size is centred and drawn on a black backdrop
   (xdg-shell's "border fill"); a larger one is left at the origin and clipped by the
   output.
4. **No title bar:** `decorations::has_title_bar` is false while full screen, so nothing
   draws, hit-tests or reserves room for the bar. The decoration mode is unchanged and the
   bar comes back on leaving.
5. **Stacking with layer shell:** an output is *covered* when its topmost window is a
   full-screen window on it, or a child (transitively, `set_parent`) of one, so a game's
   dialog does not bring the panels back. On a covered output the `top` layer is drawn and
   hit-tested below the windows; `overlay` stays above (lock screens and OSDs use it).
   Entering full screen raises the window but does not take focus.
6. **Leaving:** the window gets back the place and size it had (constrained into the work
   area of the nearest output); a window that was zoomed is refitted to its work area
   instead and keeps the `maximized` state.
7. **Interactions while full screen:** interactive move and resize are refused; zoom
   requests get a configure and are otherwise ignored; `move-window` and `resize-window`
   leave full screen first; minimise works and restores it full screen.
8. **Outputs:** a mode or scale change refits it; when its output goes away it moves to
   the first remaining output and stays full screen there (also when minimised).
9. **IPC:** `WindowInfo` gains `fullscreen` (defaults to false for older peers), shown as
   `[fullscreen]` by `neuectl windows`; new action `fullscreen-window` toggles it.

## Consequences
- Direct scanout (1.15c) can target the window an output is covered by.
- The covered rule is per output and recomputed every frame and every hit test; it is a
  walk over the window stack, cheap for desktop window counts.
- Not done: an `exclusive` keyboard surface on the `top` layer keeps the keyboard while a
  full-screen window covers it; no full-screen animation; no menu-bar reveal at the top
  edge (comes with the menu bar, Phase 2); several full-screen windows on one output just
  stack.
