# 0034: neue-platform's Wayland driver: popup placement, clipboard, activation, dialogs, AccessKit

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.6 finishes the Wayland driver (ADR 0033). Masonry's layers (menus, tooltips) were
shown as xdg-popups anchored to a 1×1 point that could only flip vertically, and
`RepositionLayer` didn't move them. Copy, paste, `TakeFocus` and AccessKit did nothing on
Linux. Masonry gives a driver little to work with: a layer kind (`Tooltip`, `Selector`,
`Other`) and a point in window coordinates; `ClipboardStore(String)`; no paste request
(drivers answer the paste key with `TextEvent::ClipboardPaste`, as the winit driver does
for Cmd-V); and nothing about text selections.

## Decision
1. **Popup placement per layer kind** (`wayland/popup.rs`). Menus whose point is the
   bottom-left corner of a widget's border box (the selector's menu) anchor to that box
   and flip above it; other menus (context menus) anchor to the point and flip on both
   axes; both slide on both axes and take an explicit grab. Tooltips anchor to the point,
   flip above it, never grab, and get an empty input region. The owner widget is found by
   walking the base layer's widget tree, because Masonry doesn't pass it. A menu opened
   while another grabbing menu is open nests on it (xdg-shell wants grabs to come from
   the topmost grabbing popup). Closing or dismissing a popup closes the popups nested on
   it. `RepositionLayer` moves the anchor by the same amount and sends
   `xdg_popup.reposition` (xdg_wm_base v3 and later). Positioners are reactive.
2. **Clipboard** (`wayland/clipboard.rs`, sctk's `data_device_manager`). Copy takes the
   selection with the serial of the input event that asked for it, under the usual text
   MIME types, and answers `send` itself until the selection is cancelled. Ctrl-V,
   Ctrl-Shift-V and Shift-Insert never reach Masonry: the driver reads the selection
   through its pipe as a calloop source and pastes once the pipe closes, so a slow source,
   or this process, can't block the loop. Our own selection is pasted from memory. Writes
   go out in `PIPE_BUF` pieces as the pipe polls writable, so they don't block either,
   with no `O_NONBLOCK` (and no `fcntl` binding).
3. **Primary selection: paste only.** A middle click pastes it, after Masonry has seen the
   press. Masonry doesn't report text selections, so the driver can't own the primary
   selection. That waits for the text fields task (2.14) or a Masonry change. (Amended
   2026-11-05: see below.)
4. **AccessKit through `accesskit_unix` 0.24** (AT-SPI over zbus, with its default
   `async-io` executor on its own thread). One adapter per window. Its handlers send
   requests to the loop over a calloop channel; the tree goes out with the next frame, as
   `accesskit_winit` does it. Window bounds are `(0, 0, physical size)`, because Wayland
   hides window positions. Keyboard focus updates the adapter's focus state.
5. **xdg-activation.** Masonry's `TakeFocus` requests a token with the latest serial and
   the focused surface, then activates the window. `XDG_ACTIVATION_TOKEN` activates the
   first toplevel. The variable is left in the environment: removing it is `unsafe` once
   threads exist, and a used token is worthless to child processes.
6. **Parent windows and modal dialogs.** `WindowSpec` gains `parent: Option<WindowId>`
   (`xdg_toplevel.set_parent`) and `modal: bool` (xdg-dialog `set_modal`). The dialog
   object is destroyed before its toplevel. The winit driver ignores both for now.
7. **neue-comp unconstrains layer-shell popups.** Its `unconstrain_popup` only knew
   windows, so popups of panels were never flipped. It now also finds the popup's layer
   surface and its place in the output's layer map, and runs when the layer shell
   assigns the popup's parent.

New dependency: `accesskit_unix` 0.24.1 (MIT OR Apache-2.0). It was already in
`Cargo.lock` through `accesskit_winit`; it adds only `futures-macro` (MIT OR Apache-2.0).
It is pure Rust and talks D-Bus through zbus, so it needs no C library.

## Consequences
- In the VM: the bottom-edge strip's selector menu flips above the strip; copy and paste
  work both ways with foot, and a middle click pastes foot's primary selection; a Python
  AT-SPI client reads the demo's tree and presses its buttons
  (`docs/checks/phase-2.md` §4).
- Window frames have no accessible name: Masonry's root node carries no title.
- No drag and drop yet: the data device ignores drag events.
- Not exercised in the VM yet: tooltips (no Masonry widget creates one), `xdg_popup.reposition`,
  nested menus (they arrive with task 2.17), `TakeFocus` and the start-up token.
- The neue-comp test harness has no xdg-popup client, so the layer-popup fix has no
  headless test yet.

## Amendment 2026-11-05 (task 2.28): owning the primary selection
Decision 3 changes: the driver owns the primary selection when a neue-kit text field
reports one. Masonry still has no signal for it, so the report goes by neue-kit's own
channel instead:
- `neue_platform::Ctx::set_primary_selection(text)`, a no-op on winit. It queues the text
  on the driver's registry (the latest call wins; empty text is ignored), and the driver
  takes the selection after the callback with the serial of the latest input event, like
  Copy (decision 2). The driver keeps the source and its text, answers `send` requests
  itself, pastes it from memory on a middle click, and forgets it when another client
  takes the selection.
- `TextField` submits `FieldAction::Selected(text)` when a primary-button press is released
  over a non-empty selection (a drag, a double or triple click, a Shift-click), and on
  Shift with an arrow, Home or End. Secure fields never do. `Runner::action` handles it
  before looking for a message handler, so fields need no `on_*` call.
- Not covered: `TextView`, Ctrl-A and clearing the selection when a field's selection
  collapses; a text view or a Masonry-level signal can reuse `Ctx::set_primary_selection`.
