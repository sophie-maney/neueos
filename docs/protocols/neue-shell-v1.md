# `neue_shell_v1`

Our private Wayland extension between neue-comp and neue-shell (task 3.1, ADR 0066).

- **XML:** `crates/neue-protocols/protocols/neue-shell-v1.xml`. Licence: GPL-3.0-or-later.
- **Offered:** `neue_shell_manager_v1` version 1, only to the shell: the client that
  `neue-comp --shell <command>` spawned with `WAYLAND_SOCKET`. Others don't see it, and
  binding it anyway is a protocol error.
- **Bindings:** `neue_protocols::neue_shell::{server, client}`, behind the `server` and
  `client` features.

## Interfaces
**`neue_shell_manager_v1`**
- Event `toplevel(new neue_shell_toplevel_v1)`: one per window that is mapped or
  minimised. On bind, every existing window, then one `active`.
- Event `active(toplevel or null)`: the window whose app owns the menu bar. It stays
  while a shell menu, a popup or the lock screen has the keyboard.
- Event `system_key(key, state, modifiers)`: volume, mute, mic mute, screen and keyboard
  brightness, play, pause, next, previous, stop, eject; pressed and released. They skip
  the focused client while a shell is bound; a compositor binding for the key wins.
- Request `run_action(new neue_shell_action_v1, name, toplevel or null)`: a `neuectl`
  action; null is the active window. The result object gets `done` or `failed(message)`
  after the request's dispatch.
- Request `send_shortcut(modifiers, keysym)`: the active window gets the keyboard (if a
  shell menu had it) and the chord, typed with the current layout. Close the menu first.
- Request `destroy`.

**`neue_shell_toplevel_v1`** (state events are applied together on `done`; after the
first batch only changes are sent)
- `identifier("window-N")` once, first: N is the `neuectl` window id, and the string is
  the window's `ext_foreign_toplevel_handle_v1` identifier.
- `title`, `app_id` (empty when unset), `state` (bits `minimized` 1, `attention` 2).
- `icon(name or null)` then zero or more `icon_buffer(fd, width, height, scale)`: sealed
  memfds of `width × height` ARGB8888 pixels, rows without padding. Map them or read at
  offset 0 (the file offset is shared). No name and no buffers: use the app id's icon.
- `menu_dbusmenu(service, path)`, `menu_gmenu(bus_name, menubar_path?, app_menu_path?,
  application_path?, window_path?)` or `menu_none` (show the fallback menu). The strings
  passed only a syntax check (ADR 0025).
- `done`, `closed` (then destroy the object).
- Requests `activate` (restores a minimised window), `minimize`, `unminimize`, `close`,
  `set_minimize_rectangle(surface, x, y, width, height)` (on one of the shell's layer
  surfaces, surface-local; width or height 0 unsets), `destroy`.

## What uses it
- **neue-comp** (`src/neue_shell.rs`, `src/shell_launch.rs`): the global, the snapshots
  it diffs per window, key routing (`input.rs`), the minimise target
  (`State::minimize_target`), and starting and restarting the shell.
- **neue-shell** (task 3.2 onward): menu bar, dock and OSDs.

## Changing it
A new request, event or enum entry is a new version: bump the interface version in the
XML, the compositor's `VERSION` and the shell's bind range, and keep older shells working
(the compositor sends only what the bound version knows). Incompatible changes need
`neue_shell_v2`.
