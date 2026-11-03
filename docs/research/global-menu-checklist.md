# Global menu verification checklist (task 0.7)

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
# On the VM console, logged in as your user (see dev-setup §2.3 for getty/seatd):
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

Fill in. One row per check; leave the notes short and paste key log lines (3–5 lines
max) under "Evidence" below the table.

| # | Check | Status (tested / observed / not run) | Protocol request seen? | Menu in widget? | Click works? | Notes |
|---|---|---|---|---|---|---|
| 1.1 | Registrar present, owner | | n/a | n/a | n/a | |
| 1.2 | Globals listed | | n/a | n/a | n/a | |
| 2.1 | Qt6 KDE theme, registrar | | | | | |
| 2.2 | Qt6 KDE theme, no registrar | | | | | |
| 2.3 | Qt6 no KDE theme | | | | | |
| 2.4 | Qt6 Dolphin, updates | | | | | |
| 2.5 | Qt5 (optional) | | | | | |
| 2.6 | GTK3 GtkApplication menubar | | | | | |
| 2.7 | GTK3 GtkMenuBar | | | | | |
| 2.8 | GTK4 set_menubar | | | | | |
| 2.9 | GTK4 / libadwaita | | | | | |
| 2.10 | Chromium default | | | | | |
| 2.11 | Chromium no registrar | | | | | |
| 2.12 | Firefox default | | | | | |
| 2.13 | Firefox prefs on | | | | | |
| 2.14 | Thunderbird (optional) | | | | | |
| 2.15 | Electron | | | | | |
| 2.16 | LibreOffice (kf6 / gtk3) | | | | | |
| 2.17 | X11 app | | | | | |
| 2.18 | Flatpak default | | | | | |
| 2.19 | Flatpak + registrar permission | | | | | |
| 2.20 | Flatpak sandbox call-in | | | | | |

### Versions

| Item | Value |
|---|---|
| Plasma / KWin / Qt | |
| Chromium / Firefox / Electron | |
| GTK3 / GTK4 | |
| LibreOffice | |

### Evidence

(Paste short log lines or screenshot paths here, one heading per check number.)

### Open questions found

(Anything surprising, or anything you could not test and why.)

## 4. After you finish

1. Copy a short coverage table (toolkit × tested/observed status × works?) into
   `global-menu.md`, replacing the "Manual VM checklist" section with a link to this file.
2. Tick task 0.7 in `PLAN.md`.
3. Where a result contradicts the "Coverage" table in `global-menu.md`, fix that row.
   ADR 0005 (task 0.8) is written from the corrected table.

## What the results feed into

| Question | Decides |
|---|---|
| Registrar needed for Qt/Chromium? (2.2, 2.11) | Whether neue-comp/neue-shell must own `com.canonical.AppMenu.Registrar` in Phase 1 |
| GTK sends `set_dbus_properties` without a module? (2.6, 2.8) | Whether `gtk_shell1` + a GMenuModel client is enough |
| Chromium flag / Firefox prefs (2.10–2.13) | What the distro must ship as defaults |
| Electron (2.15) | Fallback menu only, or wait for the upstream fix |
| Flatpak (2.18–2.20) | Needed `--talk-name` overrides and sandbox access |
