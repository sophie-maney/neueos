# 0070: `neue-desktop` v0

- **Status:** Accepted
- **Date:** 2026-11-05
- **Extends:** 0051 (icons gain a raster form)

## Context
Task 3.3. The dock (3.16), launcher, search and Preferences all need the same freedesktop
plumbing: which apps are installed, which entry a window belongs to, what its icon looks
like, how to start it, and whether the trash is full. PLAN.md §5 names the crate
`neue-desktop` ("desktop entries, icon themes, MIME, thumbnails, trash, …"); v0 is the
first four plus launching.

## Decision
1. **A portable crate, `neue-desktop`,** that only reads files and starts processes. It
   depends on `neue-paint` (for `Icon`), `png`, `resvg`, `thiserror` and `tracing`; no D-Bus,
   no Wayland. It builds and tests on macOS and Linux against fixture directories made in
   temp dirs.
2. **Desktop entries** (`entry`): the Desktop Entry Specification's keys that the shell
   uses, strings translated for a `Locale` at parse time (`lang_COUNTRY@MODIFIER` fallback
   order), `Actions` (with `new-window` as a named accessor), `NoDisplay`, `Hidden`,
   `OnlyShowIn` / `NotShowIn`, `TryExec`. `exec_argv` splits the `Exec` line (double quotes,
   their escapes) and expands `%f %F %u %U %i %c %k %%`; deprecated codes vanish. Paths and
   URLs convert as the code needs (`file://` decoded for `%f`, paths percent-encoded for
   `%u`).
3. **App database and matching** (`apps`): `applications` directories in priority order
   (`XDG_DATA_HOME`, `XDG_DATA_DIRS`, and the Flatpak export directories whether or not
   the session's `XDG_DATA_DIRS` lists them). The desktop file ID is the relative path with
   `/` turned into `-`; the highest-priority directory wins and a `Hidden` entry masks the
   ones below. `find_by_app_id` tries, in order, over all entries: the file ID, `X-Flatpak`,
   `StartupWMClass`, those three ignoring case, the last part of a reverse-DNS ID (either
   way round), the program in `Exec`, `Name`, and the part after a vendor prefix. Every rule
   runs over all entries before the next, so an exact file name beats a class that happens to
   match. `NoDisplay` entries still match (they have icons); `Hidden` ones and non-
   applications never do.
4. **Icon themes** (`icon_theme`): the Icon Theme Specification's lookup. The search order is
   the chosen theme, its `Inherits` (depth first), the fallback theme (Papirus on neueOS)
   and its parents, `hicolor`, then `pixmaps`. Each directory has `Fixed`, `Scalable` or
   `Threshold` sizing and a `Scale`; an exactly matching directory wins, otherwise the one
   with the smallest distance in device pixels. Within a directory PNG is tried before SVG.
   `index.theme` lists are comma-separated (unlike desktop entries). Absolute paths and
   names with a known extension are accepted, as `Icon=` has both in the wild. XPM is not
   loaded.
5. **Loading into `Icon`** (`icon_load`): ADR 0051's `VectorIcon` holds filled paths only,
   which real icon themes are not (strokes, gradients, groups, clip paths). So icons from a
   theme are **raster**: `neue_paint::icon::Icon::raster(Picture)`, a new variant, drawn
   through the same texture cache as the desktop picture (ADR 0069), resampled for the
   device scale, centred and aspect-kept in the rectangle. A PNG keeps its own size; an SVG
   is rasterised with `resvg` at `size × scale` pixels, so the loader is asked for the size
   it will be drawn at. `Icon::has_own_colors()` (true for vector and raster icons) replaces
   neue-kit's `vector().is_some()` test for dimming disabled icons. `IconLoader` caches per
   `(name, size, scale)`. Limits: files up to 8 MiB, PNGs up to 16 megapixels, SVGs
   rasterised up to 2048 px on a side. A file that fails logs a warning once and counts as
   not found, so callers show a generic icon.
6. **Launching** (`launch`): `command()` builds the `Command` (for tests and callers that
   want to adjust it); `launch()` runs it detached (its own process group, stdio to null, a
   thread reaps it). The `XDG_ACTIVATION_TOKEN` (and `DESKTOP_STARTUP_ID`, for toolkits that
   still read it) is the one the caller got from the compositor with `xdg-activation-v1`; the
   crate does not talk Wayland. Without one both variables are removed, so a stale token
   from the shell's own environment never leaks into an app. `GIO_LAUNCHED_DESKTOP_FILE` is
   set as GLib does. `Terminal=true` entries need the caller's terminal command
   (`["foot", "-e"]`); `DBusActivatable` entries without an `Exec` are refused for now.
7. **Trash** (`trash`): the home trash (`$XDG_DATA_HOME/Trash`) and any further trash
   directories the caller adds with `with_dir`. `state()` is `Empty` or `Full` by whether
   `files/` has anything, `empty()` deletes `files/`, `info/` and `directorysizes`, never
   follows links and carries on after an item it cannot delete. Finding other volumes'
   `.Trash-$uid` directories needs the mount table; that arrives with the dock's Trash tile
   (3.16).

## Alternatives considered
- **Flatten SVG into `VectorIcon`.** Would keep icons resolution-independent, but needs
  strokes, gradients and groups in `VectorIcon` (a rewrite of ADR 0051 §2) and a flattener;
  Papirus uses all of them. Rasterising at the drawn size is sharp enough for 16–128 px
  icons and takes one dependency. A resolution-independent path can come later behind the
  same `Icon` type.
- **`freedesktop-desktop-entry` / `freedesktop-icons` crates.** Small, but we need the
  match rules, our own `Icon` output and exact control of fallbacks; the specs are short.
- **`gio`.** A C dependency (hard rule).

## Dependencies and licences
- `resvg` 0.47 and `usvg` 0.47: Apache-2.0 OR MIT. `tiny-skia`: BSD-3-Clause. All pure
  Rust, GPL-3.0-or-later compatible. Built with `default-features = false`: no text (so no
  `fontdb`, `rustybuzz`), no system fonts (so no fontconfig), no raster images inside
  SVGs. Icon SVGs are paths; an SVG with `<text>` renders without it. They were already in
  `Cargo.lock` as Masonry's dev-dependency (with default features).
- `png` 0.18: MIT OR Apache-2.0, shared with the desktop picture (ADR 0069).

## Consequences
- The dock and launcher can ask `AppDatabase::find_by_app_id(app_id)` for an entry, and
  `IconLoader::load(entry.icon, size, scale)` for its picture, and show `Glyph::Document`
  when either is `None`.
- A raster icon is not recoloured for the selection or the dark appearance. Themed
  monochrome icons (symbolic) therefore don't follow the text colour yet.
- Not in v0: MIME lookup (shared-mime-info), thumbnails, recent files, autostart, XPM,
  D-Bus activation, watching the directories for changes, other volumes' trash.
