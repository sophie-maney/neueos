# 0047: DBusMenu export: the neue-dbus crate and window menu bars

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
ADR 0005 has neue-kit apps export their menus the way Qt does: `com.canonical.dbusmenu` on
the session bus, with the address given to the compositor through
`org_kde_kwin_appmenu` (which neue-comp already serves, ADR 0025). ADR 0045 made the menu
model ready for it (depth-first item ids, public read accessors). Task 2.18 adds the export.
Open questions: which zbus runtime, how the D-Bus thread hands chosen items to the UI
thread, what the app API looks like, and how the address reaches the Wayland driver.

## Decision
1. **New crate `neue-dbus`** (Linux-only, in xtask's `LINUX_ONLY_CRATES`), on zbus 5.19
   (MIT, already in the lockfile through `accesskit_unix`). It doesn't know neue-kit:
   `menu::MenuNode` is plain data (id, label, enabled, separator, check/radio state,
   shortcut key names, children), so the shell's DBusMenu client (Phase 3) can share the
   wire type `menu::Layout` (`(ia{sv}av)`, children as variants).
2. **zbus with its own `async-io` executor and the blocking API, not tokio.** neue-kit
   apps have no async runtime; zbus answers method calls on its executor thread. This
   amends AGENTS.md's "services use tokio with zbus" for library code shared with apps;
   services may still enable zbus's `tokio` feature for themselves. `p2p` is on too, for
   `neue_dbus::testing::pair()` (tests without a bus daemon).
3. **`MenuExport`** serves one tree at a path: `GetLayout` (with depth and property
   filters), `GetGroupProperties`, `GetProperty`, `Event`/`EventGroup` (`clicked` on an
   enabled leaf item calls the export's callback; other events are accepted),
   `AboutToShow`/`AboutToShowGroup` (never needs an update: the layout is always whole),
   and the `Version` 3, `TextDirection`, `Status`, `IconThemePath` properties. Properties
   at their default value are left out; labels double `_` (DBusMenu's mnemonic marker).
   `set_items` replaces the tree, bumps the revision and emits `LayoutUpdated(rev, 0)`;
   `ItemsPropertiesUpdated` and `ItemActivationRequested` are declared but not sent yet.
4. **`neue_dbus::connect_session()`** serves an `org.freedesktop.DBus.ObjectManager` at
   `/org/neueos/MenuBar` from the connection builder. Besides listing the exported menus,
   this makes zbus start its object server before `build()` returns: started lazily by
   the first export, it dropped a method call arriving in between (seen as a 1-in-4 hang
   in the tests). Exports must use such a connection.
5. **App API:** `Window::menu_bar(Menu<M>)` and `Cx::set_menu_bar(window, menu)` (an empty
   menu removes it). Each window exports at `/org/neueos/MenuBar/<window id>`; DBusMenu
   item ids are the model's depth-first ids plus one (0 is the root). The session-bus
   connection is made on the first menu bar, on the UI thread (one short handshake); if
   it fails, a warning is logged and the app runs without a global menu.
6. **Chosen items** come from zbus's thread as `(window, id)` over a channel plus the
   app's waker; `update` gets the item's message when the UI thread drains its queue
   (after queued and mailed messages). Handlers stay on the UI thread, as for widgets.
7. **Address:** `neue_platform::{MenuAddress, Ctx::set_menu_address}`, queued like opening
   windows and applied by the driver after it creates them. The Wayland driver binds
   `org_kde_kwin_appmenu_manager` v1–2 (`wayland-protocols-plasma`, MIT), creates the
   window's appmenu object on the first address and sends `set_address(unique name,
   path)`; `None`, and closing the window, `release` it (v2). Toplevels only. The winit
   driver ignores addresses.

## Consequences
- Tests: `neue-dbus` unit tests (properties, depth) and 6 client tests over a
  peer-to-peer connection (layout, depth and property filters, clicks incl. disabled
  items and submenus, group properties, update signal, object removal); 3 neue-kit tests
  (export of a window's menu bar, chosen items reach `update` and replacing the menu
  reaches clients, closing removes it). All pass on Linux aarch64; x86_64 not run.
- On a real session bus in OrbStack (`dbus-run-session`, the `menu_demo` example):
  `gdbus introspect` shows the interface, `GetLayout` the tree, `Event` reaches the
  callback, `GetManagedObjects` lists the menu. The Wayland address in a real session is
  unverified: `docs/checks/phase-2.md` §10.
- The gallery has a menu bar (View toggles, Pages radio items) for that check.
- Peer-to-peer connections have no unique name, so no address is set for them.
- Not yet: menu bars on macOS hosts (needs the native menu or our own); key equivalents;
  icons; `ItemsPropertiesUpdated` for cheap toggles (every change resends the layout);
  in-argument names in introspection follow Rust (`parent_id`), not the spec's camelCase
  (names aren't on the wire).
