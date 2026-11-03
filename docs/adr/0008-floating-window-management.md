# 0008: Floating window management rules (task 1.5)

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
Task 1.5 turns the 1.1 stack (cascade placement, click to focus) into OS X-style
floating window management: focus and stacking rules, interactive move and resize,
placement, minimise/zoom/close, and xdg-activation. These rules are policy that later
tasks (title bars 1.10/1.11, the dock in Phase 3, Spaces in Phase 4) build on, and the
`neuectl` protocol grows to cover them (additive, per ADR 0003/0007).

## Decision
Code: `compositor/neue-comp/src/wm.rs` (policy), `grabs.rs` (pointer grabs),
`placement.rs` (pure geometry).

- **Stacking:** raising a window raises its children (`xdg_toplevel.set_parent`, i.e.
  dialogs) after it, so children always stay above their parent.
- **Focus:** a button press on a window raises and focuses it (the click is passed on);
  a press on the background clears focus. New windows are focused when they map; there is
  no focus-stealing prevention for new windows until the launcher passes activation
  tokens (Phase 3). When the focused window closes or is minimised, focus goes to its
  parent, else the topmost window of the same client, else the topmost window.
- **Work area:** the output minus layer-shell exclusive zones (`non_exclusive_zone`), so
  the menu bar (1.12, Phase 3) is avoided without further changes. Placement and moves use
  the output under the pointer, or the window's output.
- **Placement:** centred on the parent for dialogs; else cascaded 24 px from the topmost
  window of the same app id; else where the app's last window closed (in memory, this
  session only); else centred horizontally with a third of the free height above. Then
  kept inside the work area; a window too large is aligned to its top-left.
- **Moves** (interactive and `move-window`) are free except that the top edge stays
  inside the work area's vertical range, so the title bar never goes under the menu bar.
- **Resize:** the client gets `resizing` configures with the new size; for top/left edges
  each commit moves the window so the opposite edges stay put (smallvil's approach).
  Resizing a zoomed window is ignored.
- **Minimise** unmaps the window and its children and keeps their positions; they are
  restored together by `unminimize-window`, `focus-window` or a granted activation.
  Minimised windows are listed first in `windows` with `minimized: true`. No animation
  yet (genie/scale comes with the dock).
- **Zoom** (the green button) is the xdg `maximized` state filling the work area. It
  toggles back to the previous geometry. Dragging a zoomed window unzooms it, keeping the
  pointer at the same relative x in the title bar. Fullscreen is untouched (Phase 4/1.15).
- **xdg-activation:** every token is accepted, but it is marked *granted* only if the
  requesting client had keyboard focus when it made the token, and any serial it passed
  is no older than the keyboard's last enter. A granted token used within 10 s focuses
  (and unminimises) the window; otherwise the window gets `demands_attention`, cleared when
  it is focused. Tokens are single-use.
- **Popup grabs:** `xdg_popup.grab` uses Smithay's `PopupKeyboardGrab`/`PopupPointerGrab`
  (as in anvil), so menus close on an outside click.
- **IPC (additive):**
  - actions `minimize-window`, `unminimize-window`, `zoom-window` (toggle),
    `move-window` (`x`, `y`), `resize-window` (`width`, `height`);
  - `ActionParams` gains optional `x`, `y`, `width`, `height`; `ActionInfo` gains `args`
    (required argument names);
  - `WindowInfo` gains `minimized`, `zoomed`, `demands_attention` and `parent`;
  - new event `window_changed` (a `WindowInfo`), sent when anything but `focused`
    changes: title, app id, parent, geometry, minimised/zoomed/attention state. It is held
    back during an interactive move or resize and sent when it ends;
  - `neuectl action` takes `--x --y --width --height`.
  - Older clients that don't know `window_changed` fail to parse it; all clients live in
    this repo, so no protocol bump.

## Consequences
- The dock (Phase 3) gets minimise state, attention requests and parents from the same
  state that `neuectl` reports; the shell protocol can expose it later.
- Remembered positions are lost on restart; persisting them needs a state file
  (later, with `neue-config`).
- Zoom fills the work area instead of the app's "best size": Wayland has no way to ask
  for one.

## Alternatives considered
- **Strict focus-stealing prevention for new windows:** breaks launching apps from a
  terminal until launchers pass tokens; revisit in Phase 3.
- **Refusing invalid tokens in `token_created`:** Smithay then never reports the
  activation, so the window couldn't ask for attention.
- **Hiding minimised windows inside the `Space`:** Smithay's `Space` has no hidden
  elements, so they would still be hit-tested and rendered.
