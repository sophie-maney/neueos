# 0084: The status-icon tray: StatusNotifierItem watcher, host and item client

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.13. Apps put status icons (network applet, chat clients, sync tools) in the bar's
right side through the StatusNotifierItem protocol (`org.kde.StatusNotifierWatcher`,
`org.kde.StatusNotifierItem`, menus as `com.canonical.dbusmenu`). The shell has to own the
watcher and host the items. No new dependency: this is zbus, as ADR 0005 and 0047 say.
3.13 is split like 3.12: 3.13a (this ADR) is the D-Bus side in `neue-dbus`, 3.13b the
bar's tray area in the kit and `neue_shell::bar`, 3.13c the shell wiring.

## Decision
1. **Module.** `neue_dbus::status_notifier` has `watcher` (the served interface),
   `item` (`ItemInfo`, `StatusItem`, `ItemSignals`) and `host` (`Host`). Same pattern as the
   other clients: blocking zbus, signal streams as iterators the caller runs on threads of
   its own, and no thread started by the library except the watcher's (item 2).
2. **Watcher.** `Watcher::own` serves `org.kde.StatusNotifierWatcher` at
   `/StatusNotifierWatcher`, takes the name like the appmenu registrar does (replacing an
   owner that allows it, replaceable by a later one), and forgets items and hosts whose
   connection leaves the bus (a thread on `NameOwnerChanged`, ending when the watcher is
   dropped). Items are kept and signalled as `bus/path` whatever form they registered in: a
   bus name (path `/StatusNotifierItem`), `bus/path`, or a bare path (libappindicator; the
   bus is the sender). At most 256 items and 256 hosts, so an app can't grow it forever.
   `ProtocolVersion` is 0.
3. **Host.** `Host` holds the list of items and what each last showed, and runs no thread:
   the caller subscribes to `WatcherSignals` and `ItemSignals` first, calls
   `Host::initial`, then feeds every signal to `registered`, `unregistered` or `changed`,
   which return a `HostEvent` (`Added`, `Changed`, `Removed`) or `None` when nothing
   visible changed. Every item signal (`NewIcon`, `NewToolTip`, `NewStatus`, ...) means
   "read it again" (`GetAll`): apps send them for no change often, and the host compares.
   An item whose properties can't be read yet is shown at its first later signal. A signal
   is matched to an item by its sender's unique name (resolved when the item registers) and
   path. `Host::announce` takes `org.kde.StatusNotifierHost-<pid>-1` and registers it, which
   is what makes apps show their icon instead of falling back to a window.
4. **Lenient reading.** `ItemInfo::from_properties` treats a missing or mistyped property as
   none, and drops a malformed pixmap, since apps differ widely. A pixmap is converted from
   the wire's ARGB to straight RGBA, at most 512 × 512. `Status` defaults to Active and
   `Category` to ApplicationStatus when unknown. `Menu` `/` means none. `IconThemePath`
   (libappindicator and Qt) is read; using it is up to the shell.
5. **Actions.** `Host::{activate, secondary_activate, context_menu, scroll}` take the item's
   id and run on the caller's thread, so the shell runs each through `cx.spawn`.
   `Host::menu` gives a `MenuClient` for the item's DBusMenu (3.7a); an item with
   `ItemIsMenu` expects the shell to open it instead of calling `Activate` (3.13c).
6. **Not done here.** The overlay icon, the animated attention movie and the legacy
   `org.ayatana.NotificationItem` interface are ignored (Ayatana indicators that use only
   the old interface don't appear; libappindicator uses the new one). A watcher owned by
   someone else going away isn't followed (the shell owns it).

## Consequences
- The tests run over peer-to-peer connections with a stand-in item, as the other D-Bus
  clients' do; `sni_host` (owns the watcher, prints what a tray would show) and
  `mock_sni_item` (a stand-in app) check it on a real bus with no shell
  (`docs/dev-setup.md` §1.6d).
- Reading an item costs one `GetAll` per signal.
- A second status-notifier host on the same bus works alongside ours (hosts are a list),
  but two watchers can't coexist: ours replaces one that allows it.

## Addendum: the shell wiring (task 3.13c)
- **Worker.** `tray::run` (in the shell process) connects to the session bus, owns the
  watcher (`Watcher::own`, held for as long as the worker runs), subscribes to the watcher,
  item and DBusMenu signals on a thread each, announces the host, reads what is registered
  and then feeds every signal to the `Host`. It sends the UI thread `Update`s: an `Item`
  (added or changed, icons already pictures), a `Menu` (the items of its DBusMenu) or
  `Removed`; and once the `Host`, which the UI thread keeps for the actions. If the name is
  taken by a watcher that won't leave, the worker ends and logs why: no tray.
- **Menus.** A menu is read when its item is added or its `Menu` path changes, and again on
  a `LayoutUpdated` or `ItemsPropertiesUpdated` from that path (matched by path only: a
  needless read of another app's menu is the worst a clash does). Each read runs on a
  thread of its own, at most one per item at a time and one more after it if the menu
  changed meanwhile, so an app that never answers holds nothing up. The menu is kept as
  read, so a right click opens at once; `AboutToShow` is sent while reading (3.7a), not
  again when the menu opens.
- **Icons.** A name is looked up in the item's `IconThemePath` (loose files or a theme laid
  out under it) first, then in the icon theme (`neueOS`, then Papirus; the name moves to
  `appearance.toml` when icon themes become a setting); an icon that can't be found falls
  back to the best pixmap. Everything is loaded for scale 2 and fitted to the bar's 16 px.
  An item with neither shows the placeholder.
- **Tooltip.** Title and text with the markup stripped (`<br>` is a space, entities are
  decoded): `Title: text`.
- **Presses** (the kit's `MenuTitle::on_press` and `on_context`, ADR 0085 §8): a primary
  press sends `Activate`; a secondary press opens the menu, or sends `ContextMenu` when
  the item has no menu (or it isn't read yet). An item with `ItemIsMenu` and a menu has
  no press: any press opens the menu. Return and Space press, Down and the menu key open
  the menu, and assistive technologies get Click and ShowContextMenu. The position sent
  is 0, 0: a layer surface doesn't know its place on the screen.
- **Items come and go.** Items arrive in registration order (the first is rightmost, ADR
  0085). A changed icon, name, tooltip or status is edited in place; an item that came, left,
  got another menu or acts differently needs new titles (`Handle::set_tray`), which wait
  while one of the tray's menus is up. Menu choices send `clicked` through `cx.spawn`, as
  do `Activate` and `ContextMenu`; results are logged on failure.
- **Not done.** `opened` and `closed` events for the root, scrolling over an item
  (`Scroll`), the middle click (`SecondaryActivate`) and keyboard stepping between tray
  icons.
- `mock_sni_item` now exports its menu as real tray apps do (the items under the root, not
  under a title).
