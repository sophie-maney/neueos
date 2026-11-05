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
- [ ] **3.9 Fallback menu and menu search** · Sonnet. After 3.7b and 3.10c. ADR 0005 §4:
      App, File, Edit, Window and Help for windows with no menu address. Edit items send
      key chords through `neue-shell-v1`; Window items run compositor actions; New Window
      uses the entry's `new-window` action (3.3) or is disabled. Help's search field finds
      items in the current menus, whatever their source.
      *Accept:* in the VM, Copy and Paste from the fallback menu work in foot and Firefox;
      menu search finds and runs an item in the gallery.
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
- [ ] **3.11 Menu extras: battery and Bluetooth** · Sonnet. After 3.2b. zbus proxies for
      UPower and BlueZ (not `bluer`, which links libdbus). Battery: icon with charge and
      state, menu with percentage and time left, hidden without a battery. Bluetooth:
      on/off, paired devices with connect and disconnect, hidden without an adapter.
      *Accept:* unit tests against mock D-Bus services; VM screenshots with mocked
      services where the VM lacks the hardware.
      *Owner:* battery on a laptop if one is available; pairing and connecting a device on
      the PC.
- [ ] **3.12 Menu extra: network** · Sonnet. After 3.2b. NetworkManager over zbus: wired
      and Wi-Fi state in the icon, the Wi-Fi list with signal and lock, Wi-Fi on/off, join
      a saved or open network, disconnect. Joining a new protected network needs a secret
      agent and is deferred (`nmtui` until then; note it in the plan).
      *Accept:* unit tests against a mock NetworkManager; in the VM the wired connection
      shows and can be toggled.
      *Owner:* Wi-Fi list and joining a saved network on the PC.
- [ ] **3.13 Status-icon tray** · Sonnet. After 3.7a. `StatusNotifierWatcher` and a host in
      `neue-dbus`; icons by name or pixmap, tooltips, activation, and menus through the 3.7a
      DBusMenu client.
      *Accept:* unit tests; in the VM a tray app (`nm-applet --indicator` or a test SNI
      item) shows, its menu opens and its items act.
- [ ] **3.14 Volume, brightness and OSDs** · Opus. After 3.1 and 3.2b. ADR for
      `pipewire-rs` (a C dependency, also used by 3.22). The volume extra (slider, mute,
      output device), media and volume keys routed by `neue-shell-v1`, brightness through
      logind `SetBrightness` on the backlight, and the rounded-square HUD OSD for both
      (springs, reduced motion; ADR 0059 material).
      *Accept:* goldens of the OSD; in the VM the volume keys (sent with `ydotool`) change
      the sink volume shown by `wpctl` and show the OSD.
      *Owner:* the volume you hear follows the keys and the slider; brightness keys on a
      laptop if available.
- [ ] **3.15 Notifications** · Sonnet. After 3.2b. An `org.freedesktop.Notifications`
      server in the shell (interface in `neue-dbus`): banners at the top right under the
      menu bar, sliding in with a spring, stacking, timing out; alerts for critical or
      persistent ones; default action on click and action buttons, with an activation
      token; app icon from the entry or `image-data`; Do Not Disturb in config with a menu
      item. The Notification Center panel is Phase 6.
      *Accept:* goldens of banner and alert states; in the VM, `notify-send` with actions,
      urgency and replace-id behaves as expected.

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
