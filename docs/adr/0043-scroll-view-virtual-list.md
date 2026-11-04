# 0043: Scroll view with overlay scrollers, and the virtual list

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.15 adds scrolling: a scroll view with overlay scrollers, and virtual scrolling,
which the table, outline and collection views (2.19, 2.20) and the text view (2.16) build
on. neue-kit uses `masonry_core` only (ADR 0035), so Masonry's `Portal` and
`VirtualScroll` (in the `masonry` crate) are references, not dependencies. Virtual rows
come and go while the app runs, and their message handlers (`Wiring<M>`) live in the app
runner, not in the widget tree. That needs a way for widgets to hand new handlers to the
runner.

## Decision
1. **`ScrollView::new(content)`** scrolls vertically by default; `.axes(ScrollAxes::
   {Vertical, Horizontal, Both})` chooses the directions. Along an axis that doesn't
   scroll, the content is as long as the view. The content is at least as large as the
   view. Without `.width()` or `.height()` the view takes the space it's given, so in a
   `Stack` (which gives children their content height) it needs a height.
   `Handle::{scroll_to, offset}`.
2. **Scrolling is a compose-time translation** (`set_child_scroll_translation`) inside a
   clip: scrolling repaints but never re-lays-out the content.
3. **Overlay scrollers** are child widgets above the content, so they take the pointer
   where they're drawn. They are stashed (no paint, no pointer) while hidden. They show
   when the view scrolls and when the pointer comes into a view that can scroll. They fade
   (`Spring::STANDARD`) after `motion::SCROLLER_HIDE_DELAY` (1 s) without scrolling,
   unless the pointer is on one or dragging it. Under reduced motion they disappear without
   a fade. Showing is instant. Over a scroller the knob widens from 7 to 11 px on a track
   (`Spring::SNAPPY`). Dragging the knob scrolls in proportion; a press beside it moves a
   page. Where two scrollers would meet, they leave a corner gap. Paint:
   `neue_paint::materials::scroller`, with its palette and metrics in
   `neue_theme::materials::scroller`. The knob is dark on light and light on dark, and
   more opaque under increased contrast.
4. **Input.** Wheel and trackpad deltas move the view (a line is `SCROLL_LINE`, 40 px). A
   wheel that only turns vertically scrolls a horizontal-only view sideways. At an end the
   event isn't marked handled, so an enclosing view scrolls instead. The view is a Tab
   stop with a focus ring. The arrow keys move a line, Page Up and Page Down a page (the
   view less a line), Space and Shift-Space a page when the view itself has focus, and
   Home and End go to the ends. Keys a focused descendant doesn't handle bubble up to the
   view too. `Update::RequestPanToChild` (focus, `request_scroll_to`) scrolls the target
   into view, moving as little as it can.
5. **AccessKit.** The view is a `ScrollView` with `scroll_x`/`scroll_y` and their ranges,
   `clips_children`, `ScrollUp`/`Down`/`Left`/`Right` (item or page unit) and
   `SetScrollOffset`. A shown scroller is a `ScrollBar` with orientation and a 0–1 value.
6. **`VirtualList::new(count, row_height, |i| row)`** is a scroll view whose content is
   `RowsWidget`. Rows have one fixed height. Only the rows in view and half a view either
   side exist as widgets. In compose, the scroll view tells the rows which part shows
   (`mutate_child_later`). They drop the rows outside it (`remove_child`) and build the
   missing ones by calling the closure, in the same frame (the mutate pass runs within the
   rewrite loop). `Handle::{set_count, reload, scroll_to_row}`.
7. **Row handlers reach the runner through an internal action.** The row closure returns
   an `Element<M>`. Its widget goes into the tree and its `Wiring<M>`, type-erased as
   `Box<dyn Any>`, stays in `RowsWidget`. Then the rows submit `Rewired`. `Runner::action`
   intercepts `Rewired` (as it does `RootAction`), takes the pending wiring, registers it,
   and prunes the handlers of dropped rows. This is the pattern for any widget that builds
   children from app code after the window opens.
8. **The row closure can't see the model.** It is `'static` and runs inside Masonry's
   passes, where the app's state isn't reachable. Data goes through something the app
   shares with the closure (`Rc<RefCell<…>>`); after a change, the app calls `reload`.

## Consequences
- Dropped rows lose their widget state (a half-typed field, keyboard focus). Tables (2.19)
  must keep selection and editing state in the app, not in row widgets.
- Assistive technology sees only the built rows of a list (the `List` node has no item
  count yet). Tables will need `row_count` and row indices (2.19).
- RequestPanToChild doesn't account for an inner view's scroll offset (a Masonry
  limitation), so scrolling into view through nested scroll views is only right for the
  innermost one.
- Not here: rubber-band overscroll and momentum (macOS sends momentum from the OS; on
  Wayland it is ours to synthesise), the "always show scroll bars" preference, variable
  row heights, horizontal or grid virtualisation (2.20 adds the grid), and scrolling the
  gallery's own page area.
- Goldens: 28 in `apps/gallery/tests/golden` (scroll view at rest, scrolled, with the
  scroller hovered and focused; both axes scrolled; virtual list at rest and far down,
  where the knob is at its minimum length), × light/dark × 1x/2x. `Fixture::{hover_at,
  scroll}` drive them.

## Alternatives considered
- **A fetch round-trip through the app** (Masonry's `VirtualScroll`): the list asks for
  rows with a message, and `update` supplies them with access to the model. This is
  closer to the Elm model, but every list needs a message variant and a handler. Rows also
  arrive a frame late, and goldens can't render a list without a running app. This can be
  added on top later as a data-source layer for tables.
- **Scrollers painted by the scroll view in `post_paint`**: simpler, but hit-testing
  would send presses on a scroller to the content underneath.
- **Depending on the `masonry` crate's `Portal`**: its scrollbars aren't overlay
  scrollers, its theme isn't ours, and app crates must not see Masonry types (ADR 0035).

## Addendum (2026-11-04, task 2.20b): the virtual grid
- **`VirtualGrid::new(count, cell_width, cell_height, |i| item)`** is the list with
  several items to a row. It is the same `RowsWidget` with a cell width, so building,
  dropping, `Rewired` and the handler pruning of §6 and §7 are unchanged. A list is a grid
  with one column and cells as wide as the view.
- **Columns come from the width:** `floor(width / cell_width)`, at least one. The scroll
  view's window carries the viewport width, so `RowsWidget::set_window` sets the column
  count, and a change requests layout. The vertical measure uses the width it's given, so
  the content height is right in the first pass. The slack of a row is spread evenly: each
  item is centred in a cell of `width / columns`, so items carry their own spacing.
- **Which items exist:** whole rows. The rows near the window (half a window either side)
  give the item range `first_row * columns .. min(last_row * columns, count)`; the same
  range arithmetic as the list's.
- **`Handle<VirtualGrid>::{set_count, reload, scroll_to_item}`** match the list's; scroll
  to an item reveals its row.
- **Consequences:** a resize that changes the column count moves items between rows, so
  items keep no state across it beyond what the app holds (as with scrolling). Item sizes
  are uniform; AccessKit still sees a plain `List` of the built items (`Grid` and
  `GridCell` with counts arrive with the collection view, 2.20c). The column count is
  `pub(crate)` for now; the collection view reads it for 2D arrow keys.
