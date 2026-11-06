# 0095: The dock's other section: a divider, minimised-window tiles and the Trash

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.17c. After the apps the dock shows the windows that are minimised and the Trash.
Thumbnails of the minimised windows need a toplevel-capture client the shell doesn't have
(ADR 0023 is the compositor side), so they are 3.17d; this task shows the app's icon.

## Decision
1. **Tiles.** `neue_shell::dock::TileKind` is `App`, `Minimised(ToplevelId)`, `Trash(TrashState)`
   or `Divider`. `Dock::tiles` puts, after the apps, a divider (when anything follows), a
   tile for each minimised window (oldest first; keyed `minimised-<identifier>`; named for
   the window's title, else its app) and the Trash (`TRASH_KEY`; icons `user-trash` and
   `user-trash-full` from the theme). A minimised window also stays in its app's tile, so the
   running light, the cycling click and Quit still see it.
2. **Kit.** `DockTile` gains `kind: DockTileKind { App, Minimised, Divider }` and
   `fixed`. Minimised tiles and dividers are always fixed; a fixed tile is clicked but
   never dragged (a press that moves is nothing). The divider takes a tile's slot but no
   pointer, focus or menu, and has no AccessKit label (a generic container). Apps can
   still be dropped among the section; the model keeps them after the last kept app.
3. **Painting** (`neue-paint::materials::dock_shelf`): `dock_divider` (a thin line, upright
   on any edge) and `minimised_window` (a small window with a title bar, with the app's
   icon as a badge in its bottom right corner; 3.17d replaces the body with the thumbnail).
   Colours and metrics are in `neue_theme::materials::dock_shelf` with light and dark
   variants.
4. **Minimise targets.** A window minimises to its app's tile; once minimised, to its own
   tile, where it is restored from (`minimize_targets`).
5. **Clicks and menus.** A minimised tile's click restores it; its menu is Restore and
   Close. The Trash's click opens it with `xdg-open trash:///` (we have no file manager
   yet); its menu is Open and Empty Trash (disabled while empty), which empties it through
   `neue_desktop::Trash::empty` on a thread, with no confirmation.
6. **Trash state** is polled every 2 s on a thread (`TRASH_POLL`) and looked at again as
   soon as it was emptied. No tile when there is no home directory or the trash can't be
   read.

## Alternatives considered
- **A narrow divider.** The fisheye layout assumes one pitch for every tile; a narrower
  slot would need per-slot widths in `DockLayout::place`. The divider takes a whole slot
  for now.
- **inotify on the trash.** Needs a new dependency; the poll is one `read_dir` every 2 s.
- **Moving minimised windows out of their app's tile.** It would drop the running light of
  an app whose windows are all minimised.

## Consequences
- `DockTile` and `Dock` gain public API in `neue-kit`; `Tile` gains `kind`.
- The Trash tile can't be dragged off the dock, and unlike on other desktops cannot be
  removed from it.

## Not done
- Thumbnails (3.17d), a confirmation before Empty Trash, trash on other volumes (needs the
  mount table), dropping files on the Trash, the poof, the Trash tile's "full" artwork
  (the theme's icons are used).
