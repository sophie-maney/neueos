# 0014: Chrome interaction (task 1.11)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
ADR 0013 draws server-side title bars but leaves them dead: clicks fall through, there are no
hover states, no drag to move, and a server-decorated window has no resize edges. The bar is
not part of the client's surface, so Smithay's space cannot hit-test it.

## Decision
1. **Hit-testing** (`interaction::hit_test`, a pure function). A window's *frame* is its
   client geometry plus the bar (`decorations::frame_geometry`). Inside the bar a point is a
   traffic light (`TrafficLight::rect`, so drawing and hit-testing share one source) or the
   plain title bar. Resize edges are **invisible and outside the frame**: 5 px
   (`RESIZE_MARGIN`) on every side, widened to corners within 12 px (`RESIZE_CORNER`) of an
   end. They never cover client pixels, and a zoomed window has none.
2. **Stacking.** `State::chrome_hit_at` walks the windows top to bottom and stops at the one
   that `Space::element_under` says has a client surface under the point, so a window in
   front hides the chrome of one behind it. `State::surface_under` returns nothing for
   chrome: the pointer has no client focus there and clients never see it.
3. **Pointer grabs.** A left press on chrome starts a grab (`Focus::Clear`), so the press and
   release never reach a client. Anything but a traffic light also raises and focuses the
   window; a traffic-light click does not, as on the original. Other buttons only focus.
   - `TrafficLightGrab`: the button shows pressed while the pointer is over it; releasing on
     it runs the action (close: `xdg_toplevel.close`; minimise; zoom/restore). Dragging off
     the button and releasing does nothing.
   - `TitleBarGrab`: a press is a click until the pointer travels 3 px (`DRAG_THRESHOLD`),
     then it becomes a move (a zoomed window unzooms under the pointer, as for
     `xdg_toplevel.move`, through the shared `State::begin_move`). Waiting for the threshold
     keeps a double click on a zoomed window from unzooming it on the first press.
   - Edges start the existing `ResizeGrab` (and its resize cursor).
   Grabs read the pointer position from their `PointerInnerHandle`, never from the
   `PointerHandle`, which is locked while a grab runs.
4. **Double click.** Two non-dragging clicks on the same bar within 400 ms (event
   timestamps, so tests can script it) run `[windows] title-bar-double-click` in
   `compositor.toml`: `"zoom"` (default), `"minimize"` or `"none"`. The setting is read when
   the click happens, so a reload applies at once.
5. **Hover and cursor.** On motion without a grab, `State::update_chrome_hover` records which
   window's lights the pointer is over (`Chrome::hover`) and sets the cursor: the resize
   cursor on edges, the arrow on the rest of the chrome (clients cannot set it there, as
   they have no focus), back to the client's own on leaving.
6. **Drawing.** `TitleBarStyle` gains `hover` (all three lights show their glyphs, and in an
   unfocused window their colour, as on the original) and `pressed` (one light darkened and
   flattened). Only the left slice depends on them: `Chrome::slices` renders just that slice
   for an interactive style and shares the plain style's middle and right slices, and the
   title is rendered with `without_interaction()`, so hovering re-renders about 70×22
   logical pixels and nothing else. Goldens: `titlebar-{hover-focused,hover-unfocused,
   pressed-close,pressed-minimize,pressed-zoom}-{light,dark}@{1x,2x}`.

## Consequences
- `compositor.toml` gains `[windows]` (`neue-config::WindowsConfig`), defaulted, so old
  configs keep working.
- `render::frame_geometry` moved to `decorations` (the render module is feature-gated and
  headless tests need it).
- Hover is only updated on pointer motion. A window that moves under a still pointer (a
  keyboard action) updates its hover on the next motion.
- Touch and tablets do not drag the bar or the edges yet.

## Alternatives considered
- **Edges inside the frame:** would steal the client's outermost pixels (scrollbars at the
  window edge) and need a bigger grab area to be usable on the bar.
- **Starting the move on press:** simpler, but unzooms on the first click of a double click.
- **A `SpaceElement` wrapper including the bar** (see ADR 0013): still deferred to Overview.

## Revisit when
- Overview or Spaces need chrome in the element tree (Phase 4).
- An appearance setting or theme tokens (Phase 2/3) change the metrics (`RESIZE_MARGIN`).
