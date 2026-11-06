# 0090: Notification banners and alerts: one widget with button children, a portable stack model

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.15b, the second part of notifications (ADR 0089). The shell (3.15d) shows each
notification as a banner, or as an alert when it has actions or never expires, stacked at
the top right of the screen under the menu bar. The widget must be portable (gallery,
goldens on macOS), keyboard operable, and expose its buttons to assistive technology.
Replacements (`replaces_id`) update a notification in place.

## Decision
1. **Material.** `neue_paint::materials::notification`, tokens in
   `neue_theme::materials::notification`: a 340 px panel with 8 px corners, a top-to-bottom
   grey gradient (light) or charcoal (dark), a rim and top highlight, a soft shadow; the
   app icon (40 px) or an image in its place with the icon as an 18 px badge; a bold title
   on one line and the body on at most two, both ending in an ellipsis when cut; an alert
   has an 84 px column of up to two action buttons split by hairlines; the close button is
   an 18 px disc over the top left corner. `Parts` computes every rect so the widget, the
   material and the tests agree. The widget is the panel plus a 14 px margin on every side
   (shadow and close button), so its height follows the text (at least 60 px of panel).
2. **One widget, button children.** `neue_kit::desktop::Notification<M>` takes a
   `NotificationContent` (title, body, app icon, image, action labels) and reports a
   `NotificationEvent` (`Activated`, `Closed`, `Action(i)`, `Hovered(bool)`) through
   `on_event`. The close button and `MAX_ACTIONS` (2) action buttons are child widgets, so
   each is its own AccessKit button and Tab reaches it; unused action buttons are stashed.
   Because the children always exist, `Handle::set_content` can turn a banner into an
   alert and back in place, with the message handlers registered once at build time.
   Further actions are left out (the shell keeps `default` for a click on the panel).
3. **Input.** A click on the panel (not its margin) is `Activated`; the close button is
   shown while the pointer is over the panel or while it is focused. The panel takes focus
   too: Space or Return activate, Escape closes. Hovering is reported once each way so the
   shell can pause the timeout (`Store::pause`). Access clicks bubbling from a button do
   not activate the panel.
4. **Accessibility.** A banner is a `Group`, an alert an `Alert`; the label is the title,
   the description the body, live polite; the buttons are labelled "Close" and their
   action's label.
5. **Stack model.** `neue_kit::desktop::NotificationStack` is plain data stepped by the
   caller's clock: newest on top sliding in from the right (`SLIDE`, the gentle spring)
   while the others move down (`SHIFT`, standard); a removed panel slides out and keeps
   its place until gone, then the ones below close up; pushing an id already there resizes
   it in place (or brings it back if leaving); reduced motion jumps. It reports
   `placements()` (x offset, y, height) and `extent()`, the larger of the current and
   resting height: the size the stack's surface needs. It is in the kit, not the shell, so
   the widget that hosts the panels can drive it.

## Consequences
- 3.15d hosts the panels: a container widget applying `NotificationStack` placements on
  animation frames, in one layer surface per output anchored top right `SCREEN_GAP` from
  the edge and under the bar's exclusive zone, resized to `extent()`. Resizing a layer
  surface from the app is not in `neue-kit` yet; 3.15d adds it (or sizes the surface for
  the most panels it shows).
- The transparent margin around each panel takes pointer input that lands there; clicks in
  it do nothing. A future input-region request could pass them through.
- Body markup and inline replies stay out (ADR 0089 capabilities).

## Addendum (2026-11-06, task 3.15d1): layer surfaces that follow their content
- **Opt-in content sizing.** `LayerSpec::fit_content` (and `neue_kit::Window::fit_content`
  for a `Window::layer`) opens the window's `RenderRoot` with Masonry's
  `WindowSizePolicy::Content`: the root is measured at its preferred size and Masonry
  reports `SetSize` whenever that changes. The Wayland driver turns it into
  `zwlr_layer_surface_v1.set_size` (logical, rounded up so the content is never cut) and a
  commit; the compositor configures that size and the driver resizes the buffer as for any
  configure. The window's own size is only the size before the first layout. Sizes the
  compositor already has are not sent again, so the reports after each configure and
  rescale settle at once.
- **Input region = the content's rect,** set in the same commit as the new size, so a
  shrinking surface lets the pointer through at once rather than after its next frame,
  and an axis the compositor stretches (anchored to both edges) still only takes input
  over the content. With `pointer: false` the region stays empty.
- **Scope.** For surfaces anchored to at most one edge per axis (the notification stack,
  `Anchor::TOP_RIGHT`); a stretched axis stays the compositor's. The winit driver ignores
  it (a layer window is a plain toplevel there). Toplevels keep `WindowSizePolicy::User`;
  a kit `Window` that is not a layer ignores `fit_content`.
- **Why not a fixed surface for the most panels, or an input region from the widgets:** a
  fixed, oversized transparent surface swallows clicks meant for the windows below unless
  the app also maintains an input region, which needs the same size tracking; following
  the content keeps both in one place and makes the surface no bigger than it draws.
- **Consequence:** until the compositor's configure arrives (a round trip), grown content
  is drawn into the old buffer and cut at its edge. The stack's `extent()` (the larger of
  the current and resting heights) grows when a panel is pushed, before the panels have
  moved, so at most the first frame of a slide loses a few pixels.
