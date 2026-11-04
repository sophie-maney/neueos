# 0025: Global-menu protocol side (task 1.17)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
ADR 0005 split the global menu: the compositor learns where each app exports its menus,
and the shell does the D-Bus work. Task 1.17 is the compositor half. ADR 0005 left open how
the address is exposed before `neue-shell-v1` exists, which `gtk_shell1` version to offer,
and what to announce in `gtk_shell1.capabilities`.

## Decision
1. **Globals.** `org_kde_kwin_appmenu_manager` v2 (server bindings from
   `wayland-protocols-plasma`, MIT, already in the lockfile through Smithay) and
   `gtk_shell1` v5. Both are offered to every client, sandboxed ones included: Flatpak GTK
   and Qt apps need them.
2. **`neue-protocols` crate** holds `protocols/gtk-shell.xml`, GTK's file vendored
   unchanged (LGPL-2.1-or-later, compatible with GPL-3.0-or-later; see
   `docs/protocols/gtk-shell.md`), and generates bindings with `wayland-scanner`. Features
   `server` (the compositor) and `client` (tests). It builds on macOS too. No new C
   dependency; `wayland-scanner` and `wayland-backend` were already in the tree.
3. **Version 5, not the XML's 7.** v6 only promises that surface offsets work, v7 adds
   `set_a11y_properties` (accessibility paths); we do neither, so we do not claim them.
   GTK binds the lower of its version and ours.
4. **One address per toplevel surface**, kept in the surface's `data_map`. KDE and GTK
   addresses are held separately and a surface that uses both shows the KDE one; when that
   object goes, the GTK one is the fallback. An object that is destroyed (`release` or a
   dead client) removes only the address it set, so a newer object's address is not wiped
   by an older one's destruction.
5. **Validation.** The compositor does no D-Bus, but the shell will pass these strings to
   zbus, so they are checked here: bus names (unique or well known, at most 255 bytes) and
   object paths (`/` or `/a/b`, `[A-Za-z0-9_]` elements). KDE: an invalid pair clears the
   address (no stale one). GTK: no valid `unique_bus_name`, or no valid path at all, means
   no address; an invalid single path is dropped and the rest kept. The `application_id`
   argument is not stored (`app_id` is in `WindowInfo` already). None of these are protocol
   errors: the client keeps running, with no global menu.
6. **`neuectl`.** `WindowInfo` gains `menu` (additive, `#[serde(default,
   skip_serializing_if = "Option::is_none")]; amends ADR 0003 and 0007`): `{"kind":
   "dbus_menu", service, path}` or `{"kind": "gmenu", bus_name, menubar_path?,
   app_menu_path?, application_path?, window_path?}`. A change sends `window_changed`.
   `neuectl windows` prints `[menu dbusmenu SERVICE PATH]` or `[menu gmenu BUS
   menubar=… app-menu=… app=… win=…]`.
7. **`gtk_shell1.capabilities` is 0 for now.** With `global_app_menu | global_menu_bar` GTK
   hides its in-window menu bar, and until the menu bar exists (Phase 3) that would leave
   GTK apps with no menus at all. The constant is in `appmenu.rs`; the Phase 3 menu-bar
   task sets it. Note the XML's enum has values 1, 2 and 3, which are not bit flags:
   `1 | 2` is the value of `desktop_icons`.
8. **Everything else in `gtk_shell1`/`gtk_surface1` is accepted and ignored:**
   `set_startup_id`, `system_bell`, `notify_launch`, `set_modal`, `unset_modal`, `present`,
   `request_focus`, `titlebar_gesture`, `release`. They may get real behaviour in their own
   tasks (`system_bell` and `titlebar_gesture` are the likely ones).

## Consequences
- `neue-comp` gains `neue-protocols` (server) and `wayland-protocols-plasma` (server);
  tests add their client sides. The workspace gains `neue-protocols`.
- The shell (Phase 3) reads the focused window's `menu`, over `neue-shell-v1` and, until
  then, `neuectl`. It must treat the strings as untrusted apart from the syntax check.
- Re-fetch `gtk-shell.xml` when GTK adds a version; our v5 global does not need it.

## Not done
- Checking against real Kate and GNOME Text Editor: manual steps in `docs/checks/phase-1.md`
  §18, run by the owner.
- Addresses for popups and subsurfaces (only toplevels are reported).
- Revoking a stale address when the app's bus name disappears: the shell sees that on D-Bus.
