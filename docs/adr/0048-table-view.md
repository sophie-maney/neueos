# 0048: Table view

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.19a builds the table (2.19b's outline view and 2.19c's source list follow its
pattern). It needs a fixed header above rows that scroll, rows built only while on screen
(ADR 0043), a selection the rows must draw, column widths the header and every row share,
and a structure assistive technology can read as a table. Masonry gives each widget one
action type, no inherited state, and no way to change a sibling from an event except
through a deferred mutation.

## Decision
1. **`Table::new(columns, count, |row, column| cell)`.** `Column` has a title, a width,
   minimum and maximum widths, an alignment (`Align::Start`, `Center`, `End`) and
   `sortable`. A `Cell<M>` is text (`Cell::text`, or any `&str`/`String`) or an element
   (`Cell::element`). The cell closure can't see the model; as with a virtual list, data
   goes through shared state and `Handle::reload` (ADR 0043 §8). Builder options:
   `multiple`, `alternating_rows`, `selected`, `sorted`, `on_select(|&[usize]| M)`,
   `on_activate(|usize| M)`, `on_sort(|usize, SortOrder| M)`, `width`, `height`,
   `disabled`. `Handle::{set_count, reload, set_selection, set_sorted, scroll_to_row,
   set_enabled}`; changes through the handle send no message.
2. **Structure.** The table is a scroll view (both axes) with a `Frame::Border` (a
   hairline drawn in `post_paint`, because the view's clip, inside the border, also
   applies to its own `paint`) and a **header slot**: the header sits above the viewport,
   inside the border, gets the content's width, and is translated by the horizontal offset
   only. The vertical scroller starts below it. Scroll-into-view requests subtract the
   viewport's origin. The scroll view tells virtual rows the visible rectangle, not only its
   vertical span, so a row can be revealed without scrolling sideways.
3. **The body is the virtual list's `RowsWidget` with a `TableBody`.** It reuses the row
   building, dropping and rewiring of ADR 0043 §6–7, and adds: keyboard focus (the scroll
   view is no Tab stop; its focus ring shows for focus inside), pointer and key selection,
   type-select (the first column's text cells, `TypeSelect` from menus), activation, and
   painting the content background, stripes (which go on past the last row) and selection
   bands behind the rows. Its table events (`TableAction::{Selected, Activated}`) are
   submitted untyped, as its typed action stays `Rewired`.
4. **Shared state.** `TableState` (widths, `Selection` with anchor and cursor, emphasis,
   options) is an `Rc<RefCell<…>>` shared by the builder, the body and the row-building
   closure, so a row is built with the right widths and look. After a change, the body
   pushes looks to the rows that exist through `mutate_self_later` (events can't reach
   children's `WidgetMut`). Rows (`TableRowWidget`) lay out one `CellWidget` per column;
   the last column stretches to fill the view. A text cell draws its text (white on a
   focused selection, the disabled colour when disabled); an element cell holds the element,
   which keeps its own colours.
5. **Selection.** Click selects one row; with `multiple`, Shift extends from the anchor and
   Cmd (Ctrl off macOS) toggles; without it, Cmd-click on the selected row deselects. A
   press on a row and a drag selects the rows passed over; a click below the rows clears
   the selection. Up/Down (Shift extends), Home/End, Page Up/Down (a view less a row),
   Cmd/Ctrl-A (multiple). Return and double-click activate only when the app listens
   (`on_activate`); otherwise Return reaches the default button. The selection is drawn in
   `selection` while the body has focus in an active window, `unfocused_selection`
   otherwise.
6. **Header.** `HeaderWidget` paints the bar (`neue_paint::materials::table_header`:
   gradient, dividers, pressed title, sort triangle) and takes the pointer. A click on a
   sortable title sorts by it, ascending first, then reversing, and sends `Sorted`; the
   app sorts and reloads. Dragging within `DIVIDER_GRAB` of a column's right edge resizes
   it within its limits and sends the new widths to the body (`mutate_later`), which
   pushes them to its rows; the cursor shows `ColResize`. `ColumnHeaderWidget`s draw the
   titles (clipped before the sort triangle) and take no pointer.
7. **Assistive technology.** The body is a `Table` with the full `row_count`, the
   `column_count`, `multiselectable`, and the cursor's row as `active_descendant`. Rows are
   `Row`s with `row_index`, `selected` and `Click` (which selects). Cells are `Cell`s with
   `column_index` and a text cell's text as label. The header is a `Row` of
   `ColumnHeader`s with label, `column_index`, `sort_direction` and `Click` (sortable).
   The header row is a sibling of the table node, inside the scroll view, not a child.
8. **Tokens.** `neue_theme::materials::table`: `ROW_HEIGHT` 19, `HEADER_HEIGHT` 17,
   insets, default and minimum column widths, `DIVIDER_GRAB`, sort-triangle metrics, and a
   palette (header gradient, pressed, border, dividers, titles, sort triangle, frame) for
   light and dark, firmer under increased contrast. Row colours are the semantic
   `content_background`, `alternate_row`, `selection`, `unfocused_selection`,
   `selected_label`.

## Consequences
- The outline view (2.19b) can reuse the body, rows, cells and header: it adds indentation
  and disclosure to the first column and flattens the visible tree into rows.
- Selecting all of a huge table builds a set of every index; ranges would scale better.
- Type-select builds the first cell of each row until a match, so it is linear in the
  rows.
- Not yet: variable row heights, editable cells, column reordering, drag and drop of rows,
  auto-size on a divider double-click, autoscroll while drag-selecting past the view's
  edge, ellipsis truncation (cells clip), column header nodes under the table node, and
  selected element cells turning white.
