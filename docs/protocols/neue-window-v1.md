# `neue_window_v1`

Our private Wayland extension: what a window that draws its own chrome tells neue-comp
about it (task 2.35, ADR 0064).

- **XML:** `crates/neue-protocols/protocols/neue-window-v1.xml`. Licence: GPL-3.0-or-later.
- **Offered:** `neue_window_manager_v1` version 1, to every client (sandboxed ones too).
- **Bindings:** `neue_protocols::neue_window::{server, client}`, behind the `server` and
  `client` features.

## Interfaces
- `neue_window_manager_v1.get_window(id, wl_surface)` makes a `neue_window_v1` for a
  surface. One at a time per surface; a second one is the `window_exists` error.
  `destroy` leaves the objects it made alone.
- `neue_window_v1.set_chrome_height(uint)`: where the chrome the client draws at the top
  of its window (title bar and toolbar) ends, in surface-local logical pixels.
  Double-buffered: applied on the next `wl_surface.commit`, so in step with the frame
  that draws it. Used while the surface is an `xdg_toplevel`.
- `neue_window_v1.destroy`: every fact is unset on the next commit, and the surface may
  get a new object.

## What uses it
- **neue-comp** (`src/neue_window.rs`) keeps the committed height in the surface's cached
  state. A sheet (a modal `xdg-dialog` with a parent, ADR 0058) hangs from it: its top
  sits that far below the top of the surface, measured from the parent's window geometry
  and clamped to it. Unset, the sheet hangs from the top of the window geometry (or under
  our server-side title bar). The Phase 4 slide animation can start from the same line.
- **neue-platform**'s Wayland driver sends it for toplevels whose `WindowSpec` carries a
  `ChromeHeight`, after each layout, only when it changed. neue-kit's window root records
  its chrome's laid-out height there (rounded up), so toolbar display modes and zoom
  follow on the next frame.

## Changing it
A new request or event is a new version: bump the interface version in the XML, the
compositor's `VERSION` and the driver's bind range, and keep older clients working.
Incompatible changes need `neue_window_v2`.
