# Phase 3: Shell MVP and session

Task list for Phase 3 of the roadmap in `PLAN.md` §8. Architecture and decisions
live in `PLAN.md` and `docs/adr/`.

One task = one thread = one PR, as before. Order: the foundations 3.1–3.6 first (3.1 before
3.2a–c; 3.3, 3.4 and 3.6 can run alongside them). Then the menu bar (3.7–3.15), the dock
(3.16–3.17), the session (3.18–3.20) and the portal and Preferences (3.21–3.26) mostly run
in parallel, each after the foundations it names. 3.27–3.30 close the phase.

## How checks work in this phase
- **No task waits for the owner.** Every task verifies itself: unit, golden and headless
  compositor tests, plus the VM, which the agent drives itself over ssh with `neuectl`,
  `ydotool` and `neuectl screenshot` (`docs/vm-env.sh`, `docs/dev-setup.md`).
- **What only the owner can check goes on the task's *Owner:* line**: the real PC (install,
  NVIDIA, performance), sound you have to hear, Bluetooth and Wi-Fi hardware, VoiceOver,
  and whether something looks right. Write each as a concrete check with what to expect;
  3.29 turns them into runbook steps.
- **3.29 writes `docs/checks/phase-3-owner.md`** in the format of
  `docs/checks/phase-2-owner.md` (Run/Do/Expected, STOP AND REPORT, Note, one report at
  the end). It also carries Phase 2's open owner steps (2.26, the exit check).
- **The owner runs it in one sitting** and reports every failed step in one message. Each
  fix becomes a new task from **3.31** onward, with the step numbers it answers and which
  steps to re-run. 3.30 closes the phase after the fixes.

Shell widgets follow the widget rules in `AGENTS.md` (gallery page, goldens, AccessKit,
keyboard). The `neue-shell` process is Linux-only; its widgets live in portable modules so
their goldens and the gallery's "Shell" page run on macOS.

## A. Foundations
- [x] **3.1 `neue-shell-v1` protocol** · Opus. The private protocol between the
      compositor and the shell (`crates/neue-protocols/`, `docs/protocols/`), with an
      ADR. Events: the focused toplevel, and per window its app id, title, minimised and
      attention state, `xdg-toplevel-icon` and the global-menu address (ADR 0005, 0025),
      matched to `ext-foreign-toplevel-list` and `neuectl` ids. Requests: activate,
      minimise, unminimise, close, a minimise-target rectangle per window (for Phase 4's
      genie), send a key chord to the focused surface (fallback Edit menu, ADR 0005 §4),
      run a compositor action, and get media, volume and brightness keys routed to the
      shell. The ADR also decides who may bind it (only the shell; never sandboxed
      clients) and how the shell is started and restarted after a crash (PLAN.md §3.3,
      §9: the compositor spawning it on a private socket, or a systemd unit; ADR 0027's
      "revisit").
      *Accept:* headless tests with a scripted client cover every event and request,
      including a refused bind from a second client; `neuectl` unchanged or extended
      additively.
      *Done:* ADR 0066, `docs/protocols/neue-shell-v1.md`. The compositor spawns the shell
      itself (`neue-comp --shell <cmd>`, private socket in `WAYLAND_SOCKET`) and restarts it
      (at once after 10 s of life, else 250 ms doubling to 30 s); only that client sees the
      global. `neuectl` unchanged. 3.2b adds `--shell /usr/bin/neue-shell` to
      `neue-comp.service`.
- [x] **3.2 `neue-shell` skeleton, menu bar and desktop background**, split into 3.2a–c
      (the platform had no per-output layers and no way for the shell to use 3.1's
      protocol on its one connection).
- [x] **3.2a Shell surfaces in platform and kit** · Opus. `neue-platform` tells the app
      which outputs exist (added and removed, with name, logical size and scale) and
      `LayerSpec` takes an output. The Wayland driver binds `neue_shell_manager_v1` when it
      is offered and passes its window list (identifier, title, app id, state, closed) and
      the active window up through `App`, and activate, minimise, unminimise, close,
      `run_action` and `send_shortcut` down through `Ctx`. `neue-kit`: `Window::layer(..)`,
      and output and shell events reaching `Application`. ADR. Icons, menu addresses,
      system keys and minimise rectangles wait for the tasks that use them (3.7, 3.14,
      3.16).
      *Accept:* headless `neue-comp` tests run the real driver on a socket pair: a layer
      surface lands on the second output, and window and active-window events arrive and
      requests take effect; kit unit tests with the fake host.
      *Done:* ADR 0067. `neue_platform::desktop` (re-exported as `neue_kit::desktop`);
      `Ctx::keep_alive` / `Cx::keep_alive` keeps the shell running with no surface open.
      Tests: `neue-comp/tests/platform_shell.rs`, `neue-kit/src/tests/desktop.rs`.
- [x] **3.2b `neue-shell` skeleton and menu bar** · Opus. After 3.2a. New crate
      `shell/neue-shell`: one process, several layer surfaces, shared state (window list
      from 3.1, settings, later the app registry); widgets in portable modules, the binary
      Linux-only. A menu bar on every output (top layer, exclusive zone, height from
      `neue-theme`), translucent gradient material with the opaque fallback until 3.5; the
      logo menu (About, Preferences…, Lock Screen, Log Out, Restart, Shut Down; logind over
      zbus), the focused app's name in bold, and the clock extra. Starting and restarting
      as decided in 3.1: `--shell /usr/bin/neue-shell` in `neue-comp.service`. Gallery
      "Shell" page with the bar's pieces.
      *Accept:* goldens of the bar (light/dark × scale 1 and 2); in the VM the name
      follows focus, the clock ticks, Log Out ends the session, `pkill neue-shell` brings
      the shell back within a second, all checked by the agent with screenshots.
      *Owner:* the bar on the PC at its native scale and on a second display: one bar
      per display, crisp text and logo, windows start below it. Restart and Shut Down
      from the logo menu do what they say (not run in the VM).
      *Done:* ADR 0068. `neue_kit::desktop::{MenuBar, MenuTitle}`, DejaVu Sans Bold
      (`Options::extra_fonts`), `neue_dbus::login`, goldens `shell-bar-*` in the gallery
      (Shell page). Log Out is the compositor's `quit` action. The `neue-comp` PKGBUILD
      gained a `neue-shell` package. VM (arm64): name follows focus and survives a shell
      restart, clock ticks, `pkill neue-shell` → bar back within 1 s, About opens, Log Out
      ends the compositor; Lock (no lock screen until 3.18), Restart and Shut Down not run.
- [x] **3.2c Desktop background** · Sonnet. After 3.2b. A `background` layer surface per
      output with a procedural default picture (light and dark) and an optional image
      from `neue-config`. The image decoder needs an ADR and a licence check (shared with
      3.3's `png`/`resvg`).
      *Accept:* goldens of the default picture; a VM screenshot with and without a
      configured image.
      *Done:* ADR 0069. `neue_kit::desktop::Desktop`, `neue_paint::materials::desktop`
      (default picture, `Picture`, `Fit`), `neue_config::ShellConfig` (`shell.toml`,
      `[desktop] picture` and `fit`), `neue_shell::picture` (PNG and JPEG). Goldens
      `shell-desktop*` (default, fill, fit). The config is read at start; live reload comes
      with Preferences (3.24). VM (arm64): default picture behind the bar, a JPEG with `fit`
      (black bars) and a PNG with `fill`, a missing file warns and falls back to the
      built-in picture, all by screenshot after `pkill neue-shell`.
      *Owner:* the background on the PC at native scale and on a second display; a real
      photo (JPEG) looks sharp, and every `fit` value does what it says.
- [x] **3.3 `neue-desktop` v0** · Sonnet. New portable crate: desktop entries (parsing,
      localisation, `Actions` including `new-window`, `NoDisplay`, `OnlyShowIn`), matching
      an app id to its entry (`StartupWMClass`, Flatpak ids, the file name), icon theme
      lookup (index.theme, sizes and scales, hicolor, Papirus as fallback), loading SVG
      and PNG icons into the kit `Icon` type (ADR 0051; check `resvg` and `png` licences),
      launching an entry with an `XDG_ACTIVATION_TOKEN`, and Trash state (empty or full,
      empty it; freedesktop trash spec).
      *Accept:* unit tests against fixture directories, on macOS and Linux.
      *Done:* ADR 0070. `neue-desktop` (`entry`, `apps`, `icon_theme`, `icon_load`, `launch`,
      `trash`); `neue_paint::icon::Icon::raster` (a `Picture` drawn through the texture cache)
      because theme SVGs are not filled paths, and `Icon::has_own_colors`. 28 tests on
      fixture directories (macOS and the OrbStack arm64 machine). `resvg` 0.47 with default features off. Not run in the VM
      (nothing consumes the crate until 3.16). Not in v0: other volumes' trash (needs the
      mount table, 3.16), D-Bus activation, MIME, thumbnails, recent files, autostart.
- [x] **3.4 Backdrop blur: compositor** · Opus. ADR choosing the protocol:
      `ext-background-effect-v1` if our pinned wayland-protocols and Smithay have it,
      else a private one (ADR 0002 §7). A dual-Kawase blur pass in the GLES renderer for
      toplevels, popups and layer surfaces with a blur region: damage widened by the blur
      radius, the blurred background cached while nothing behind changes, off for
      direct-scanout and full-screen windows.
      *Accept:* a headless test renders a client with a blur region over a pattern and
      checks the pixels behind it; a VM screenshot; no extra frames when idle (ADR 0019).
      *Owner:* on the PC (AMD/Intel and NVIDIA), dragging a blurred window (`blur-demo`, below) over a busy
      window, and a window under it, stays smooth at the display's refresh rate.
      *Done:* ADR 0071. `ext-background-effect-v1` (in our wayland-protocols and Smithay
      pins); `blur::BlurElement`, a Smithay framebuffer effect below each surface with a
      blur region (windows, subsurfaces, popups, layer surfaces): the damage tracker
      re-captures when damage behind reaches it, the blur reads 32 logical px around the
      region, the result is cached per tracker. Dual Kawase, 3 passes (4 above scale 1.5),
      as Smithay custom texture shaders. Off for full-screen windows and locked sessions.
      7 headless tests (`tests/blur.rs`) and 5 unit tests; VM: screenshot of `blur-demo`
      over foot on the tty backend, 0 CPU ticks in 10 s idle with it open.
      `cargo run -p neue-comp --example blur-demo [W H]` makes a blurred window. Not done:
      real hardware.
- [x] **3.5 Backdrop blur: clients** · Sonnet. After 3.4. `neue-platform`'s Wayland
      driver asks for blur on surfaces whose root material is translucent (menus,
      popovers, HUD panels, the menu bar), and translucent materials (ADR 0039) switch
      from the opaque tinted fallback to translucent when the compositor offers blur. The
      winit driver keeps the fallback.
      *Accept:* existing goldens unchanged; new goldens of the translucent variants; a VM
      screenshot of a gallery menu over a coloured window.
      *Owner:* the menu bar and a menu over a busy window look right, light and dark.
      *Done:* ADR 0072. Scope cut with the owner: menus, popovers and sheets have no
      translucent variant yet, so only the menu bar and HUD panels go translucent (the
      popups stay opaque; follow-up 3.5b below). `neue-platform`: `Ctx::has_blur`,
      `WindowSpec::blur` (`Blur { corner_radius }`), `wayland/blur.rs` (manager,
      capabilities, rounded region). `neue-kit`: `Window::blur_behind`, `Cx::has_blur`,
      `Cx::backdrop`; HUD chrome paints the translucent tint. The shell's bar asks for
      blur. Tests: 4 region unit tests, 3 kit tests (HUD, bar), 1 headless integration
      test (`tests/platform.rs`: the compositor receives a rounded region). Existing
      goldens unchanged. VM: log shows blur offered and the bar's layer surface opened
      blurred, screenshot of the bar over the wallpaper. Not done: a HUD in the VM
      (the click helper didn't hit the gallery's Show HUD button), a coloured window
      behind the bar, real hardware.
      *Owner:* the menu bar over the wallpaper and the gallery's Popovers page HUD
      (`Show HUD`) over a busy window: translucent with a blur, light and dark, square
      bar and rounded HUD corners, nothing blurred under Increased contrast.
- [x] **3.5b Translucent menus and popovers** · Sonnet. After 3.5. A `Backdrop`
      parameter and translucent palettes for `menu_panel`, `popover` and `sheet`
      (neue-theme, neue-paint, goldens for both variants, light and dark, 1x and 2x);
      xdg-popups for menus and popovers ask for blur with their corner radius (the arrow
      of a popover stays out of the region); the gallery's Menus page shows it over a
      coloured window.
      *Accept:* new goldens, existing goldens unchanged; a VM screenshot of a gallery
      menu over a coloured window.
      *Owner:* a gallery context menu (Menus page) and a popover (Popovers page) over a
      coloured window: translucent with a blur, light and dark, rounded corners, the
      popover's arrow sharp and not blurred (also when the compositor flips it), nothing
      blurred under Increased contrast.
      *Done:* ADR 0073. `neue-theme`/`neue-paint`: `menu`, `popover` and `sheet` palettes
      and `menu_panel`, `popover`, `sheet` take a `Backdrop` (opaque is the old colour);
      `MaterialTile` gains `Menu`, `Popover`, `Sheet`. `neue-platform`: `PopupBlur`
      property, `Placement::blur_box`, popup effect objects with a region set on open and
      on every configure. `neue-kit`: `BackdropProp` default property (set from
      `Cx::has_blur`), menus and popovers carry `PopupBlur`. The gallery's Materials page
      shows the three tiles. Scope cut: sheets get the palette and goldens only; the kit's
      in-window sheet stays opaque (nothing blurs behind it). Tests: 36 neue-paint and 24
      gallery Specimen goldens (new; existing unchanged), 2 `blur_box` unit tests, 2 kit
      tests (menu and popover ask for blur and paint translucent), palette tests.
      `cargo xtask check` passes on macOS and in the OrbStack machine. Not done: the VM
      screenshot. Clicks and keys did not reach the gallery in the VM (also with an
      unmodified `main` build, as in 3.5), so no menu could be opened there; and no
      headless compositor test opens a popup, so the popup effect object reaching the
      compositor is untested end to end. Both are on the *Owner:* line.
- [x] **3.6 `neue-settingsd` v0** · Opus. The settings service (PLAN.md §3.4) on
      tokio + zbus, ADR for the interface: `org.neueos.Settings1` with typed keys from the
      `neue-config` schema (get, set, a changed signal), writing the compositor config
      (hot-reloaded already), GTK `settings.ini` (colour scheme, fonts, cursor) and dconf
      keys through `ca.desrt.dconf.Writer` over D-Bus. A mock backend so Preferences runs
      on macOS. New keys keep defaults (AGENTS.md).
      *Accept:* unit tests on macOS; in the VM, setting dark mode through `busctl` changes
      the compositor config, `settings.ini` and `gsettings get
      org.gnome.desktop.interface color-scheme`.
      *Done (ADR 0074):* dark mode lives in the new `appearance.toml` (read by everyone),
      not `compositor.toml`; compositor keys are set the same way and hot-reload. Keys
      are the schema's own dotted paths; `Changed(a{sv}, as)` also follows hand edits. The
      mock is `Store::in_memory()`; the D-Bus client for apps comes with 3.24. Not yet:
      the session doesn't start the service (3.27). *Follow-up (ADR 0075):* the
      compositor's title bars, the shell and the cursor now read and follow
      `appearance.toml`.

## B. Menu bar
- [x] **3.7a Global menu A: registrar and DBusMenu** · Opus. After 3.2b. Split from 3.7
      on 2026-11-05. In `neue-dbus` (ADR 0005 §1): `com.canonical.AppMenu.Registrar`
      owned by the shell, and a DBusMenu client (layout, properties, `AboutToShow`,
      events, toggles, shortcuts, icons, updates). The Wayland driver passes each window's
      menu address (`menu_dbusmenu`/`menu_gmenu`/`menu_none`) to the shell. The menu bar
      shows the active window's menus as kit menus opened from the layer surface; their
      items act. Navigation inside an open menu is the kit's as before.
      *Accept:* tests against neue-kit's own DBusMenu export (`neue-dbus` `testing`); in
      the VM the gallery's and a Qt6 app's menus show and their items act.
      *Owner:* Chromium and LibreOffice menus on the PC; in the VM, choosing an item from
      the gallery's Pages menu and KWrite's File ▸ New in the bar acts.
      *Done (ADR 0076):* in the VM the gallery's and KWrite's (Qt6) menus show, KWrite
      hides its own menu bar, the bar follows the active window, and a killed shell
      brings the registrar and the menus back. Items acting is covered by kit and
      `neue-dbus` tests; not clicked in the VM, because ydotool input didn't reach the
      session in that run (not even Tab in the gallery), so it is on the *Owner:* line.
      Icons are parsed but not drawn; Super shows as the kit's command key until 3.10c.
- [x] **3.7b Global menu A: menu-bar navigation** · Opus. After 3.7a. Hover-switching
      between open menus, Left/Right across the bar's menus, Escape, and a shortcut to
      focus the menu bar: a `neue_shell_v1` event (`focus_menu_bar`, version bump) sent by
      a compositor action, bound to Ctrl+F2 by default; the shell opens the first menu
      from the keyboard. ADR for the protocol change.
      *Accept:* kit tests for switching and Left/Right; a headless compositor test for the
      action reaching a scripted shell; in the VM, Ctrl+F2 then arrows walk the gallery's
      menus.
      *Done (ADR 0077):* in the VM, Ctrl+F2 opens the logo menu, Right/Left walk to View
      and Pages and back (skipping the app-name title, which opens nothing until 3.9),
      Escape gives the gallery the keyboard back, pointing at Pages with View open
      switches, and choosing Pages ▸ Values acts (the 3.7a *Owner:* item for the
      gallery). Two fixes it needed: neue-comp now grabs for popups of layer surfaces and
      hands the keyboard back when the bar's last menu closes; and neue-platform adopts
      seats before its first roundtrip (since 3.5, kit apps on a compositor with blur had
      no keyboard or pointer, which is why 3.7a's VM input failed). Known gap: the app's
      window draws inactive while a bar menu has the keyboard.
      *Owner:* on the PC, Ctrl+F2 and the arrows over a Qt app's menus (KWrite).
- [x] **3.8 Global menu B: GMenuModel** · Sonnet. After 3.7a. An `org.gtk.Menus` +
      `org.gtk.Actions` client in `neue-dbus`, the same menu rendering, and the
      `gtk_shell1` capabilities in `appmenu.rs` switched on (ADR 0025 §7: the enum values
      are not bit flags).
      *Accept:* in the VM, `gtk3-demo` and `gtk4-demo` (`GtkApplication`) show their menus
      in the bar and hide their in-window menu bar; a GTK app with no exported menus keeps
      working.
      *Done (ADR 0078):* in the VM, `gtk3-demo-application` and `gtk4-demo-application`
      (the "Application Class" demos; `gtk3-demo` itself exports no menus, and keeps working)
      show their menus in the bar, hide their in-window menu bar, and Help ▸ About and
      Preferences ▸ Prefer Dark Theme act. GTK 3's app menu is the menu under the app's name.
      Capabilities are 3: GTK tests bit `value - 1`, so the enum values *are* used as bits
      after all (ADR 0025 §7 amended). Icons and section headers are not shown.
      *Owner:* Nautilus/gedit-style GNOME apps on the PC; a Flatpak GTK app (its bus name is
      the sandbox's own).
- [x] **3.9 Fallback menu and menu search** · Sonnet. After 3.7b and 3.10c. ADR 0005 §4:
      App, File, Edit, Window and Help for windows with no menu address. Edit items send
      key chords through `neue-shell-v1`; Window items run compositor actions; New Window
      uses the entry's `new-window` action (3.3) or is disabled. Help's search field finds
      items in the current menus, whatever their source.
      *Accept:* in the VM, Copy and Paste from the fallback menu work in foot and Firefox;
      menu search finds and runs an item in the gallery.
      *Done (ADR 0082):* `neue_shell::fallback` (App, File, Edit, Window from one
      description), `neue_shell::search` and `help` (Help ▸ Search Menus… opens a popover
      with a search field and up to 8 hits; an app's own Help menu gets the item at the
      top). Two fixes it needed: `Popover::focus` in the kit, and `send_shortcut` in
      neue-comp ending a popup's keyboard grab (the chord went to the closing menu). In
      the VM: foot's Edit ▸ Paste (Ctrl+Shift+V; Cut, Undo and Select All disabled),
      Firefox's Edit ▸ Copy (menus show ⌘), Window ▸ Zoom, File ▸ New Window disabled for
      foot (no `new-window` action), and in the gallery Help ▸ Search Menus…, "valu",
      Return switches to Pages ▸ Values. Not run in the VM: Hide, Quit, Bring All to
      Front, New Window of an app that has the action (unit tests cover the requests),
      Copy in foot (scripted pointer selection didn't work there). About is disabled.
      *Owner:* on the PC, Edit ▸ Copy/Paste in a libadwaita app (Text Editor) and an
      Electron app; File ▸ New Window in Nautilus or a browser; search with two outputs
      (the panel opens under the first bar's Help).
- **3.10 Super as ⌘**, split into three PRs (2026-11-06: about 30 files across six crates
  and a protocol change were too many for one). PLAN.md Q1, decided yes on 2026-11-05.
- [x] **3.10a Super as ⌘ A: compositor remap** · Opus. The compositor remaps Super+key
      chords to Ctrl+key for third-party apps by rewriting the modifiers it sends them, except
      an exceptions list in `compositor.toml` (terminals by default) and clients that bound
      `neue_window_manager_v1` or `neue_shell_v1` (neue-kit apps, which take Super
      themselves). No remap while a client inhibits shortcuts. ADR.
      *Accept:* headless tests: Super+C reaches a scripted client as Ctrl+C, an exempt app
      gets Super+C, a kit client gets Super+C, compositor bindings still win; in the VM,
      Super+C/Super+V copy and paste in Firefox, and foot gets Ctrl-Shift-C semantics
      unchanged.
      *Done (ADR 0079):* `KeyFocus` in `command_key.rs` rewrites Super to Control in the
      modifiers sent to remapped clients; settings in `[super-as-command]` (`enabled`,
      `exceptions`). In the VM, Firefox: Super+L focuses the address bar, Super+A and
      Super+C copy (read back with `wl-paste`), Super+V pastes; foot: Super+C does not
      interrupt `sleep`, Ctrl+C does, Ctrl+Shift+V pastes. The gallery's Super+C/V moved to
      3.10b (the kit still takes Ctrl as its command key). Known gap: the default Super+Q
      binding quits the session, so ⌘Q never reaches apps.
      *Owner:* on the PC, Super+C/V in a GTK app (gedit) and a Qt app (KWrite); Super+C in
      an X11 app through xwayland-satellite.
- [x] **3.10b Super as ⌘ B: the kit's command key** · Sonnet. After 3.10a. neue-kit's
      command key is Super (META) on Linux as on macOS: text fields, text views, tables,
      Super+V paste in neue-platform's clipboard driver; kit menus draw ⌘. ADR 0042 amended.
      *Accept:* kit tests with META as the command key on every host; in the VM, Super+C and
      Super+V copy and paste in the gallery, and its menus show ⌘.
      *Done (ADR 0080):* META is the command key on every host in fields, text views,
      tables and collections; the Wayland driver pastes on Super+V (Ctrl+V no longer does);
      `Shortcut::command` always draws ⌘ (the per-thread Ctrl override is gone, so the
      menu goldens changed). ⌘←/→/⌫ also work on Linux; Ctrl stays the word key and
      Ctrl+Home/End. In the VM, gallery: Super+A, Super+C copy (read back with `wl-paste`),
      Super+V pastes into another field; Ctrl+A and Ctrl+C do nothing; the context menu
      shows ⌘X ⌘C ⌘V.
      *Owner:* none beyond 3.10a's: Super+C/V in a kit app on the PC.
- [x] **3.10c Super as ⌘ C: ⌘ in third-party menus** · Sonnet. After 3.10a. A
      `neue_shell_toplevel_v1` event (manager version 3) telling the shell whether Super
      reaches the window's app as Ctrl; neue-platform passes it on in `Toplevel`, and the
      shell's global menu shows those apps' Control shortcuts with ⌘. ADR for the protocol
      change.
      *Accept:* a headless test of the event (exempt and remapped apps); shell unit tests of
      the mapping; in the VM, Firefox's menus show ⌘C and foot's do not.
      *Done (ADR 0081):* `neue_shell_toplevel_v1.command_key(super | ctrl)`, since 3, in a
      toplevel's first batch and again when it changes (new app id, config reload); both
      the manager and the toplevel interface are version 3 (a toplevel takes its manager's
      version). `Toplevel::super_as_ctrl` in neue-platform; the shell shows Control
      shortcuts as ⌘ for such windows (Super beside it counts once) and rebuilds the bar's
      menus when the flag changes. Shortcut inhibitors are not tracked. Headless tests:
      remapped, exempt and kit apps, later windows, app-id and config changes, and a
      version-2 shell getting no event. In the VM, KWrite (Qt6, DBusMenu): File shows
      ⌘N ⇧⌘N ⌘O ⌘L ⌘W ⇧⌘W ⌘Q; after adding it to `exceptions` and reloading, ⌃N ⌃⇧N and the
      rest. Firefox and foot export no menu on Wayland (`menu: none`), so the plan's
      Firefox/foot check isn't possible; KWrite stands in for the remapped app.
      *Owner:* GTK apps' menus (gedit, Nautilus) show ⌘ on the PC; Chromium's too.
- [x] **3.11a Battery and Bluetooth A: UPower and BlueZ clients** · Sonnet. After 3.2b.
      Split from 3.11. `neue-dbus` modules `power` and `bluetooth`: zbus proxies for UPower
      (`DisplayDevice`) and BlueZ (`ObjectManager`, not `bluer`, which links libdbus) on the
      system bus, and plain models with the logic (`neue-dbus` is Linux-only, so these are not
      portable; 3.11b's builders take their own portable view types, as `BarContent` is):
      `Battery` (percentage, state, time left; `None` when absent), `Adapter` and `Device`
      (powered, connected, name). Calls: adapter
      power, device connect and disconnect. Change signals as a stream of snapshots the
      shell worker (3.11c) can wait on.
      *Accept:* unit tests against mock UPower and BlueZ services over `testing::pair()`: no
      battery, charging, discharging, time left, no adapter, power on and off, connect and
      disconnect, a device appearing and disappearing.
      *Done:* `neue_dbus::power::{Power, Battery, BatteryState, PowerChanges}` (UPower's
      `DisplayDevice`, `IsPresent` false → `None`; time left is until empty when
      discharging, until full when charging, `None` without an estimate) and
      `neue_dbus::bluetooth::{Bluetooth, State, Adapter, Device, BluetoothChanges}` (first
      adapter by path; only paired devices of it, connected first; `set_powered`, `connect`
      (blocks: call it off the UI thread) and `disconnect`). `*Changes` are blocking
      iterators of "read again" signals, like `MenuSignals`. 23 neue-dbus unit tests
      (arm64 Linux via `cargo xtask linux`) against mock services over `testing::pair()`.
      No ADR: zbus only, no new dependency or protocol. Pairing a new device needs a BlueZ
      agent and is not planned yet (paired devices only).
- [x] **3.11b Battery and Bluetooth B: icon extras in the kit** · Sonnet. After 3.11a's
      models (can start from their shapes). `MenuTitle::icon` and `Handle::set_hidden`;
      original vector battery (charge level, charging) and Bluetooth icons, light and dark;
      `bar::battery_menu` and `bluetooth_menu` builders over the 3.11a models. Gallery Shell
      page entries, AccessKit labels.
      *Accept:* goldens (states × light/dark × scale 1 and 2); kit tests for icon and hidden
      titles.
      *Done (ADR with 3.11c):* `neue_kit::desktop::ExtraIcon` (`Battery { level, charging }`,
      `Bluetooth { powered, connected }`), `MenuTitle::icon(icon, name, menu)` and `.hidden`,
      `Handle::set_icon` and `set_hidden` (a hidden title takes no room, paints nothing, takes
      no focus and opens nothing; hiding it closes its menu). Icons are drawn by
      `neue_paint::materials::menu_bar::{battery, bluetooth}` (+ `_label`) with metrics and a
      bolt halo colour in `neue_theme`; an open title draws them in the highlighted colour.
      `neue_shell::bar`: portable `BatteryView`, `BluetoothView`, `DeviceView`,
      `BluetoothCommand`; `battery_icon`/`battery_menu`, `bluetooth_icon`/`bluetooth_menu`
      (switch, then a submenu per paired device, a tick when connected);
      `BarContent::{battery, bluetooth}` (`None` hides the extra), `BarHandles::{battery,
      bluetooth}`, and `menu_bar` takes a second closure for `BluetoothCommand`. Extras
      order, right to left: clock, battery, Bluetooth. The shell passes `None` for both and
      only logs Bluetooth commands until 3.11c. Goldens: `shell-bar` gained battery-open and
      bluetooth-open states (the old ones changed: the extras are in the bar) and
      `shell-extras` shows batteries and Bluetooth icons in each state; 3 kit tests.
- [x] **3.11c Battery and Bluetooth C: shell wiring** · Sonnet. After 3.11a and 3.11b. A
      worker thread (as the global menu's) following UPower and BlueZ, bar sync, extras
      absent without a battery or an adapter, menu actions (power toggle, connect,
      disconnect). ADR for the extras.
      *Accept:* in the VM with a mock UPower and BlueZ on the system bus: both extras show
      with screenshots, the Bluetooth menu toggles and connects (checked on the mock), and
      neither shows with the mocks stopped.
      *Done (ADR 0083):* `neue-shell`'s `extras.rs` (Linux-only): a worker thread per service
      on the system bus, each sending a snapshot (`Option<BatteryView>`, `Option<State>`) when
      it changes: it reads at start, on each `PowerChanges`/`BluetoothChanges` signal and
      every 5 s (signals don't cover a service starting or stopping). A failed read counts
      as `None`, which hides the extra. The shell keeps BlueZ's `State` to turn the menu's
      device indices into object paths; `set_powered`, `connect` and `disconnect` run
      through `cx.spawn` and only log their result (the state comes back by signal).
      `neue-dbus` example `mock_power_bluetooth` serves the mocks (`docs/dev-setup.md`
      §1.6b). Unit tests: view mapping and the follow loop (4, on Linux). *VM:* with the
      mock both extras appeared with their menus (64 %, 3:12 remaining; adapter, three
      devices); Turn Bluetooth Off and On reached the mock (`Powered = false`/`true`) and the
      menu followed; Connect on Keyboard reached the mock and gained its tick; stopping the
      mock hid both within 5 s. Not checked: Disconnect and the failing `Broken` device (the
      same code path as Connect; the failure only logs).
      *Owner:* battery on a laptop if one is available; pairing and connecting a device on
      the PC.
- [ ] **3.12 Menu extra: network** · Sonnet. After 3.2b. NetworkManager over zbus: wired
      and Wi-Fi state in the icon, the Wi-Fi list with signal and lock, Wi-Fi on/off, join
      a saved or open network, disconnect. Joining a new protected network needs a secret
      agent and is deferred (`nmtui` until then; note it in the plan). Split like 3.11 into
      3.12a, 3.12b and 3.12c.
- [x] **3.12a Network A: NetworkManager client** · Sonnet. After 3.2b. `neue-dbus::network`
      (Linux-only, zbus proxies on the system bus) and plain models: `State` (Wi-Fi switch,
      wired devices, the Wi-Fi device) with `Wired`, `Wifi` and `AccessPoint` (SSID, signal,
      lock, saved, active). Calls: Wi-Fi on and off, scan, join (a saved or open network),
      connect a wired device, disconnect. Change signals as a blocking "read again" iterator
      (`NetworkChanges`, like `PowerChanges`). A `mock_network_manager` example for the VM
      (`docs/dev-setup.md` §1.6c).
      *Accept:* unit tests against a mock NetworkManager over `testing::pair()`: no devices,
      wired up and down, Wi-Fi off and on, the AP list (merged by SSID, sorted, saved and
      active marked), join saved, join open, a protected unsaved network refused, wired
      connect and disconnect, a device or AP appearing and disappearing, noise not signalled.
      *Done:* `neue_dbus::network::{Network, State, Wired, Wifi, AccessPoint, NetworkChanges}`.
      A read is `GetAll` calls (NetworkManager has no object manager); devices and APs that
      vanish mid-read are skipped. Unmanaged devices (`lo`) are left out; with several Wi-Fi
      devices the first by interface name is used. Access points of one SSID are one entry
      (the active one, else the strongest); hidden networks (no SSID) are not listed.
      `join` activates the saved connection for the SSID (preferring one NetworkManager
      lists as available for the device), else for an open network uses
      `AddAndActivateConnection` with a minimal connection NetworkManager saves, else fails
      with `Error::SecretRequired` (`AccessPoint::joinable` is false, so a menu can grey it
      out). `connect` (wired) activates the device's first available connection
      (`Error::NoConnection` without one); `disconnect` is `Device.Disconnect` for both
      kinds. `NetworkChanges` watches every signal under NetworkManager's path and drops
      `LastSeen`/`LastScan` and traffic counters; a scan still produces bursts, so 3.12c
      debounces. 14 unit tests (arm64 Linux via `cargo xtask linux`). The example was
      smoke-tested on a private bus with `busctl` (disconnect and activate move the wired
      device's `State`; activating an AP moves `ActiveAccessPoint`). No ADR: zbus only, no
      new dependency or protocol. Protected networks that aren't saved are deferred until a
      secret agent exists (`nmtui` until then).
- [x] **3.12b Network B: icon extra in the kit** · Sonnet. Can start from 3.12a's shapes.
      `ExtraIcon::Network` (wired, Wi-Fi with signal level, Wi-Fi off, disconnected),
      original vector icons in `neue_paint::materials::menu_bar`, light and dark; portable
      `NetworkView`, `NetworkCommand` and `bar::network_menu` (Wi-Fi switch, the network
      list with a signal glyph, a lock and a tick on the active one, wired entries,
      Disconnect), `BarContent::network` (`None` hides it). Gallery Shell page entries,
      AccessKit labels.
      *Accept:* goldens (states × light/dark × scale 1 and 2); kit tests.
      *Done (ADR with 3.12c):* `neue_kit::desktop::{ExtraIcon::Network(NetworkIcon), NetworkIcon}`
      (`Wired`, `Wifi { level }` with 0 to 3 arcs lit, `WifiOff` faded and struck through,
      `Disconnected` the wired icon faded); every state has one size, so the bar doesn't
      shift. Drawn by `neue_paint::materials::menu_bar::{wifi, wired, strike, wifi_label,
      wired_label}` with metrics in `neue_theme`. New kit feature `MenuItem::signal(bars,
      locked)` (`neue_kit::Signal`): a Wi-Fi fan, and a padlock before it, at an item's
      right, read as its accessible description ("strong signal, password needed"); drawn
      by `neue_paint::materials::{menu_signal, menu_lock}`. `neue_shell::bar`: portable
      `NetworkView` (`WiredView`, `WifiView`, `WifiNetworkView`), `NetworkCommand`
      (`SetWifi`, `Join`, `DisconnectWifi`, `ConnectWired`, `DisconnectWired`, `Scan`),
      `signal_bars` (1 to 3 arcs from 0 to 100 %), `network_icon` (a wired connection that
      is up wins over Wi-Fi), `network_menu` (state line and switch; the networks with a
      tick on the joined one, a signal glyph and a padlock, a protected network that isn't
      saved greyed out; Disconnect Wi-Fi; a submenu per Ethernet device: Connect,
      Disconnect or "Cable Unplugged"), `BarContent::network` (`None` hides it),
      `BarHandles::network`; `menu_bar` takes a closure for `NetworkCommand`, needs
      `M: Clone`, and the network title sends `Scan` when its menu opens. Extras order,
      right to left: clock, battery, network, Bluetooth. The shell passes `None` and only
      logs network commands until 3.12c. Goldens: `shell-bar` gained a network-open state
      (the window is larger, the others changed: the extra is in the bar) and
      `shell-extras` a row of network icons; 2 kit tests, 4 shell tests, 1 paint test. The
      Linux-only `shell.rs` passes clippy on arm64 Linux (`cargo xtask linux`). Not
      checked: the extra in a running session (nothing feeds it until 3.12c).
- [ ] **3.12c Network C: shell wiring** · Sonnet. After 3.12a and 3.12b. A worker thread in
      `extras.rs` following NetworkManager (debounced), bar sync, the extra absent without
      NetworkManager, menu actions through `cx.spawn` (Wi-Fi switch, join, connect,
      disconnect, scan when the menu opens). Addendum to ADR 0083.
      *Accept:* in the VM with the mock NetworkManager: the extra shows with screenshots, the
      wired connection can be toggled, the Wi-Fi list shows and joining Home and Cafe reaches
      the mock; the extra is gone with the mock stopped.
      *Owner:* Wi-Fi list and joining a saved network on the PC.
      *Done (not ticked, VM check open):* `extras::{run_network, network_view}` and the shell
      wiring as in the addendum to ADR 0083. 1 new test (`network_view`); the existing
      `follow` tests take the debounce. Clippy and tests pass on arm64 Linux. In the VM
      without NetworkManager the extra is absent (screenshot checked). **Not done:** the
      mock NetworkManager run (the VM's sudo refused `1234`); the steps are in
      `docs/dev-setup.md` §1.6c. Tick this box once they pass: extra shows, Wired
      connection 1 toggles, Home and Cafe join (the mock log shows the calls), Library is
      greyed, and the extra is gone with the mock stopped.
- [x] **3.13a Status-icon tray A: watcher, host and item client** · Sonnet. After 3.7a.
      Split from 3.13 (same shape as 3.12). `neue_dbus::status_notifier`: the
      `StatusNotifierWatcher` the shell owns, a threadless `Host` and the item client; icons by
      name or pixmap, tooltips, activation, and menus through the 3.7a DBusMenu client. ADR 0084.
      *Accept:* unit tests; the host and watcher on a real bus with a mock item.
      *Done:* `Watcher` (`own` takes the name and forgets departed connections, `serve` for
      tests), `WatcherSignals`, `ItemInfo`/`StatusItem`/`ItemSignals`, `Host`
      (`announce`, `initial`, `registered`/`unregistered`/`changed` returning `HostEvent`, the
      actions, `menu`). 9 unit and 7 peer-to-peer tests (`tests/status_notifier.rs`); examples
      `sni_host` and `mock_sni_item` (`docs/dev-setup.md` §1.6d). Checked in OrbStack on a real
      session bus: the host registers, the mock item is added with its tooltip, pixmap and
      menu, `Activate` reaches it, a menu click turns its status to NeedsAttention (a
      `changed` event), and killing it removes it. Clippy and tests pass on arm64 Linux.
- [x] **3.13b Status-icon tray B: the tray area in the bar** · Sonnet. After 3.13a. The kit
      and `neue_shell::bar`: a dynamic group of extras that can gain and lose items without
      rebuilding the bar, an `ExtraIcon::Image` (a named icon or an RGBA pixmap, one fixed
      size, light and dark treatment), the tooltip as accessible description, `Passive` items
      hidden and a `NeedsAttention` item showing its attention icon. Placement: left of the
      other extras (right to left: clock, battery, network, Bluetooth, tray). ADR for the
      placement and the overflow policy.
      *Accept:* gallery entry; goldens (states × light/dark × scale 1 and 2); kit and shell
      tests.
      *Done:* ADR 0085. Kit: `ExtraIcon::Image(Icon)` (16 px, `ExtraIcon::pixmap` from RGBA;
      `ExtraIcon` is no longer `Copy`), `MenuBar::{with_tray, min_width}`,
      `Handle<MenuBar>::set_tray`, `MenuTitle::description` and `Handle::set_description`;
      icons that don't fit before the titles are stashed, newest first. Paint:
      `materials::menu_bar::image_label` (pictures softened on the dark bar). Theme:
      `TRAY_ICON_SIZE`, `TRAY_DARK_OPACITY`, `TRAY_OVERFLOW_GAP`. `neue_shell::bar`:
      `TrayStatus`, `TrayItemView`, `tray_title`, `tray_titles`, `BarContent::tray`,
      `BarHandles::tray`; `menu_bar` takes a closure for each item's menu (the shell passes an
      empty one until 3.13c). Goldens: new `shell-tray` (items in each state, a crowded bar,
      a tray menu open); the existing ones are unchanged. 7 kit, 4 shell and 1 paint tests;
      gallery Shell page has the tray section. Not checked: a bar in a running session
      (nothing feeds the tray until 3.13c).
- [ ] **3.13c Status-icon tray C: shell wiring** · Sonnet. After 3.13a and 3.13b. The shell
      owns the watcher (`Watcher::own`) and runs the host on a worker; the bar follows
      `HostEvent`s; named icons resolve through `neue-desktop`'s icon theme (and the item's
      `IconThemePath`); a click calls `Activate`, or opens the DBusMenu when `ItemIsMenu` is
      set; right click opens the menu (or `ContextMenu` without one); menu items send
      `clicked` through `cx.spawn`. Addendum to ADR 0084. Needs a kit addition first (ADR 0085
      §8): a tray title that sends a message on a primary press and opens its menu on a
      secondary one (today a press opens the menu).
      *Accept:* in the VM a tray app (`mock_sni_item`, then `nm-applet --indicator`) shows, its
      menu opens and its items act (the mock's log shows the calls).
      *Owner:* a real tray app on the PC.
      *Done (not ticked, VM check open):* kit `MenuTitle::on_press` and `on_context` (a
      primary press sends a message, a secondary one opens the menu or sends the context
      message; Return, Space, Down, the menu key and the accessibility actions follow).
      `neue_shell::bar::{TrayActions, update_tray_title}`; `tray::run` (worker: watcher,
      host, menus read on threads, icons resolved from `IconThemePath`, the theme or the
      pixmap) and the shell's `Tray` state; ADR 0084 addendum. 3 kit and 7 shell tests, and
      an ignored test that runs the real worker against `mock_sni_item` on a private session
      bus in OrbStack (the mock's log showed `Activate`, `ContextMenu` and `clicked 3`;
      `mock_sni_item`'s menu now has the items under the root, as real apps do). **Not
      done:** the bar in a running session: the VM's sudo refused `1234`, so the console
      couldn't be taken over; the steps are in `docs/dev-setup.md` §1.6d. Tick this box once
      they pass: the mock's disc shows left of the network extra, a left click logs
      `Activate`, a right click opens the menu, Mark Attention turns the disc red, Quit
      removes it; then `nm-applet --indicator` shows.
- [x] **3.14a System keys, brightness and the OSD** · Opus. After 3.1 and 3.2b. Split
      from 3.14 (volume, brightness and OSDs). `ShellEvent::SystemKey` through
      `neue-platform` and the kit (the shell repeats held keys itself); `neue_kit::desktop::Osd`,
      an overlay layer surface without input: the rounded-square HUD (`materials::hud`,
      ADR 0059) with a speaker or sun glyph and a 16-step level bar, fading with a spring
      (reduced motion: no fade), AccessKit label. Brightness: the backlight read from
      `/sys/class/backlight` and set through logind `SetBrightness` (`neue_dbus::login`).
      Until 3.14b the volume keys move an in-memory level.
      *Accept:* OSD goldens (volume, muted, brightness × light/dark × scale 1 and 2); gallery
      Shell page entry; brightness unit test against a fake sysfs folder; in the VM a volume
      key sent with `ydotool` shows the OSD (screenshot).
      *Owner:* brightness keys on a laptop: the screen follows and the OSD shows the sun.
      *Done:* ADR 0086. The Wayland driver repeats the stepping keys at
      `wl_keyboard.repeat_info`'s rate (`SystemKeyState::Repeated`);
      `neue_paint::materials::osd` (`hud_rounded`, 20 px corners, quarter-lit steps),
      `neue_theme::materials::osd` tokens, `Osd`/`Handle<Osd>::{show, hide}`,
      `LayerSpec::pointer`, `Window::blur_behind_rounded`, layer-surface roots no longer
      fill a background; `Login::set_brightness`; `neue_shell::levels` (stepping, the
      backlight lookup) and the shell's `osd.rs` (the surface opens on the first key and
      closes after the fade). Goldens `shell-osd-*`; 5 kit, 4 paint, 5 shell, 1 dbus and 1
      `neue-comp` (real driver: press, repeat, release) tests. VM (arm64): Volume Up shows
      the OSD at 9 of 16 steps, a held Volume Down repeats (about 30/s, the compositor's
      rate) down to 0 (no waves), the OSD fades and closes, a click on it reaches the
      desktop below. Not checked: brightness (no backlight in the VM).
- [x] **3.14b PipeWire volume backend** · Opus. After 3.14a. ADR for `pipewire-rs` (a C
      dependency through bindgen and clang; licence check; also used by 3.22). A new
      Linux-only crate `neue-audio`: the PipeWire loop on its own thread, the default sink
      from the `default` metadata, its volume (cubic, as `wpctl` shows it) and mute, the
      list of sinks, setting the default sink. Volume and mute keys use it, and mic mute
      toggles the default source (with the OSD). PKGBUILD and CI dependencies.
      *Accept:* in the VM the volume keys (sent with `ydotool`) change the sink volume shown
      by `wpctl get-volume @DEFAULT_AUDIO_SINK@`, and the OSD shows the real level.
      *Owner:* on the PC (a real card with routes): the volume keys change what you hear and
      `wpctl get-volume @DEFAULT_AUDIO_SINK@` follows; after a reboot the level is kept.
      *Done:* ADR 0087. `pipewire` 0.10; `neue-audio` (`Audio::{start, set_volume,
      set_muted, set_default}`, `State`/`Endpoint`/`Kind`, `cubic`/`linear`; the thread
      reconnects every 2 s) and its `audio` example; `OsdGlyph::{Microphone,
      MicrophoneMuted}` (goldens `shell-osd-microphone{,-muted}`, gallery); the shell's
      `osd.rs` drives the default sink and source. 6 `neue-audio` and 1 new shell test.
      VM (arm64, null sinks from `docs/vm-audio.conf`, §1.6e): from 0.50 Volume Up gives
      0.56 in `wpctl` and 9 of 16 steps on the OSD, three Downs 0.38, Mute `[MUTED]` with the
      muted speaker, a held Up reaches 1.00, Mic Mute mutes the source with the
      struck-through microphone; the example's `default neue-test-sink-2` moves the `*` in
      `wpctl status`; after `systemctl --user restart pipewire wireplumber` the shell
      reconnects in 2 s and the keys work again. Not checked: a card with routes (the VM's
      card belongs to the greeter's seat), and the VM's own build (no `clang` there, sudo
      refused `1234`; binaries were built in OrbStack).
- [x] **3.14c Volume extra** · Opus for the kit part, then Sonnet. After 3.14b. A slider
      item in kit menus; the volume extra in the bar (ADR 0083 pattern): glyph by level, a
      menu with the slider, Mute and the output devices as a radio list.
      *Accept:* goldens of the extra and its menu; in the VM the slider and the device
      choice show in `wpctl status`.
      *Owner:* the volume you hear follows the keys and the slider; brightness keys on a
      laptop if available.
      *Done:* ADR 0045 and 0083 addenda. `MenuItem::slider`/`on_change`
      (`Anchor::on_change`, `TitleAction::Changed`; menu-bar menus only, left out of
      DBusMenu), `menu` tokens `SLIDER_ITEM_{HEIGHT,WIDTH,STEP}`; `ExtraIcon::Volume`
      (`menu_bar::speaker{,_label}`, waves as on the OSD, faded when muted);
      `bar::{VolumeView, OutputView, VolumeCommand, volume_icon, volume_menu,
      ExtraMessages}`; the shell's `Keys::{volume_view, volume_command}`. Goldens
      `shell-bar-volume-open-*`, a volume row in `shell-extras-*` (specimens widened: the
      bar to 520 px, the extras to 120 px, so every `shell-bar-*` golden changed); 3 kit
      (`menu_slider`), 1 kit model, 1 paint, 2 shell `bar` and 1 shell `osd` tests. VM
      (arm64, null sinks of §1.6e): the extra shows the default sink (0.31, one wave); a
      press on the slider gave 0.44 in `wpctl`, a drag 0.78, the menu stayed up and the icon
      gained waves; Test Output moved the `*` in `wpctl status`, the reopened menu showed it
      selected at its 0.50; Mute gave `[MUTED]` and the faded speaker; Volume Up then
      unmuted at 0.56. Not checked: keyboard stepping in the VM (unit tests only).
- [x] **3.14d Media keys and the keyboard backlight** · Sonnet. After 3.14a. Play, pause,
      next, previous and stop go to the active MPRIS player (`neue_dbus`, the most recently
      playing one); keyboard brightness keys set the `leds` `*::kbd_backlight` device
      through logind `SetBrightness`, with a keyboard glyph on the OSD (goldens).
      *Accept:* unit tests with a mock MPRIS player; in the VM `playerctl status` follows
      the play key sent with `ydotool`.
      *Owner:* keyboard backlight keys on a laptop that has one.
      *Done:* ADR 0088. `neue_dbus::mpris` (`Mpris::{session, players, active, send, watch}`,
      `Command`, `pick`; the memory of who played last comes from a signal thread and from
      key presses); `Backlight::find_keyboard` (`leds`, may go to 0; `Backlight::subsystem`);
      `OsdGlyph::Keyboard` (goldens `shell-osd-keyboard`, gallery, label "Keyboard
      brightness"); the shell's `osd.rs` routes the five media keys (off the event loop) and
      the keyboard brightness keys; `mock_mpris` example. 8 `mpris` (4 on a mock bus and
      player), 2 `levels` tests. VM (arm64, two and then three mock players, §1.6f): Play
      pauses the playing one and starts it again, Pause, Stop, Next and Previous reach it
      (`playerctl -a status` and the mock's log), a paused player that was last to play wins
      over one earlier on the bus, also when it played only through a signal. Not checked:
      the keyboard backlight (no LED in the VM).
- [x] **3.15 Notifications**, split into 3.15a–d (a D-Bus server, a new widget and
      material, activation tokens across platform, kit and compositor, and the shell
      wiring). The Notification Center panel is Phase 6.
- [x] **3.15a Notifications server** · Sonnet. After 3.2b. `neue_dbus::notifications`:
      `org.freedesktop.Notifications` at `/org/freedesktop/Notifications` (spec 1.2):
      `Notify` (replace-id; actions; hints `urgency`, `image-data`/`image_data`/`icon_data`,
      `image-path`/`image_path`, `desktop-entry`, `resident`, `transient`, `category`),
      `CloseNotification`, `GetCapabilities`, `GetServerInformation`; signals
      `NotificationClosed` (with its reason), `ActionInvoked`, `ActivationToken`. A
      portable store owns replacement, expiry (−1 is the server's default, 0 never;
      critical never expires) driven by the caller's clock; the server hands out ids.
      `neue-config` gains `[notifications]` (`do_not_disturb`, `banner_timeout`) with
      defaults. ADR.
      *Accept:* tests over a peer-to-peer connection with a scripted client cover every
      method and signal, replace-id, expiry and each close reason; store unit tests.
      *Done:* ADR 0089. `neue_dbus::notifications` (`Notifications::{serve, own, close,
      invoke, is_open}`, `Request`, `Notification` with `icon()`/`buttons()`, `Hints`,
      `Image`, `Urgency`, `Timeout`, `CloseReason`, `IconSource`); the portable
      `neue_shell::notifications` (`Store<T>`, `Lifetime`, `lifetime`); `shell.toml`
      `[notifications] do-not-disturb, banner-timeout`. 9 unit and 6 peer-to-peer
      `neue-dbus` tests, 7 store tests, 1 config test. Nothing owns the name yet (3.15d),
      so there is no VM check.
- [x] **3.15b Banner and alert widgets** · Opus. After 3.15a (its types).
      `neue_kit::desktop::Notification`: a banner (app icon or image, bold title, body
      clipped to two lines, a close button on hover) and an alert (the same with action
      buttons). Material in `neue-paint` with `neue-theme` tokens, light and dark. A
      portable stack model: positions top right under the bar, slide in and out with a
      spring (reduced motion: no slide), the stack closing up. AccessKit roles and labels,
      the buttons reachable by keyboard.
      *Accept:* gallery Shell page entry; goldens of banner, banner with image, banner with
      hovered close, alert with two actions × light/dark × scale 1 and 2; stack model unit
      tests.
      *Owner:* banners and alerts look right on the PC, light and dark.
      *Done:* ADR 0090. `neue_paint::materials::notification` (`Parts`, `panel`, `picture`,
      `close_button`, `panel_height`) with tokens in `neue_theme::materials::notification`;
      `neue_kit::desktop::{Notification, NotificationContent, NotificationEvent}` (close
      and two action buttons as child widgets, `Handle::set_content` in place, Escape
      closes, `Hovered` reported for pausing); `neue_kit::desktop::NotificationStack`
      (`push`, `set_height`, `remove`, `step`, `placements`, `extent`); `Fixture::hover_at_px`.
      Goldens `shell-banner` (normal, close hovered, focused), `shell-banner-image`,
      `shell-alert` (normal, close hovered); gallery Shell page. 5 material, 7 stack model and
      5 widget tests (clicks, hover, Tab order and keys, AccessKit, replacement). Not done
      here: the container that hosts the panels and applies the model (3.15d).
- [x] **3.15c Activation tokens for the shell** · Opus. After 3.2a. `Ctx::activation_token`
      through `neue-platform` (an `xdg_activation_v1` token for a surface and the input
      serial) and the kit; the compositor grants tokens asked for by the shell's client
      even without keyboard focus (a banner never has it). ADR 0066/0067 addendum.
      *Accept:* a headless `neue-comp` test with the real driver: the shell's token
      activates another client's toplevel; a token from an unfocused ordinary client is
      still refused.
      *Done:* ADR 0066 and 0067 addenda. `Ctx::activation_token(window) -> TokenRequest`
      and `App::on_activation_token` (one answer per request; the window's own surface and
      the latest serial; `None` on winit); `Cx::activation_token(window, |token| Message)`
      in the kit; `token_created` grants every token from the shell's client. Tests: 1
      platform, 1 kit, headless `platform_shell` (the shell's token brings another
      client's window forward from no focus; an unfocused ordinary client only gets
      attention; tokens single-use; the test fails without the grant). No VM check: nothing
      in the shell asks for a token until 3.15d.
- [ ] **3.15d Shell wiring**, split into 3.15d1–d4 (a layer surface that follows its
      content, the kit container that hosts the panels, the shell wiring, and the bar
      extra). Split because the surface can't be resized from an app yet (`neue-platform`
      opens every window with `WindowSizePolicy::User`) and a fixed, oversized transparent
      surface would swallow clicks meant for the apps under it (ADR 0090).
- [ ] **3.15d1 Layer surface follows its content** · Opus. After 3.15c. Opt-in content
      sizing for layer windows in `neue-platform` (the surface is resized as the root
      widget's size changes, with `WindowSizePolicy::Content` or an equivalent), passed
      through `LayerSpec` and `Window` in `neue-kit`. The input region is the content's
      rect, so clicks outside it reach the windows below. The winit driver ignores it. ADR
      (addendum to 0066/0090).
      *Accept:* a headless `neue-comp` test with the real driver: a layer surface anchored
      top right takes the size of its content, follows it when it grows and shrinks, and a
      click outside the content reaches the client under it; kit test of the opt-in.
- [ ] **3.15d2 Notification host** · Opus. After 3.15b; the surface it lives in comes from
      3.15d1. `neue_kit::desktop::NotificationHost`: a container widget that owns the
      `Notification` panels, applies `NotificationStack` placements on animation frames
      and reports its `extent()` as its size. Add, replace in place (`set_content`),
      remove (slides out), and per-id `NotificationEvent`s to the app. Respects reduced
      motion.
      *Accept:* gallery Shell page entry; goldens of a stack of three (one an alert) ×
      light/dark × scale 1 and 2; tests for slide in and out, closing up, replacement,
      resizing to `extent()`, and keyboard reach of every panel's buttons.
- [ ] **3.15d3 Notifications in the shell** · Sonnet. After 3.15a–c, 3.15d1 and
      3.15d2. The shell owns `org.freedesktop.Notifications` and shows banners top right
      under the bar on the output with the active window, in one layer surface per
      output sized by the host, stacked, timing out (paused while hovered); replace-id
      updates in place; critical and never-expiring (`expire_timeout` 0) notifications
      show as alerts that stay until dismissed (`Lifetime::Never`, ADR 0089). A click on
      a banner sends `ActivationToken` (`Cx::activation_token`, 3.15c) and
      `ActionInvoked("default")` and closes it (unless `resident`); the action buttons
      send their keys. App icon from `app_icon` or `desktop-entry` through the icon
      theme, or `image-data`/`image-path`. Do Not Disturb (`shell.toml`
      `[notifications] do-not-disturb`) hides banners, and the store still counts them.
      *Accept:* in the VM `notify-send` with `--action`, `--urgency=critical`,
      `--replace-id`, `--expire-time` and `--wait` behaves as expected (screenshots,
      `notify-send` prints the chosen action), and Do Not Disturb hides banners.
      *Owner:* a real app's notification (a browser or a chat client) opens the app
      window on click.
- [ ] **3.15d4 Notifications extra in the bar** · Sonnet. After 3.15a (independent of
      the rest). A notifications extra at the far right of the bar, dimmed while Do Not
      Disturb is on, with a menu holding a Do Not Disturb toggle saved to `shell.toml`
      (the shell follows the file); in Phase 6 a click on it opens the Notification Center.
      Icon in `neue-paint`/`neue-theme` as the other extras', light and dark.
      *Accept:* goldens of the extra (on, Do Not Disturb) × light/dark × scale 1 and 2;
      menu tests (the toggle's message, its check state); gallery Shell page entry.

## C. Dock
- [ ] **3.16 Dock A: shelf, launching and running apps** · Opus. After 3.1–3.3. The
      glass shelf material in `neue-paint` (procedural, light and dark, reflections;
      goldens) and the dock layer surface at the bottom: pinned apps from config, launch
      on click, running indicators, click to activate (and cycle its windows), launch
      bounce and attention bounce (springs, reduced motion), and minimise-target
      rectangles sent to the compositor.
      *Accept:* goldens of the dock; in the VM launching foot from the dock, its indicator,
      activation and an attention bounce (`xdg-system-bell`), checked by screenshots.
      *Owner:* the shelf's look on the PC, light and dark.
- [ ] **3.17 Dock B: magnification, edges and editing** · Sonnet. After 3.16.
      Magnification, the 2D dock on the left and right edges, drag to rearrange, drag off
      to remove, Keep in Dock, context menus with the app's windows and Quit, the Trash
      tile (empty/full, Empty Trash; 3.3), and tiles for minimised windows (a thumbnail via
      toplevel capture, ADR 0023; click restores).
      *Accept:* goldens; in the VM each behaviour checked with `ydotool` and screenshots.
      *Owner:* magnification feels right with a real mouse and trackpad.

## D. Session
- [ ] **3.18 Lock screen** · Opus. After 3.2b. An `ext-session-lock-v1` client on linen
      (ADR 0015 §3): user picture and name, password field, a shake on a wrong password
      (reduced motion). ADR for the PAM binding (a C dependency) and for where the locker
      lives (in the shell or its own binary). Locks from the logo menu, a shortcut, idle
      (`ext-idle-notify`, timeout in config), logind's `Lock` signal and before sleep
      (a delay inhibitor).
      *Accept:* goldens; in the VM: lock, a wrong password shakes, the right one unlocks;
      killing the locker leaves the session locked and a new one can unlock;
      `loginctl lock-session` locks.
      *Owner:* lock before suspend and unlock after resume on the PC.
- [ ] **3.19 Greeter** · Sonnet. After 3.18 (reuses its look). `neue-greeter` speaks the
      greetd IPC (check the `greetd_ipc` licence) and runs under a minimal
      `neue-comp --greeter` mode (no shell; exits when the greeter does). User list (UID
      ≥ 1000), password, errors, Shut Down, Restart, Sleep (logind). Replaces `agreety` in
      `session/greetd/config.toml`.
      *Accept:* in the VM, a reboot shows the greeter; a wrong password shows an error, the
      right one starts the neueOS session.
      *Owner:* the greeter on the PC, including on the NVIDIA card.
- [ ] **3.20 Polkit agent** · Sonnet. After 3.2b. `neue-polkit-agent` registers as the
      session's authentication agent over zbus, runs polkit's own
      `polkit-agent-helper-1` as a subprocess (not linked), and shows a kit dialog: lock
      icon, message, user, password, Cancel and Authenticate, details disclosure, a shake
      on failure.
      *Accept:* in the VM, `pkexec true` asks, a wrong password fails, the right one
      succeeds; Cancel fails the action.

## E. Portal and Preferences
- [ ] **3.21 Portal A: skeleton, Settings and Screenshot** · Sonnet. After 3.6.
      `xdg-desktop-portal-neue`: the `.portal` file, `neueos-portals.conf` (ours first,
      gtk as the fallback for the rest), the Settings portal (colour scheme, accent
      colour, contrast from `neue-settingsd`) and Screenshot (output capture, ADR 0022;
      PNG into `~/Pictures`).
      *Accept:* in the VM, `gdbus` calls to both portals return the expected results; a
      GTK4 app follows dark mode when it is switched through `neue-settingsd`.
- [ ] **3.22 Portal B: ScreenCast** · Opus. After 3.14 (the `pipewire-rs` ADR) and 3.21.
      A PipeWire video stream from output and toplevel capture (ADR 0022, 0023), shm
      buffers first (dmabuf later), cursor modes, and a source picker dialog.
      *Accept:* in the VM, `gst-launch-1.0 pipewiresrc` on the portal's node shows
      frames (a test script in `docs/vm-env.sh`).
      *Owner:* screen sharing in a browser call on the PC.
- [ ] **3.23 Portal C: FileChooser (simple)** · Sonnet. After 3.21. Open, open multiple,
      save and choose folder, as a kit window parented through xdg-foreign: sidebar (Home,
      Desktop, Documents, Downloads), table view of the folder, path control, filters
      popup, name field for save, hidden files toggle.
      *Accept:* golden of the dialog; in the VM `GTK_USE_PORTAL=1 gtk4-demo` opens and
      saves through it.
      *Owner:* open and save from a Flatpak app on the PC.
- [ ] **3.24 Preferences v0: app, Appearance and Dock** · Opus. After 3.6. The
      Preferences app (PLAN.md §3.5): pane grid, Show All, back and forward, talking to
      `neue-settingsd` (the mock on macOS). Appearance: light/dark, accent, increased
      contrast, reduced motion. Dock: size, magnification, position. Sets the pattern for
      first-party apps.
      *Accept:* goldens of both panes; runs on macOS; in the VM a change shows at once in
      the shell and the gallery.
      *Owner:* VoiceOver on macOS reads the pane grid and both panes.
- [ ] **3.25 Preferences: Displays, Keyboard and Trackpad** · Sonnet. After 3.24. Keys
      the compositor config already has: outputs (mode, scale, position as a list),
      keyboard (layouts, options, repeat rate and delay), touchpad and pointer (tap to
      click, natural scroll, speed, click method).
      *Accept:* goldens; in the VM each change reaches the compositor (`neuectl outputs`,
      key repeat, a pointer setting).
      *Owner:* scale and refresh rate on the PC's display; trackpad settings on a laptop
      if available.
- [ ] **3.26 Preferences: Sound** · Sonnet. After 3.14 and 3.24. Output and input device,
      output volume and mute, input level.
      *Accept:* golden; in the VM the pane and `wpctl` agree.
      *Owner:* switching output device on the PC.

## F. Distribution and exit
- [ ] **3.27 Packages, user units and `neueos-desktop`** · Sonnet. PKGBUILDs following
      ADR 0028 for `neue-settingsd`, `xdg-desktop-portal-neue`,
      `neue-greeter`, `neue-polkit-agent` and `neue-preferences`; user units in
      `neue-session.target` (or D-Bus activation where it fits); the greetd config; and
      the `neueos-desktop` meta-package (PipeWire, WirePlumber, NetworkManager, BlueZ,
      UPower, greetd, polkit, xwayland-satellite, Papirus, DejaVu, Flatpak,
      xdg-desktop-portal and the gtk fallback). `cargo xtask package` knows them all.
      *Accept:* packages build for aarch64 and x86_64; in the VM, installing
      `neueos-desktop` and rebooting gives the greeter and a session with every service
      running (`systemctl --user status`, checked by the agent).
- [ ] **3.28 Dev ISO and `neue-install-core`** · Sonnet. x86_64 only (decided
      2026-11-05; the aarch64 VM keeps updating through packages). An archiso profile in
      `distro/iso/` with our packages in a local repo, `cargo xtask iso`, and
      `neue-install-core` (PLAN.md §5.3) with a CLI driven by a TOML answer file: Btrfs
      subvolumes, pacstrap, systemd-boot, microcode, a user, `neueos-desktop`. No LUKS
      and no GPU detection yet (Phase 8). The installer is Rust, not a shell script.
      *Accept:* the ISO builds; in x86_64 QEMU (OrbStack amd64) it boots, the CLI installs
      into a disk image from an answer file, and the installed system boots to the
      greeter.
      *Owner:* install from a USB stick on the PC (wipes the target disk: use a spare
      one).
- [ ] **3.29 Owner runbook** · Sonnet. After every task above. Writes
      `docs/checks/phase-3-owner.md` in the format of `docs/checks/phase-2-owner.md`
      from every task's *Owner:* line, plus Phase 2's open owner steps (2.26: the gallery
      in light and dark, identical on macOS and in the VM; 2.4 IME and VoiceOver). Parts
      in the order the owner does them: the PC install, the VM, daily use on the PC,
      macOS. Ends with a single report-back.
      *Accept:* every *Owner:* line maps to a step; every step has a command or action and
      an Expected.
- [ ] **3.30 Exit check** · Sonnet, plus the owner. After the runbook's fixes (3.31 on).
      The owner re-runs the failed steps, then logs in from the greeter on the PC and uses
      neueOS as the daily desktop for a day, noting anything that forced a switch back.
      Tick Phase 3 (and Phase 2's 2.26) in `PLAN.md` §8.

## Fixes from the owner run
Added after the owner reports back on 3.29, numbered from 3.31.

- **Exit:** log in from the greeter on your PC and use neueOS as the daily desktop.
