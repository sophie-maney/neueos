# Global menu on Wayland: research (2026-11-03)

Desk research only; nothing was run. Confidence: H = primary source seen, M = forum or secondary, L = inferred.

## Coverage

| Toolkit | Exports on Wayland? | Mechanism | Requirements | Conf. |
|---|---|---|---|---|
| Qt6 | Yes | `org_kde_kwin_appmenu` + DBusMenu | plasma-integration and `QT_QPA_PLATFORMTHEME=kde`; registrar needed? verify | M |
| Qt5 | Same, if Qt5 plasma-integration exists | Same | Low priority | L |
| GTK3, `GtkApplication` menubar | Yes, no module | `gtk_shell1.set_dbus_properties` -> `org.gtk.Menus`/`org.gtk.Actions` | Compositor implements `gtk_shell1` | M |
| GTK3, `GtkMenuBar` widgets | No | `appmenu-gtk-module` is X11-only | n/a | M |
| GTK4 / libadwaita | Only apps calling `set_menubar` (same Wayland code path, seen in source) | Same as GTK3 | Most libadwaita apps use a menu button and export nothing; no GtkModules | H/M |
| Chromium / Chrome | Yes (137+) | Ozone enables it only when `org_kde_kwin_appmenu_manager` is bound | Registrar probably; flag unknown | H/M |
| Electron | Broken. Fix PR #54629 open | Would use `org_kde_kwin_appmenu` | Hides its in-window menu, shows nothing | H |
| Firefox | Yes (138+), off by default | kde-appmenu client + DBusMenu | Prefs `widget.gtk.global-menu.enabled` and `.wayland.enabled`; blank-bar bug 1995226 | M |
| Thunderbird | Prefs reportedly ineffective | n/a | Retest | L |
| LibreOffice | Unclear (kf6 plugin may work via Qt) | Qt path | Flatpak ships GTK plugin only | L |
| Java / Swing | No | Agents are X11-only | n/a | L |
| Flatpak | Per toolkit, plus sandbox | See above | Historically `--talk-name=com.canonical.AppMenu.Registrar` | M |

X11 routes (`_KDE_NET_WM_APPMENU_*`, `_GTK_*` properties, Chromium's `GDK_BACKEND=x11` trick) do not count. Nothing found says xwayland-satellite translates them, so X11 apps get the fallback menu.

## What we must implement

1. `org_kde_kwin_appmenu_manager` v2 (`create`, `release`) and `org_kde_kwin_appmenu` v2 (`set_address(service, path)`). The MIT crate `wayland-protocols-plasma` has server bindings.
2. `gtk_shell1` v5 with `gtk_surface1.set_dbus_properties` (app id, app menu path, menubar path, window path, application path, unique bus name). Advertise `global_menu_bar`.
3. DBusMenu client: `com.canonical.dbusmenu` (`GetLayout`, `AboutToShow`, `Event`, `LayoutUpdated`, `ItemsPropertiesUpdated`).
4. GMenuModel client: `org.gtk.Menus` (`Start`, `End`, `Changed`) and `org.gtk.Actions` (`DescribeAll`, `Activate`, `Changed`).
5. Own `com.canonical.AppMenu.Registrar` (`RegisterWindow`, `UnregisterWindow`, `GetMenuForWindow`); cheap, likely required.
6. Map the focused toplevel to its endpoint; otherwise use the fallback menu.

The upstream successor, `xdg-dbus-annotation` (wayland-protocols MR 52), is still unmerged and the KWin/Plasma ports are Drafts since Jan 2024. Treat it as later work.

## Rust crates

- `dbusmenu-zbus` 0.1.0 (MIT/Apache-2.0): proxies only, zbus 4, so a version mismatch is likely.
- `system-tray` (MIT): SNI + DBusMenu client on zbus 5; the best reference.
- `gio` (MIT): `DBusMenuModel` works, but it wraps C GLib, so it needs an ADR.
- No pure-Rust `org.gtk.Menus` client found; write one in `neue-dbus`.

## Other shells

Hyprland: nothing native; the third-party `hypr-appmenu` plugin bridges the KDE protocol to the registrar (licence unstated). mango: request closed as not planned. waybar, ironbar, niri, Wayfire, COSMIC: nothing found (unconfirmed absence). Plasma itself: Qt only; GTK still broken (KDE bug 424485).

## Recommendation

Keep the PLAN.md design with one correction: for GTK on Wayland use `gtk_shell1`, not `appmenu-gtk-module`. Order: kde-appmenu + registrar + DBusMenu client (Qt, Chromium, Firefox); then `gtk_shell1` + GMenuModel; fallback menu for everything else (Electron, libadwaita, X11, Java).

## VM verification (2026-11-03, task 0.7)

Run on KWin (Plasma 6.8.2 Wayland) in the Parallels VM; details, versions and screenshots in
`global-menu-checklist.md`. "Tested" = run and seen; no input injection was available, so
clicks were sent as DBusMenu `Event`s, not mouse clicks.

| Toolkit | Status | Result on KWin | vs. the desk research above |
|---|---|---|---|
| Qt6 (Kate, Dolphin) | tested | Exports, widget shows the menu. No registrar: no export. | **No `plasma-integration` needed:** `qt6-wayland` 6.12 has the client built in; still exports with `QT_QPA_PLATFORMTHEME=generic`. Registrar required. |
| Qt5 | not run | | |
| GTK3 `GtkApplication` | tested (partial) | `org.gtk.Menus` exported on the bus with no module; KWin has no `gtk_shell1`, so no protocol request to observe | Module not needed for the bus export. Wayland side unverified. |
| GTK3 `GtkMenuBar`, GTK4 `set_menubar` | not run | | |
| GTK4 / libadwaita (Loupe) | tested | Nothing exported | As expected |
| Chromium 156 | tested | Exports with no flag; waits for the registrar and exports when it appears | Registrar required, confirmed |
| Firefox 159 | tested, failed | Never calls `create`, even with both prefs on | Prefs alone are not enough here; unresolved |
| Electron (VS Code 1.139.1, Flatpak) | tested | Nothing exported; in-window menu stays visible | R1 said the in-window menu is hidden; it was not |
| Thunderbird | not run | | |
| LibreOffice 26.8 | tested | `kf6`: works. `qt6`: `set_address` sent, widget empty. `gtk3`: no export and the in-window menu is gone | `gtk3` plugin can leave no menu at all |
| Java / Swing | not run | | |
| Flatpak (Kate) | tested | Works when the sandbox may talk to `com.canonical.AppMenu.Registrar` (Kate ships that); without it nothing is exported. The host can call into the sandbox | Confirmed |
| X11 app (GTK3 under Xwayland) | tested | The widget showed the menu, via Plasma's `gmenudbusmenuproxy` and the `_GTK_*` X11 properties | R1 said X11 apps get nothing; true only without that proxy. xwayland-satellite untested |

Consequences for ADR 0005: we need our own `com.canonical.AppMenu.Registrar` first (Qt,
Chromium and Flatpak apps all depend on it). Then the KDE protocol, then `gtk_shell1` with a
GMenuModel client, checked on our compositor (KWin cannot show it). Electron, Firefox and
libadwaita get the fallback menu until retested.

## Sources

wayland.app/protocols/kde-appmenu; wayland.app/protocols/gtk-shell; bugs.kde.org #424485, #397625; community.kde.org/Plasma/Wayland_Showstoppers; discuss.kde.org/t/global-menu-not-working-with-some-applications/34158; discourse.nixos.org (topic 80226); bugzilla.mozilla.org #1995226; github.com/electron/electron PR 54629; invent.kde.org kwin MR 5064; github.com/memtable/hypr-appmenu; github.com/mangowm/mango/issues/832; docs.rs `dbusmenu-zbus`, `system-tray`; GTK source `gtk/gtkapplication-wayland.c`; Chromium `ozone_platform_wayland.cc`.
