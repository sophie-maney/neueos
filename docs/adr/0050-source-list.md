# 0050: Source list

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.19c adds the source-list sidebar: groups of items with icons, labels and count
badges, on a tinted background, with a gradient selection. ADR 0049 anticipated it as an
outline style, and the outline already has the tree, the virtual rows, the selection and
the keys. What differs is looks, rows that can't be selected, and what a sidebar
doesn't have (a header, a border, columns, several selected rows).

## Decision
1. **`SourceList<M, Id>` wraps an `Outline<M, Id>`** built in a sidebar style
   (`Table::build(.., source_list: true)`; `Table::new` is `build(.., false)`). The style
   sets `TableState::source_list`: one column as wide as the view, no header (it isn't
   added), a vertical-only scroll view in the new `Frame::Sidebar` (no border and no focus
   ring: the selection shows the focus), rows `sidebar::ROW_HEIGHT` (22) high, and a body
   that paints the sidebar and its selection instead of the content background, stripes
   and bands. The role is `Tree`, its rows `TreeItem`s (label, badge as the description,
   level, set size, position, expanded, selected). The type is separate from `Outline` so
   its API can say what a sidebar is: one selection (`on_select(Option<&Id>)`), groups,
   entries, icons and badges.
2. **The top level is the groups.** Level 0 rows are group headers: small capitals (capital
   letters at the small size: only DejaVu Sans Regular is bundled, ADR 0038, so there is
   no bold), no icon or badge, and **not selectable**; level 1 and deeper are items.
   `TableState::selectable(row)` says which, and everything that selects asks it: a click
   or a drag over a header does nothing (and so does a click in empty space, which would
   otherwise clear the selection), Up, Down, Home, End and Page keys go to the nearest
   item, type-select skips headers, AccessKit's click does nothing, Left on a first-level
   item does not go "to its parent", and a collapse that hides the selection leaves it
   empty (`on_select(None)`) instead of selecting the header, as an outline's does.
3. **Show and Hide.** A group with items shows Show or Hide at its right end while the
   pointer is over its header (`TableState::hover`, set from pointer moves and cleared when
   the pointer leaves the body, pushed to the rows with `Look::hovered`); a press in the
   right `sidebar::ACTION_WIDTH` of the row toggles it, as AccessKit's `Expand` and
   `Collapse` do. Headers have no disclosure triangle and no keyboard toggle (as in the
   platform this reinterprets); items with items of their own have a triangle in a
   disclosure column left of the icon, and Right and Left as an outline's. Groups start
   open (`SourceList::new` expands them; `collapsed` closes some).
4. **Cells draw themselves.** `Cell` gets a source variant (`Entry`: label, `SourceIcon`,
   badge text), and `CellContent::Source` (`SourceContent`, `widgets/source_list.rs`)
   paints the whole row's parts from neue-paint's geometry (`source_item`,
   `source_badge_rect`, `source_group_x`, `source_group_action`), shared with hit-testing.
   Text on a focused selection is white with a dark shadow, the icon and the triangle too;
   a badge on the selection turns white with the gradient's colour as its text; unfocused,
   the selection is grey and its text stays dark.
5. **Materials and tokens.** neue-paint `materials::{sidebar, source_list_selection,
   source_badge, source_icon}` and `SourceState`; tokens `neue_theme::materials::sidebar`
   (metrics and a `Palette` with light, dark, graphite and increased-contrast values).
   Everything is vector and procedural.
6. **Icons are an enum for now.** `SourceIcon` (`Folder`, `Document`, `Picture`, `Music`,
   `Star`, `Clock`; `non_exhaustive`) are original flat glyphs in neue-paint, drawn in one
   colour so they follow the row's text. A general icon type (themes, SVG, colour) belongs
   to the icon and collection view (2.20); this enum becomes a case of it then.

## Consequences
- One selection only: `multiple` isn't offered. Groups can't be selected, so a keyboard
  user can't move onto a header to collapse it; the pointer and assistive technology can.
- Labels aren't truncated: a long label runs under the badge and is clipped at the edge
  (as in tables, ADR 0048). Row heights are fixed.
- `RowsWidget` and `CellWidget` know about two more cell kinds and hover; the outline
  and the table are otherwise untouched, and their goldens didn't change.
- Not yet: reordering and drag and drop of items, rename, badges as anything but text,
  a context menu (wrap an entry's cell in a `ContextMenu` when cells can be elements),
  an editable group header, tooltips for truncated labels.
