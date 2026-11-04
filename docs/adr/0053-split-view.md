# 0053: Split view

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.21a adds the split view: panes side by side or stacked with draggable dividers, as
in `NSSplitView`. The sidebar-and-content layout of every app needs it, and the window
chrome (2.22) will sit over it.

## Decision
1. **One widget owns the panes and the dividers.** `SplitWidget` (Masonry) holds the panes
   as children and draws and hit-tests the dividers itself; a divider is a gap in the
   layout, not a widget. This keeps resizing, collapsing and keys in one place. Not a
   `Stack` with divider widgets between the children: the dividers' drags change two
   panes at once, and the limits are about pairs.
2. **Pane lengths are state.** The widget keeps the length of every pane along the split
   (collapsed ones zero) and a remembered length for each collapsed pane. On layout, `fit`
   clamps every pane to its range and hands the difference to the view's length to the
   panes that don't hold their size, in proportion to their length; if none can take it,
   to the ones that do. A pane's first length is its `Pane::size`; panes without one
   share what is left. Positions are rounded to whole logical pixels.
3. **A divider moves a pair.** Dragging, keys and `Handle::set_position` ask for a length
   of the pane before the divider; the pair's total stays fixed, and `drag_pair` clamps
   it to both panes' ranges. A collapsible pane collapses when asked for less than half
   its minimum (`COLLAPSE_AT`), as long as the other can take its room; it comes back the
   same way, or by double-click, Return, or an arrow key in the direction that would
   grow it, at the length it had.
4. **Two divider styles**, `DividerStyle::{Thin, Thick}` in `neue-paint`: a 1 px line, or
   a 9 px bar with a gradient across, two edge lines and a grip of three dots.
   Colours and metrics are `neue_theme::materials::split`; disabled dims the dividers.
5. **Keyboard.** The view is one Tab stop. The arrows along the split move the active
   divider by 10 px, the ones across pick the previous or next divider (with three or
   more panes), Home and End move it to either end (collapsing a collapsible pane there),
   and Return collapses or restores. The active divider shows the focus ring.
   (The task plan said Home/End collapse; moving to the ends is the WAI-ARIA splitter
   behaviour, and collapsing follows when the pane is collapsible.)
6. **Accessibility.** Role `Splitter` with the orientation of the bar, the numeric value
   of the active divider's first pane, a minimum and a maximum; `Increment`, `Decrement`
   and `SetValue` move it.
7. **Messages.** `on_resize(&[f64])` sends every pane's length after a user change;
   changes through a `Handle` send nothing.

## Consequences
- `neue_kit::{SplitView, Pane, DividerStyle}`; `neue_paint::materials::{DividerStyle,
  split_divider}`; `neue_theme::materials::split`.
- Goldens: `split-thin`, `split-thick` (normal, focused, disabled), `split-stacked`
  (normal, focused), `split-collapsed`; gallery page "Panes".
- Not yet: saved positions (needs config), animated collapse, a wider grab area for thin
  dividers (a press must land on the 1 px), dragging past a pane to resize the one after
  it, a divider with a custom view (a toolbar in it).
