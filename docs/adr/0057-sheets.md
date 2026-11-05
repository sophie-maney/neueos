# 0057: Sheets: one kit widget, a modal child window on Wayland, drawn in the parent on macOS

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.23b adds sheets: a modal panel attached to a window, with no title bar, that comes
out from under the window's title bar. PLAN §3.2 maps modal dialogs to xdg-dialog. The
Wayland driver already opens a toplevel with a parent and the xdg-dialog modal flag
(ADR 0034 §6); the winit driver ignores both, so on macOS a second window would be a free
floating, non-modal window. neue-comp records the modal flag but does not place modal
dialogs or block their parent's input yet (ADR 0015 §7; placement is task 2.23c).

## Decision
1. **API.** `Cx::show_sheet(window, Sheet::new(size, content).title(..)
   .default_button(label, msg).cancel_button(label, msg))` and `Cx::close_sheet(window)`.
   One sheet per window; showing another replaces it. The buttons send messages but do not
   close the sheet: the app closes it, so it can keep it open (to point at a mistake).
   A sheet has an explicit size, like a window, because on Wayland its window is sized
   before its content is laid out. Without a `default_button`, a content button made with
   `Button::as_default` is the sheet's default.
2. **One widget, two hosts.** `SheetWidget` (sheet material, content, a button row in the
   bottom right corner) is the same everywhere. `neue_platform::Ctx::has_modal_windows`
   tells the kit which host to use:
   - **Wayland:** the sheet is the root content of a toplevel with `parent`, `modal` and
     client-side decorations (no title bar). Its focus fallback is the sheet widget.
   - **winit (macOS) and the kit's tests:** the parent window's `RootWidget` draws it,
     centred, its top edge under the chrome (title bar and toolbar), no wider or taller
     than the content area. Focus moves to the sheet, which becomes the window's focus
     fallback; on closing, focus and the fallback go back.
3. **Blocking the parent.** In both hosts the parent's `RootWidget` lays an invisible
   scrim over its content (under the chrome) while a sheet is open; pointer events land on
   it, not on the content. On Wayland that does not rely on the compositor. Return in the
   parent does not press the parent's default button while a sheet is open, and in the
   drawn-in host the parent's default button loses its default look and pulse until the
   sheet closes.
4. **Keys are the sheet's.** The sheet handles Return (default button), Escape (cancel
   button) and Tab/Shift-Tab itself and reports them as a `SheetKey` action. The runner
   presses the button if it is enabled, as it does for a window's default button. For
   Tab it walks the sheet's subtree for widgets that take focus (not disabled, not
   stashed) and moves focus to the next one, wrapping, so focus never leaves the sheet:
   Masonry's own Tab traversal covers the whole window and has no focus scope. A
   compositor close request on a sheet's own window presses its cancel button and does
   not reach `Application::close_requested`. Closing the parent closes its sheet.
5. **Material.** `neue_paint::materials::{sheet, sheet_shadow}`: the window background
   with the title bar's shadow fading down its top edge, a hairline rim down the sides and
   along the bottom, square corners; drawn in the parent it also casts a drop shadow
   beside and below it, never above. Tokens in `neue_theme::materials::sheet`.

## Consequences
- No change to the Masonry fork, the protocols or the compositor. neue-platform gains only
  `Ctx::has_modal_windows`.
- On Wayland the sheet window appears wherever neue-comp puts a new toplevel until task
  2.23c places modal dialogs under their parent's chrome. The compositor may offer resize
  edges on it (client-side decorations, 2.22d); 2.23c should treat modal dialogs as fixed
  size, or the driver should set a fixed size.
- Keyboard input to the parent window on Wayland is blocked only for Return; other keys
  reach it if the user focuses it, until neue-comp enforces modality.
- The toolbar stays clickable while a sheet is drawn in its window (the scrim covers the
  content only). No slide animation (Phase 4).
- Tab order is the tree order of the sheet: content first, then Cancel, then the default
  button.

## Revisit when
- neue-comp enforces modality for xdg-dialog, or the Masonry fork gains focus scopes.
- Sheets need to size themselves to their content, or to queue (a second sheet waiting
  for the first).
- The winit driver supports child windows on macOS.
