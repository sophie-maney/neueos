# 0005: Global-menu strategy

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
The menu bar shows the focused app's menus (PLAN.md §3.3). R1 (`docs/research/global-menu.md`)
and the 0.7 VM run (`docs/research/global-menu-checklist.md`) found:
- Qt6 (built into `qt6-wayland`, no `plasma-integration` needed), Chromium and
  LibreOffice-kf6 export DBusMenu through the KDE appmenu Wayland protocol, **but only while
  `com.canonical.AppMenu.Registrar` is on the session bus**. Flatpak apps also need
  permission to talk to that name.
- GTK3/GTK4 apps using `GtkApplication` menus export `org.gtk.Menus`/`org.gtk.Actions` and
  announce them through `gtk_shell1.set_dbus_properties`, not through
  `appmenu-gtk-module` (X11-only). `GtkMenuBar` widget apps export nothing on Wayland.
- libadwaita apps, Electron and Firefox (in our test) export nothing usable.
- X11 apps only export through X11 window properties.
- The upstream successor protocol (`xdg-dbus-annotation`) is unmerged.

## Decision
1. **Split of work:**
   - `neue-comp` implements the Wayland side only: `org_kde_kwin_appmenu_manager` v2 (server
     bindings from `wayland-protocols-plasma`, MIT) and `gtk_shell1` v5 (XML vendored in
     `neue-protocols`; other `gtk_shell1` requests are accepted as no-ops). It stores a menu
     address per toplevel: either `DbusMenu { service, path }` or `GMenu { bus_name,
     menubar_path, app_menu_path, application_path, window_path }`.
   - The compositor passes the focused toplevel's address to the shell through
     `neue-shell-v1` (Phase 3). Before that, `neuectl windows` exposes it for testing.
   - `neue-shell` does all D-Bus work: it owns `com.canonical.AppMenu.Registrar`, and runs a
     DBusMenu client and a GMenuModel client, both written on zbus in `neue-dbus`. No `gio`
     (C) and no `dbusmenu-zbus` (old zbus); the `system-tray` crate is the reference.
2. **Order:** (a) registrar + KDE appmenu + DBusMenu client (Qt, Chromium, LibreOffice-kf6,
   neue-kit apps); (b) `gtk_shell1` + GMenuModel client (GTK3/GTK4 `GtkApplication` apps),
   verified on our compositor since KWin can't show it; (c) the fallback menu for
   everything else.
3. **neue-kit apps** export through the same path as Qt (DBusMenu + KDE appmenu), so the
   shell has one primary code path and neue-kit can be tested against third-party shells.
4. **Fallback menu** for apps with no address (libadwaita, Electron, Firefox, X11 apps,
   Java, `GtkMenuBar` apps): App (About, Hide, Quit), File (New Window, Close), Edit (Undo,
   Cut, Copy, Paste, Select All), Window (Minimise, Zoom, Bring All to Front), Help (menu
   search). Edit items ask the compositor, over `neue-shell-v1`, to send the matching key
   chord (`Ctrl+Z`, `Ctrl+X`, …) to the focused surface only. Window items are compositor
   actions. "New Window" uses the desktop entry's `new-window` action if present, otherwise
   it's disabled.
5. **No X11 bridging.** We don't port `gmenudbusmenuproxy` or read `_KDE_NET_WM_APPMENU_*` /
   `_GTK_*` properties; that would be X11 code (AGENTS.md). X11 apps get the fallback menu.
6. **Flatpak:** we can't change app manifests. Apps that request
   `--talk-name=com.canonical.AppMenu.Registrar` work; others get the fallback menu. We
   document this and may ship `flatpak override` defaults later.
7. **Shell restarts:** when the shell restarts, the registrar name disappears and comes back.
   Phase 3 must test that Qt and Chromium re-export on the name's return; if they don't, the
   registrar moves to a tiny separate service.
8. **Later:** `xdg-dbus-annotation` once it's merged upstream and KWin/GTK ship it.
   Retest Firefox (prefs plus the blank-bar bug), Electron (PR #54629), and
   xwayland-satellite when they change.

## Consequences
- §3.1's protocol list gains `gtk_shell1`; §3.3's pipeline no longer mentions
  `appmenu-gtk-module` or `plasma-integration` for menus.
- `neue-dbus` grows two clients and a registrar; `neue-comp` stays D-Bus-free (ADR 0003).
- The fallback menu is a first-class feature, not a stopgap: libadwaita and Electron are a
  large share of everyday apps.
- `neue-shell-v1` must carry: menu address of the focused toplevel, and a request to send
  a key chord to it.

## Alternatives considered
- **Compositor owns the registrar and D-Bus clients:** puts zbus into the calloop process for
  no gain; the shell renders the menus anyway.
- **`gio` for `org.gtk.Menus`:** works, but it's a C library with its own main loop.
- **Waiting for `xdg-dbus-annotation`:** no merge date; it would leave us with no menus.
- **Per-app in-window menu scraping or accessibility-tree menus (AT-SPI):** fragile, slow,
  and breaks with every app update.
