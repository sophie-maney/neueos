# Global menu verification checklist (task 0.7)

**Status: run on 2026-11-03 by Claude over SSH, with the owner's Plasma session open in the
VM (results in §3).** Skipped or only partly run checks are marked in the tables.

Owner runs this in the Parallels VM under a stock Plasma Wayland session. It verifies the
desk research in `global-menu.md` (R1), so we can settle ADR 0005. Record results in the
tables at the end, then copy the summary table into `global-menu.md`.

Status words: **tested** = you ran it and saw the result; **observed** = seen incidentally
or taken from logs only; **not run** = skipped (say why).

## 0. Prepare the VM

Plasma replaces our compositor for this test only. Do this once.

```sh
# Packages. If a name doesn't exist, `pacman -Ss <word>` and note the real name in the log.
sudo pacman -S --needed plasma-desktop plasma-workspace kwin konsole kate dolphin \
    plasma-integration qt6-wayland xdg-desktop-portal-kde \
    gtk3 gtk4 chromium firefox libreoffice-fresh xterm wayland-utils flatpak
```
`gtk3-demo` and `gtk4-demo` come with the `gtk3` and `gtk4` packages (check with
`pacman -Ql gtk3 | grep demo`). For the Electron test, either use any Electron app you
have or `flatpak install flathub com.visualstudio.code`.

Start Plasma from the VM console (tty1), not over SSH, because it needs a seat:
```sh
# On the VM console, logged in as your user (see dev-setup §3.2 for getty/seatd):
dbus-run-session startplasma-wayland
```
Then open a terminal (Konsole) inside Plasma and use it for everything below.
Add the **Global Menu** widget to the panel: right-click the panel → Add Widgets → "Global
Menu". (If the panel has no room, also try right-click panel → Edit Mode → add "Application
Menu" in the title bar settings.) If Plasma will not start, stop here, save the log
(`journalctl --user -b | tail -n 200`), and report it.

To read results from the Mac instead of the VM window, capture the screen:
`prlctl capture neueos-vm --file <name>.png` (save images under `docs/research/img/`).

### Inspection commands

Run in the VM terminal. Use them for every app below.

```sh
# Which globals does the compositor offer? Expect org_kde_kwin_appmenu_manager.
# (If wayland-info shows no gtk_shell1, note it.)
wayland-info | grep -i -E 'appmenu|gtk_shell|dbus'

# Is a registrar on the session bus, and who owns it?
busctl --user list | grep -i -E 'AppMenu|dbusmenu|kded'
busctl --user introspect com.canonical.AppMenu.Registrar /com/canonical/AppMenu/Registrar

# Which protocol requests does an app send? (replace <app>)
WAYLAND_DEBUG=1 <app> 2>&1 | grep -i -E 'appmenu|gtk_surface1|set_dbus_properties|set_address'

# Is the menu on the bus? Look for names that appear when the app starts.
busctl --user list | grep -i -E 'org.gtk|dbusmenu|appmenu'
```

For each test, answer these four questions and fill in the result table (§3):
1. Did the protocol request reach the compositor (`WAYLAND_DEBUG` line seen)?
2. Did the menu appear in the Plasma Global Menu widget?
3. Does clicking a menu item work (opens a submenu, runs an action)?
4. What did the window itself do (in-window menu hidden or still shown)?

## 1. Baseline

- [ ] **1.1 Registrar present?** Run the `busctl` registrar check above on the fresh
      session, before starting any app. Write down whether `com.canonical.AppMenu.Registrar`
      exists and which process owns it (likely `kded6`). This tells us whether Plasma's
      registrar must be mirrored by ours.
- [ ] **1.2 Globals.** Run the `wayland-info` check. Record whether `org_kde_kwin_appmenu_manager`
      and `gtk_shell1` are both listed, and their versions.

## 2. Per-toolkit checks

Run each app from the Konsole in the Plasma session. Use `WAYLAND_DEBUG` once, then run
normally for the visual check.

### Qt6

- [ ] **2.1 Qt6, KDE theme, registrar present** (default Plasma state).
      `kate` (File/Edit/View… menu bar). Expect the menu in the widget.
- [ ] **2.2 Qt6, KDE theme, registrar absent.** Stop the registrar if you can
      (`qdbus6 org.kde.kded6 /kded org.kde.kded6.unloadModule appmenu`, or
      `busctl --user call org.kde.kded6 /kded org.kde.kded6 unloadModule s appmenu`), then
      restart `kate`. Does the menu still arrive? This answers "registrar needed?" for Qt.
      Reload the module afterwards (`loadModule`).
- [ ] **2.3 Qt6 without the KDE theme.** `QT_QPA_PLATFORMTHEME=generic kate`. Confirm
      nothing is exported (no `set_address`, no menu in the widget).
- [ ] **2.4 Qt6 second app.** `dolphin`. Same expectation as 2.1; guards against a
      Kate-only quirk. Also note menu changes (open a file, toggle a menu item): does the
      widget update? (tests `LayoutUpdated`).
- [ ] **2.5 Qt5 (optional).** Only if you have a Qt5 app with a menu bar. Otherwise mark
      "not run".

### GTK

- [ ] **2.6 GTK3 `GtkApplication` + `set_menubar`.** Try `gtk3-demo` → "Application Class"
      demo (or any GTK3 app you know uses `GtkApplication`; the research suggested Inkscape
      or GIMP, but note in the log whether their menus are `GtkMenuBar` widgets, which are
      expected *not* to export). Is `gtk_surface1.set_dbus_properties` sent without
      `appmenu-gtk-module`? Does `busctl --user list | grep org.gtk` show the app's menu
      path (`org.gtk.Menus` on the app's unique name)?
      Note: Plasma itself does **not** implement GTK Wayland menus (KDE bug 424485). So
      "no menu in the widget" is expected. What we need is whether the *app sends* the
      requests. Record the `WAYLAND_DEBUG` evidence; this also tells us what `gtk_shell1`
      must carry for our compositor.
- [ ] **2.7 GTK3 with `GtkMenuBar`.** One app with an in-window menu bar (GIMP 3/Inkscape,
      or `gedit`/`mousepad` if installed). Confirm nothing is exported.
- [ ] **2.8 GTK4 with `set_menubar`.** `gtk4-demo` (look for a demo using `GtkApplication`
      with a menubar). Same questions as 2.6.
- [ ] **2.9 GTK4 / libadwaita typical app** (for example `gnome-text-editor`, `nautilus`
      or `loupe`). Confirm nothing is exported (menu button only).

### Browsers and Electron

- [ ] **2.10 Chromium, default flags.** `chromium --ozone-platform=wayland`. Check
      `chrome://version` for the version (research says 137+ is required). Is the menu in the
      widget? Check `WAYLAND_DEBUG` for `org_kde_kwin_appmenu`.
- [ ] **2.11 Chromium, registrar absent.** Repeat 2.10 with the registrar unloaded as in
      2.2. Does it still work? Is any flag needed (`chrome://flags` → search "global menu"
      / "appmenu")? Write down the flag name if one exists.
- [ ] **2.12 Firefox, default.** `MOZ_ENABLE_WAYLAND=1 firefox`. Record the version.
      Expect no menu (prefs off by default).
- [ ] **2.13 Firefox, prefs on.** In `about:config` set `widget.gtk.global-menu.enabled` and
      `widget.gtk.global-menu.wayland.enabled` to `true`, restart. Does the menu show? Is the
      bar blank (Mozilla bug 1995226)? Press `Alt` to reveal the in-window bar first, since
      Firefox hides it by default.
- [ ] **2.14 Thunderbird (optional).** Same prefs as 2.13. Mark "not run" if not installed.
- [ ] **2.15 Electron.** VS Code (or any Electron app) with
      `--ozone-platform=wayland`. Research says: in-window menu hidden, nothing shown.
      Record the Electron version (`Help → About`) and whether that holds.

### Other

- [ ] **2.16 LibreOffice.** `libreoffice --writer` with the default VCL plugin
      (`SAL_USE_VCLPLUGIN=kf6` for the Qt path; try both `kf6` and `gtk3`). Record which
      plugins are present, and which exports a menu.
- [ ] **2.17 X11 app via Xwayland.** Plasma runs Xwayland itself; use `xterm` or
      `xeyes` plus any X11 app with a menu (for example `xfig`, or Java Swing if installed:
      then also covers the "Java/Swing: no" row). Expect no menu in the widget. This is
      only a sanity check of the fallback; xwayland-satellite itself is tested in Phase 1.

### Flatpak

- [ ] **2.18 Flatpak Qt/GTK app, default permissions.** Pick one Flatpak Qt app and one
      GTK app (for example `org.kde.kate` and `org.gnome.Loupe` or a GTK3 app).
      Run `flatpak run <id>`. Do menus arrive? (Check the Global Menu widget, and
      `busctl --user list | grep -i dbusmenu`.)
- [ ] **2.19 Flatpak with registrar permission.** Repeat with
      `flatpak run --talk-name=com.canonical.AppMenu.Registrar <id>`. Is anything different?
- [ ] **2.20 Sandbox call-in.** For a Flatpak app that did export, click a menu item in the
      widget. Does the action run? (The shell must be able to call into the sandboxed
      app's well-known/unique name.)

## 3. Results

Run on 2026-11-03, in the Parallels VM (aarch64), Plasma 6.8.2 Wayland on KWin, driven over
SSH against the live session. Screenshots (`prlctl capture`) are in
`docs/research/img/global-menu/`. "Protocol" = what `WAYLAND_DEBUG=1` showed the app send.
"Widget" = the Plasma Global Menu widget in the top panel.

**How this was run, and its limits.** There is no input injection in the VM (no `ydotool` or
`xdotool`, and `sudo` needs a password), so nothing was clicked with the mouse. "Click works"
means a DBusMenu `Event(clicked)` sent to the app's menu object opened the item (About
dialog); clicking in the widget itself was not tested. Alt was not pressed in Firefox.
Every app was started from SSH with the session's environment. Plasma's own bus
(`/tmp/dbus-*`, not `/run/user/1000/bus`) must be used or the registrar is not visible.

| # | Check | Status | Protocol | Widget | Click works? | Notes |
|---|---|---|---|---|---|---|
| 1.1 | Registrar present, owner | tested | n/a | n/a | n/a | `com.canonical.AppMenu.Registrar` is owned by `kded6` (its `appmenu` module). `gmenudbusmenuproxy` also runs (X11 GTK bridge). |
| 1.2 | Globals listed | tested | n/a | n/a | n/a | `org_kde_kwin_appmenu_manager` v2 only. **No `gtk_shell1`** in KWin. |
| 2.1 | Qt6, KDE theme, registrar | tested | `create` + `set_address(:1.41, /MenuBar/1)`, then `release` and a new object `/MenuBar/2` | Yes (Kate, 26.08.2) | Yes, by `Event` on "About Kate" | App serves `com.canonical.dbusmenu` (`GetLayout` works). |
| 2.2 | Qt6, KDE theme, no registrar | tested | none (only binds the manager) | No (empty) | n/a | In-window menu bar shown instead. The registrar is required. |
| 2.3 | Qt6 without the KDE theme | tested, **result differs from R1** | `set_address` sent | Yes | n/a | Still exports with `QT_QPA_PLATFORMTHEME=generic` and with `XDG_CURRENT_DESKTOP` unset. No plasma platform-theme plugin was loaded. `libQt6WaylandClient.so.6.12.0` itself contains `org_kde_kwin_appmenu`, so Qt 6.12's Wayland plugin has a native client. |
| 2.4 | Qt6 Dolphin | tested | `set_address(:1.46, /MenuBar/1)`, objects re-created | Yes | not run | Menu updates (`LayoutUpdated`) not observed: no input to change state. |
| 2.5 | Qt5 | not run | | | | No Qt5 app with a menu bar installed. |
| 2.6 | GTK3 `GtkApplication` menubar | tested | none (KWin offers no `gtk_shell1`, so no `set_dbus_properties` can be sent) | No | `org.gtk.Menus.Start` answered | `gtk3-demo-application` exports `org.gtk.Demo2` on the session bus with `/org/gtk/Demo2/menus/{appmenu,menubar}` without any GTK module. The in-window menu bar stays. |
| 2.7 | GTK3 `GtkMenuBar` | not run separately | | | | Covered in part by 2.6 (the demo's bar is built from a `GMenuModel`). No app with a hand-built `GtkMenuBar` was tried. |
| 2.8 | GTK4 `set_menubar` | not run | | | | `gtk4-demo` is not installed (separate package, needs `sudo`). |
| 2.9 | GTK4 / libadwaita | tested (Loupe 51.rc, Flatpak) | none | No | n/a | Nothing exported, as expected. |
| 2.10 | Chromium default | tested (156.0.8078.79) | `create` + `set_address(:1.64, /com/canonical/menu/1)` | Yes (File Edit View History Tools Profiles Help) | not run | `--ozone-platform=wayland` only; no flag needed. |
| 2.11 | Chromium without registrar | tested | none at start; **`set_address` appeared 11 s later, when the registrar was loaded again** | Yes after reload | n/a | Chromium waits for the registrar and exports when it shows up. Required, but picks up later. |
| 2.12 | Firefox default | tested (159.0) | binds the manager (v2), never calls `create` | No | n/a | |
| 2.13 | Firefox prefs on | tested, did not work | none | No | n/a | `widget.gtk.global-menu.enabled` and `.wayland.enabled` both `true` (in `prefs.js`, strings exist in `libxul`), plus menubar autohide off via `xulstore.json`: still no `create`. Two attempts, so stopped. Alt key and the blank-bar bug (1995226) not reached. |
| 2.14 | Thunderbird | not run | | | | Not installed. |
| 2.15 | Electron (VS Code 1.139.1, Flatpak) | tested | not seen (no Wayland trace inside the sandbox) | No | n/a | Screenshot only. The in-window menu (File Edit Selection View …) **stays visible**, so R1's "hides its menu" did not show. |
| 2.16 | LibreOffice 26.8.1.1 | tested | `kf6`: `create`+`set_address`; `qt6`: `set_address`; `gtk3`: none | `kf6`: Yes; `qt6`: **empty**; `gtk3`: empty | not run | With `gtk3` the in-window menu bar is also gone, so no menu anywhere (first-run dialog was open). `gtk4` plugin not tried; default plugin not recorded. |
| 2.17 | X11 app (GTK3 via Xwayland) | tested, **result differs from R1** | n/a (X11) | **Yes** (Preferences, Help) | not run | Plasma's `gmenudbusmenuproxy` bridges the `_GTK_*` X11 properties. X11-only; nothing found says xwayland-satellite does this. No X11 Java/Swing app tried. |
| 2.18 | Flatpak Kate 26.04.2, default permissions | tested | not seen (see 2.15) | Yes | see 2.20 | The Kate manifest already grants `com.canonical.AppMenu.Registrar=talk`. |
| 2.19 | Flatpak Kate, registrar permission removed (`--no-talk-name=`) | tested | no `/MenuBar` on its bus name | No | n/a | In-window menu shown. The permission is required inside the sandbox. |
| 2.20 | Flatpak call-in | tested | n/a | n/a | Yes | The host called `GetLayout` and `Event` on the app's `xdg-dbus-proxy` name (`:1.156 /MenuBar/2`); the About dialog opened. |

### Versions

| Item | Value |
|---|---|
| Plasma / KWin / Qt | 6.8.2 / 6.8.2 / qt6-base 6.12.0-3, qt6-wayland 6.12.0-1 |
| Kate / Dolphin (host) | 26.08.2 / 26.08.2 |
| Chromium / Firefox / VS Code (Electron) | 156.0.8078.79 / 159.0 / 1.139.1 (Flatpak) |
| GTK3 / GTK4 | 3.24.52 / 4.24.1 |
| LibreOffice | 26.8.1.1 (plugins present: gen, gtk3, gtk4, kf6, qt6) |
| Flatpak apps | Kate 26.04.2 (KDE 6.10 runtime), Loupe 51.rc and gedit 51.0 (GNOME 51), VS Code 1.139.1 |

### Evidence

- 1.1/1.2: `busctl --user list` shows `com.canonical.AppMenu.Registrar` owned by `kded6`;
  `wayland-info` lists only `org_kde_kwin_appmenu_manager` (v2), no `gtk_shell1`.
- 2.1: `org_kde_kwin_appmenu_manager#18.create(...)` then
  `org_kde_kwin_appmenu#44.set_address(":1.41", "/MenuBar/1")`, then `release()` and a second
  object. Images: `2.1-kate.png`, `2.1-kate-click.png`.
- 2.2: `2.2-kate-noregistrar.png` (menu inside the window, widget empty).
- 2.3: `libQt6WaylandClient.so.6.12.0` matches `grep -a org_kde_kwin_appmenu`; Kate's `maps` has no
  platform-theme plugin. Image: `2.3-kate-generic-nodesktop.png`.
- 2.6: `org.gtk.Menus.Start au 1 0` on `/org/gtk/Demo2/menus/menubar` returned the Preferences and
  Help submenus. Image: `2.6-gtk3-application.png`.
- 2.10/2.11: chromium `set_address(":1.64", "/com/canonical/menu/1")`. In the no-registrar run
  the only `set_address` is stamped 18:53:31, when the `appmenu` kded module was loaded again.
  Images: `2.10-chromium.png`, `2.11-chromium-noregistrar.png` (taken after the reload, so it
  shows the menu).
- 2.12/2.13: `2.12-firefox-default.png`, `2.13-firefox-prefs-menubar.png`. The black strip at the
  bottom of the Firefox window is a rendering glitch on virgl, unrelated to menus.
- 2.15–2.20: `2.15-vscode.png`, `2.16-libreoffice-{kf6,qt6,gtk3}.png`, `2.17-x11-gtk3.png`,
  `2.18-flatpak-kate.png`, `2.19-flatpak-kate-no-registrar-perm.png`,
  `2.20-flatpak-kate-click.png`.

### Open questions found

1. **Firefox 159:** why no `create` with both prefs on? Try the Alt key and `MOZ_LOG`, ideally
   on a real session where input works, or check whether the build needs another pref.
2. **LibreOffice `qt6` plugin:** `set_address` is sent but the widget stayed empty. Retest
   without the first-run dialog.
3. **GTK on Wayland:** KWin has no `gtk_shell1`, so whether GTK sends `set_dbus_properties`
   (and with what values) cannot be observed here. It needs our compositor (Phase 1) or a
   compositor that implements `gtk_shell1`. What *is* confirmed: GTK3 exports `org.gtk.Menus`
   and `org.gtk.Actions` on the session bus without any module.
4. **Qt6:** the Wayland client is built into `qt6-wayland` 6.12, so `plasma-integration` is not
   needed. Retest with an older Qt (6.8 LTS?) if we must support it.
5. **Menu updates and widget-side clicks** (`LayoutUpdated`, `AboutToShow`) were not observed;
   no input injection in the VM.
6. **xwayland-satellite** vs the X11 route (2.17): untested, because it is not installed. Does
   it forward `_GTK_*` or `_KDE_NET_WM_APPMENU_*` properties?
7. GTK4 `set_menubar` (2.8), Qt5 (2.5), Thunderbird (2.14), LibreOffice `gtk4`: not run.

### Things left on the VM

Flatpak apps installed for the user (about 5.4 GB under `~/.local/share/flatpak`),
`~/vsc-data`, and throw-away profiles under `/tmp`. Remove with
`flatpak --user uninstall --all` if you want the space.

## 4. After you finish

Done for 0.7 (2026-11-03): the coverage summary is in `global-menu.md` and the plan box is
ticked. Remaining owner-only items are the open questions above that need real input.

## What the results feed into

| Question | Answer from the VM | Decides |
|---|---|---|
| Registrar needed for Qt/Chromium? (2.2, 2.11) | Yes for both. Chromium exports when it appears later; Qt did not export when started without it | neue-comp/neue-shell must own `com.canonical.AppMenu.Registrar` in Phase 1 |
| GTK sends `set_dbus_properties` without a module? (2.6, 2.8) | Cannot tell on KWin (no `gtk_shell1`). The `org.gtk.Menus` export works without a module | Still needs `gtk_shell1` + a GMenuModel client; verify on our compositor |
| Chromium flag / Firefox prefs (2.10–2.13) | Chromium: no flag. Firefox: prefs did not work in 159 | Ship Chromium defaults as they are; Firefox: fallback until retested |
| Electron (2.15) | Nothing exported; the in-window menu stayed visible | Fallback only; wait for the upstream fix |
| Flatpak (2.18–2.20) | Needs `--talk-name=com.canonical.AppMenu.Registrar` (Kate ships it); host can call into the sandbox | Document the override; check which Flatpak apps lack it |
