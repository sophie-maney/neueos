# 0049: Outline view

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.19b adds an outline view: a table whose first column is a tree. The table of
ADR 0048 already has virtual rows, selection, keys, sorting and resizing. What is new is a
tree the app describes, expansion state that must survive data changes, disclosure
triangles, and a selection that has to stay on the same items when rows appear and
disappear above it. The source list (2.19c) will be an outline style.

## Decision
1. **`Outline<M, Id>` wraps a `Table<M>`.** `Id` names a node (a path, a key) and must be
   stable: expansion and selection are remembered by it. The app gives
   `children(Option<&Id>) -> Vec<Id>` (`None`: the top level) and `cell(&Id, column)`. A
   node is expandable when it has children, or as `Outline::with_expandable` says (so a
   folder isn't read until opened). Like a table's cell closure, these can't see the
   model; `Handle::reload` reads the tree again. Builders: `expanded`, `selected`,
   `multiple`, `alternating_rows`, `sorted`, `on_select(&[Id])`, `on_activate(&Id)`,
   `on_sort`, `width`, `height`, `disabled`, `bind`. `Handle::{reload, set_expanded,
   set_selection, scroll_to, set_enabled}` send no message.
2. **The visible rows are flattened and virtualised.** `Tree<Id>` holds the flattened rows
   (id, level, expandable, position and size among siblings) and the set of expanded ids,
   and is shared as `Rc<RefCell<dyn FlatTree>>` in the table's `TableState`. `FlatTree` is
   the object-safe view the body needs (row info, parent, `set_expanded`), so the
   widgets stay free of the app's node type. The table's row count is the tree's; a
   change sets the count and rebuilds the visible rows in one edit
   (`RowsWidget::edit_count_and_reload`: rows built by a count change aren't in the tree
   yet to be dropped by a second edit).
3. **Expansion is remembered by id.** Collapsing keeps what was open below, so opening
   again shows the same state; a recursive collapse forgets it. A recursive expansion opens
   every descendant, to 64 levels so a tree that loops ends. A node that is hidden (or
   gone) only remembers: it returns expanded.
4. **The selection follows its rows.** An edit takes the rows to follow (the selected
   rows, the anchor and the cursor) and returns where each went. Rows outside the rebuilt
   range move by arithmetic; rows inside it are found by id. Rows that are no longer
   visible leave the selection; if a collapse hid some, the collapsed row is selected
   instead. The app's `on_select` gets ids, and only when they differ from those last
   sent, so expanding a row above the selection sends nothing.
5. **Cells.** The first column's cell indents by level (`OUTLINE_INDENT`, 14) and reserves
   a disclosure column (`DISCLOSURE_WIDTH`, 14) so leaves line up; text and element cells
   both start after it. The triangle (`neue_paint::materials::disclosure_triangle`;
   right when closed, down when open) is `Palette::disclosure`, white on a focused
   selection and the disabled colour when disabled. Geometry is in
   `neue_paint::materials::{disclosure_rect, outline_text_inset}`, shared by painting and
   hit-testing.
6. **Input.** A press in a row's disclosure column expands or collapses it, with every
   level below on Option (Alt off macOS), and leaves the selection alone. Right opens
   the cursor row, or moves to its first child if it is open; Left closes it, or moves to
   its parent; with Option they act on all levels below. Other keys are the table's.
7. **Assistive technology.** The body is a `TreeGrid` (`Table` for a plain table) with the
   row count. Rows have `level` (from 1), `expanded` (expandable rows only),
   `size_of_set`, `position_in_set`, and the `Expand`/`Collapse` actions, which toggle.
   Level, size and position are stored per row at flatten time, so reading them is not
   linear in the tree.

## Consequences
- Expanding or collapsing rebuilds the visible rows (there is no animation yet). The
  cost is the visible rows, not the tree.
- Flattening asks the app for each visible node's expandability (its children, unless
  `with_expandable`), and a recursive expansion reads every descendant.
- `index_of` and following a hidden selection by id scan the rows or the rebuilt range.
- The `Selection` is still a set of row numbers; remapping a huge selection is linear in it.
- Not yet: animated expansion, lazy loading with a placeholder row, drag and drop,
  expand/collapse notifications to the app (`on_expand`), and `*` (expand all) on the keypad.
