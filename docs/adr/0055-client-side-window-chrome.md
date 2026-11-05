# 0055: Client-side window chrome for toolkit windows, one material for both sides

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.22 gives neue-kit windows Mountain Lion chrome: a title bar that can grow into a
unified title bar and toolbar, under one gradient, with toolbar items inside it. The
compositor already draws a 22 px server-side bar with traffic lights for clients that
accept server-side decorations (ADR 0013), with hover, press, drag, double-click and
resize edges (ADR 0014). The kit's Wayland driver asks for `ServerDefault` today, so kit
windows currently get that bar. PLAN §3.1 says kit apps draw their own unified toolbars
"using the same `neue-paint` code so both look identical". This ADR settles how.

## Decision
1. **Kit windows with chrome draw it themselves (client-side).** A kit window that has
   chrome (2.22b: `Window::toolbar`) draws the bar, the traffic lights, the title and the
   toolbar items in its own surface. The Wayland driver asks for client-side decorations
   through `xdg-decoration` for those windows; neue-comp grants an explicit client-side
   request (ADR 0013 §1), so there is no double bar. Kit windows without chrome (layer
   surfaces, popups) are unchanged. On macOS, the winit driver keeps the native shadow,
   corners and resizing by using a transparent, full-size title bar with the native title
   and buttons hidden. Our chrome is drawn underneath, so it looks the same as on Linux.
2. **One material, any height.** `neue_paint::materials::window_chrome(scene, width,
   height, style)` draws the bar with its rounded top corners, the top highlight, the
   bottom separator and the traffic lights. One gradient spans the whole height. The
   traffic lights and the title stay in the top [`TITLE_BAR_HEIGHT`] (22 px) row, and
   the rest is room for the toolbar. `window_chrome_titled` also draws a `neue-paint`
   title. The server-side bar is the 22 px case: the slices are drawn by the same
   private `background` and `button` code, a unit test checks that `window_chrome` at
   22 px matches `title_bar` pixel for pixel (±2), and the `titlebar-*` goldens did not
   change. The kit draws the title with its own Masonry text, but uses the colours of
   `BarPalette::new`, `title_x` and `TITLE_BASELINE`, so both titles sit in the same
   place.
3. **Behaviour comes from the platform's window signals.** Drag to move, resize,
   zoom (maximise) and minimise use Masonry's `RenderRootSignal::{DragWindow,
   DragResizeWindow, ToggleMaximized, Minimize}`, which both drivers already turn into
   `xdg_toplevel.{move, resize, set_maximized, set_minimized}` and winit calls. Close
   goes through the app's normal close request. Traffic-light hover and press use the
   same `TitleBarStyle` states as the compositor's bar. A double-click on the bar zooms.
   Kit windows do not read the compositor's `title-bar-double-click` setting yet.
4. **The compositor provides the resize edges.** Client-side windows get the same 5 px
   invisible edges outside their frame as server-side ones (2.22d). Kit surfaces don't
   need an input region larger than their window geometry, and no shadow margin in the
   buffer.

## Consequences
- Toolbar items are ordinary kit widgets in the window's widget tree, with focus,
  AccessKit and hit-testing for free. The compositor sees a plain client-side window.
- Each kit window repaints its chrome on the GPU with the rest of its scene, instead
  of using the compositor's cached CPU slices. The bar is a few fills, so this is cheap.
- There are no shadows for client-side windows until the compositor's shadow shader
  arrives (PLAN §3.1). It will draw them from the window geometry, so nothing changes in
  the client.
- The top corners must be transparent, so kit toplevels with chrome need a surface
  with alpha (2.22b).
- New goldens: `chrome-{22,52}-{focused,unfocused}-{light,dark}@{1x,2x}` in `neue-paint`.

## Alternatives considered
- **Extend the server-side bar with a private protocol** (the client tells neue-comp how
  tall its toolbar is, and the compositor paints the gradient behind the client's
  toolbar area or draws the whole bar). Rejected:
  - it needs a new protocol;
  - it doesn't work on macOS (winit) or under other compositors;
  - the toolbar items are client widgets, so the client would have to draw onto a
    background it doesn't own and match the compositor's frame timing on resize;
  - focus, AccessKit and hit-testing for the bar would be split across processes.
- **Client-side chrome drawn by the client, with resize edges and a shadow in the
  client's own buffer** (the GTK/libadwaita way): it works under any compositor, but it
  needs a larger buffer and input region on every window and a client-side shadow. It
  could be added later for foreign compositors if we ever need them.
- **A separate tall gradient with its own stops for unified bars.** Not needed now. One
  gradient over the whole height keeps the 22 px case identical. If side-by-side
  comparisons show that taller bars need a darker bottom, revisit this in
  `neue_theme::materials::title_bar`.

## Revisit when
- The compositor gains shadows (check that client-side windows get them).
- Clients should follow `title-bar-double-click` (it needs a settings channel, Phase 3).
- Kit apps have to run under a compositor other than neue-comp.
