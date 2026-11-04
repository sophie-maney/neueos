# 0052: Collection view

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.20c adds the collection view: items of an icon over a label in a grid, with
selection and keys. ADR 0043's addendum made `RowsWidget` a grid (`VirtualGrid`), and
ADR 0051 added the `Icon` type. What is left is the items, the selection and the keys.
A table's body already does selection on the same widget, but in terms of rows, bands and
columns; the grid's keys and hit-testing differ.

## Decision
1. **A collection body next to the table body.** `RowsWidget` gets a second optional hook
   set, `collection: Option<Box<CollectionBody>>`, as it has `table`. The hooks
   (pointer, text, access, paint, accessibility, restyle) live in
   `widgets/collection.rs` as `impl RowsWidget`, and `RowsWidget` calls them when it has a
   collection. They reuse the table's `Selection`, `TableAction` (the body reports indices;
   the builder turns them into ids), `Look`, `command` and the `Later` trait, which became
   `pub(super)`. Not one body type for both: a table's body is about rows and a column
   header, and sharing the keys would have meant a flag in every branch.
2. **`Collection<M, Id>` takes ids, not a count.** `Collection::new(ids, |&id| Item)`;
   the ids are kept in an `IdList` (a `Vec` and an id-to-index map) that the builder, the
   items and the body share, with the id type erased behind a trait for the body (as the
   outline's tree is). The app speaks in ids (`on_select(&[Id])`, `on_activate(&Id)`,
   `Handle::{set_items, set_selection, scroll_to}`); the body speaks in indices. The
   selection is kept by id through `set_items` (reordering, filtering), and ids that are
   gone are deselected without a message. A million items cost a `Vec` and a map of ids,
   and only the items near the view exist as widgets.
3. **Icon size sets the cell.** `materials::collection::cell_size(icon)` gives the cell:
   the icon in a backdrop, a 17 px label pill under it, insets around, at least 72 px wide.
   `Collection::icon_size` (default 48, clamped to 16 to 128) and `Handle::set_icon_size`
   change it; the grid's cell size follows (`RowsWidget::set_cell`) and the items are built
   again. The builder's option reaches the grid when the widget is added.
4. **Items draw their own selection.** `CollectionItemWidget` paints the icon, the
   selection's rounded backdrop behind it and the label in a pill (the accent with white
   text while the collection has focus; grey with the normal label colour without). The
   body paints the content background. Colours are the semantic ones plus
   `collection::Palette` (the icon backdrop, light and dark); a disabled collection
   shows no selection. A label that is too wide is cut at the front with an ellipsis,
   found by binary search over its characters when the width changes.
5. **Selection and keys.** Click selects; with `multiple`, Shift extends from the anchor
   and Cmd (Ctrl elsewhere) toggles, **in item order** (not a rectangle); a click between
   items clears the selection (Shift or Cmd keeps it); a double-click activates. Left and
   Right move by one item (across the end of a row), Up and Down by a row (Down from under
   a short last row goes to the last item), Home and End to the ends, Page Up and Page
   Down by the rows of a view less one; Shift extends; Cmd-A selects all with `multiple`;
   typing selects the first item whose label starts with it (the menus' `TypeSelect`);
   Return activates when the app listens. The keys scroll the selection into view.
6. **Accessibility.** The body is a `Grid` with the row and column counts of the current
   layout and multiselectable; each built item is a `GridCell` with its label, row and
   column index and selected state, and takes a click. The items hold the shared state so
   that their indices follow a change of columns.
7. **Frame.** The scroll view uses `Frame::Border` (a hairline and the focus ring), so
   the collection is a Tab stop that shows its focus; icon views with no border can come
   later.

## Consequences
- `neue_kit::{Collection, Item}`; `neue_theme::materials::collection`.
- Goldens: `collection` (normal) and `collection-selected` (normal = selected without
  focus, focused, disabled), each light and dark at 1× and 2×.
- Not yet: drag and drop, rename, variable item sizes, rubber-band selection, sections,
  gaps between cells, horizontal scrolling, icons that aren't on a grid line (a list or
  column mode), a context menu on items beyond wrapping the collection in `ContextMenu`.
- Linux aarch64 and x86_64 goldens were not run (macOS only), and the VM check
  (`docs/checks/phase-2.md` §16) is not run.
