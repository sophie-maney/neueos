# 0059: HUD windows: one more kind of kit chrome, and a parent that keeps its look

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.23d. Mountain Lion has small dark floating panels (inspectors, tool palettes) with
a title bar that holds a close button and a title and nothing else. neue-paint already
has the HUD material (`materials::hud`, used for the volume indicator). A kit window
needs to use it as its chrome, and being a utility window it must not make its parent look
inactive when it takes the keyboard.

## Decision
1. **`Window::hud()`** is a third kind of client-side chrome (`ChromeKind::Hud`, beside
   the title bar and the unified toolbar of ADR 0055), not a separate widget. It reuses the
   chrome's event handling: drag the bar to move, the close button asks the app's close
   request, AccessKit "Close". It differs in what it draws and in three small things:
   - **Paint.** `materials::hud` fills the whole window (opaque tint: there is no blur
     behind a kit window yet), then a 13 px close disc in the top-left corner (a cross
     while the pointer is over it, brighter while pressed) and the title in the HUD's label
     colour, centred between two equal insets. The HUD is dark in both appearances, and
     looks the same focused or not. The window root skips its background fill: the panel
     is the chrome's. Metrics (`TITLE_HEIGHT` 19, `CLOSE_SIZE`, `CLOSE_INSET`,
     `TITLE_BASELINE`, `TITLE_INSET`) and the disc colours are tokens in
     `neue_theme::materials::hud`.
   - **One control.** Only the close button. No minimise or zoom, so a double-click on the
     bar does not zoom. `Window::toolbar` is ignored.
   - **Decorations.** Client, over a transparent surface, like every chrome window. The
     compositor's resize edges (2.22d) apply as they do to other client-side windows.
2. **Utility window = a window with a parent.** `Window::hud().parent(p)` is an
   `xdg_toplevel` with a parent, which the compositor keeps above it. No new protocol, no
   new `WindowSpec` field. Without a parent the HUD is a plain floating window.
3. **The parent keeps its focused look.** Wayland gives the keyboard to the HUD window, so
   the parent gets `WindowFocusChange(false)`. The runner tracks the HUD windows that
   have a parent and the ones with the keyboard (`Kit::{huds, huds_focused}`), and sets
   `RootWidget::set_keep_active` on the parent while any of its HUD windows has it. The
   parent's chrome then paints its focused look. When the keyboard leaves for another
   app, both lose focus and both look inactive; when the HUD closes, the flag is cleared.
   Only the chrome is affected: the parent's controls (default button, focus rings)
   still follow the real window focus.
4. **HUD windows don't outlive their parent.** Closing a window through the kit closes its
   HUD windows.
5. **`Label::on_hud()`** draws a label in the HUD's text colours. Without it, text in a
   HUD window uses the (light-theme) label colour and is unreadable in the light
   appearance.

## Consequences
- Controls inside a HUD window still use the app's light or dark theme. Dark controls need
  a per-subtree theme override (its own task); until then a HUD holds labels and the like.
- The shadow around the panel is not visible: a client-side window has no room for it
  until the compositor draws shadows (ADR 0055).
- No translucency until the backdrop-blur protocol (Phase 3): `Backdrop::Translucent` is
  the one-line change in `Chrome::paint_hud`.

## Alternatives considered
- **A compositor-drawn HUD title bar** (a window type the client asks for). Rejected for
  the reasons of ADR 0055: a protocol, nothing on macOS, split focus and AccessKit.
- **Telling the compositor to leave the parent active** (a protocol, or never giving the
  HUD keyboard focus). Not possible with core Wayland, and a HUD must take keys for its
  own controls. The look is the client's to fake.
