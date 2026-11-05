# 0058: Sheet placement in neue-comp: modal dialogs hang from their parent's chrome

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.23c. The kit opens a sheet on Wayland as a toplevel with a parent and the
`xdg-dialog` modal flag (ADR 0057). neue-comp recorded the flag only (ADR 0015 §7) and
placed the window like any dialog: centred on its parent, then left alone. A sheet must
come out from under its parent's title bar and stay attached when the parent moves or
resizes. Parents move in many places: interactive and scripted moves, zoom, full screen,
outputs going away, a minimised family coming back.

## Decision
1. **What is a sheet.** A toplevel with a parent whose `xdg-dialog` hint is `modal`
   (`State::is_sheet`). A dialog made modal after it mapped becomes a sheet then. A
   modal dialog without a parent, and a plain dialog with one, are placed as before.
2. **Where.** Its frame (its own server-side bar, if it has one, and its client area) is
   centred horizontally on the parent's client area, its top at the top of that area:
   under the parent's server-side title bar, or at the top of the window geometry of a
   parent that draws its own chrome (`placement::sheet`). Not constrained to the work
   area, so it stays attached when the parent hangs off the screen. A zoomed or full
   screen sheet follows those rules instead.
3. **Staying attached.** `State::attach_sheets` puts every mapped sheet where its parent
   says, without changing the stacking order. The window manager calls it after its own
   moves, zooms and commits (so a `neuectl windows` right after a move sees the sheet in
   place), and the event loop runs it once per iteration, which catches every other way
   a parent moves or resizes without hooking each one.
4. **A sheet doesn't move or resize on its own.** Move requests (the client's, the title
   bar's, `move-window`) are ignored, and the compositor offers no resize edges on it. A
   client may still resize it; it is centred again.
5. **Events.** While its parent is being dragged or resized, a sheet's `window_changed`
   is held back like the parent's, and both are reported when the grab ends.
6. **Fix on the way:** `move_window_to` relocates the window instead of remapping it,
   which raised it above its own children.

## Consequences
- No protocol, IPC or kit change. Sheets look right in the VM with the gallery's
  "Save…" button.
- Not yet: blocking keyboard focus to the parent while a sheet is open (modality), the
  slide animation (Phase 4), dropping the server-side bar of a modal dialog that asks for
  one (a GTK dialog gets a bar under its parent's), constraining a sheet taller than the
  room below its parent.
- Task 2.35 (ADR 0064): a parent that announces its chrome height through
  `neue_window_v1` gets its sheets under that line instead of at the top of its geometry.
- Task 2.36: **keyboard modality.** Focusing a window that has a sheet open (a click on
  it, `focus-window`, xdg-activation, coming back from minimised) gives the keyboard and
  the activated state to its topmost sheet, following sheets on sheets
  (`State::modal_focus_target`). Pointer input to the parent is unchanged: the kit's
  scrim blocks it. When a sheet closes, focus falls back to its parent and so to the
  parent's next sheet, if any.
