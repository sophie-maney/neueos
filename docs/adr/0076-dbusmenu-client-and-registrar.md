# 0076: The global menu's DBusMenu side: registrar, client and the bar's app menus

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 3.7a (split from 3.7). ADR 0005 has the shell own `com.canonical.AppMenu.Registrar`
and read apps' `com.canonical.dbusmenu` menus with a client on zbus in `neue-dbus`. The
compositor already sends each window's menu address over `neue_shell_v1` (ADR 0025,
0066), but the Wayland driver dropped those events, and the menu bar had no way to change
its titles at runtime. Open questions: how the client runs beside the kit's UI thread,
how updates and `AboutToShow` work with menus that open synchronously, and what to do
with icons.

## Decision
1. **`neue_dbus::menu_client::MenuClient`** wraps one exported menu with a blocking
   proxy (no property cache). `fetch()` reads `GetLayout(0, -1, [])`, then sends
   `AboutToShowGroup` (falling back to `AboutToShow` per id) for empty submenus: every
   menu-bar title without children and every item saying `children-display: submenu`
   with none, up to 3 rounds and 64 ids a round, reading the layout again when the app
   says something changed. That fills lazy menus (LibreOffice) before they are shown.
   `event(id, name)` sends `clicked`, `opened`, `closed` with a millisecond timestamp.
2. **Parsing is `MenuNode::from_layout`**, the export's own type, so both sides share it:
   hidden items are dropped, mnemonic markers removed (`_File` → `File`, `__` → `_`),
   only the first shortcut kept, unknown or mistyped properties take their defaults.
   `MenuNode` gains `icon: Option<MenuIcon>` (`icon-name` or `icon-data` PNG), which the
   export now serves too.
3. **Updates: one signal stream for the bus, not one per menu.** `MenuSignals` is a
   blocking iterator over every `com.canonical.dbusmenu` `LayoutUpdated` and
   `ItemsPropertiesUpdated` (one match rule), yielding sender and path. Either signal
   means "read the whole menu again"; menus are small and partial property updates aren't
   worth a second code path. A per-menu subscription would need a thread or task per
   followed window, and zbus's `Executor::spawn` is hidden from its public API.
4. **`neue_dbus::registrar::Registrar`** serves the interface (`RegisterWindow`,
   `UnregisterWindow`, `GetMenuForWindow`, `GetMenus`, both signals) and `own()` takes
   the name with `ReplaceExisting | AllowReplacement`. On Wayland apps only need the name
   to exist; registrations are kept for completeness.
5. **The shell's worker** (`neue-shell` binary, `global_menu.rs`) is one thread started
   with `cx.spawn`. It owns a session connection with a 2 s method timeout
   (`neue_dbus::connect_session_client`, so a hung app can't stall it), the registrar,
   and a second thread forwarding `MenuSignals`. The UI thread sends `Follow(address)`
   when the active window's menu address changes and item events; the worker coalesces
   queued input (a burst of signals is one read) and sends back the menu tree. Every
   `Follow` starts a new **generation** on both sides; events and trees of another
   generation are dropped, so a click on a title that was just replaced never reaches
   the next app.
6. **Kit:** `MenuBar` gets a third group of titles between the fixed ones and the extras
   (`with_menu`, `Handle<MenuBar>::set_menus`, which replaces them all and prunes their
   handlers). `MenuTitle` gets `on_open`/`on_close` messages (its action type becomes
   `TitleAction { Opened, Chosen, Closed }`); `set_menu` keeps only the items.
7. **Opening a menu** sends `AboutToShow` and `opened` for its title, and `closed` when it
   closes. Kit menus open synchronously, so an app that only fills a menu in
   `aboutToShow` shows what it had at the last read on that first opening; the new
   tree comes with the update. **While one of the app's menus is up, new trees are held
   back** and applied when it closes: replacing a title under its own open menu would
   orphan the menu.
8. **Icons are read but not drawn.** The bar's menus, like Mountain Lion's, show none.
   The parsed icon stays in `MenuNode` for the status-icon tray (3.13), which may.
9. **Shortcuts** map `Control`, `Alt`, `Shift` to the kit's modifiers and `Super` to its
   command key, until 3.10 decides how Super and Ctrl show in menus.
10. **The platform's `Toplevel`** gains `menu: ToplevelMenu` (`None`, `DbusMenu`,
    `GMenu`), filled from `menu_dbusmenu`/`menu_gmenu`/`menu_none`. GMenu addresses are
    passed through for 3.8; the shell follows only DBusMenu ones.

## Consequences
- Tests: `neue-dbus` round-trips every property through export and parse, and 5 client
  tests run against our own export over a peer-to-peer connection (whole tree, events,
  change signal and re-read, errors, registrar). Kit tests: app menus placed after the
  fixed titles, replaced whole, and their open, item and close messages. Shell tests:
  conversion to kit menus, shortcut mapping, matching signals to the followed menu.
- In the VM: the gallery's View and Pages and KWrite's (Qt6) File … Help show in the bar,
  KWrite hides its own menu bar, and the bar follows the active window. Killing the shell
  brings the registrar and KWrite's menus back when the compositor restarts it (ADR 0005
  §7 holds for Qt).
- The navigation between titles (hover, Left/Right, a focus shortcut) is 3.7b.
- A bar too narrow for every title lets them run under the extras; truncation can come
  with 3.7b or later.
