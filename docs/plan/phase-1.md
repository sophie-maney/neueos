# Phase 1: Compositor MVP

Task list for Phase 1 of the roadmap in `PLAN.md` §8. Architecture and decisions
live in `PLAN.md` and `docs/adr/`.

One task = one thread = one PR, as in Phase 0. Rough order: 1.1 → 1.2 → 1.3 first, then
1.4–1.8 in any order, then 1.9 onward. Unless noted, tasks touch only `compositor/neue-comp`
and are checked with `cargo xtask linux cargo xtask check` plus a headless test.
Decisions to follow: ADR 0001 (Smithay), 0003 (IPC), 0004 (config), 0005 (global menu).

- [x] **1.1 Compositor skeleton on winit** · Opus. calloop loop, Smithay state, compositor,
      shm, linux-dmabuf, seat, output, xdg-shell; winit backend; GLES renderer with damage
      tracking; new windows placed in a stack, click to focus; Super+Q quits.
      *Accept:* `foot` and `weston-terminal` show and take input nested under weston in the VM.
      *Done:* also has the core data device (foot needs it) and dmabuf v4 feedback; the
      winit backend is a dev-only cargo feature (ADR 0001 amendment 1). VM steps:
      `docs/checks/phase-1.md` §3.
- [x] **1.2 Headless backend + test harness** · Opus. Headless backend (EGL surfaceless when
      pixels are needed), a scripted test client on `wayland-client`, and a first test
      that maps an `xdg_toplevel` and asserts the window list. Runs in CI on Linux.
      *Accept:* the test passes in `cargo xtask check` on both Linux CI jobs.
      *Done:* in-process harness (`compositor/neue-comp/tests/harness/`) with socket-pair
      clients and `Server::with_state`; headless backend has no renderer yet, EGL moves to
      1.16 (ADR 0006). Linux checks need `libwayland-client` (dev-setup §3.1).
- [x] **1.3 `neue-ipc` + `neuectl`** · Sonnet. New portable crate `neue-ipc` and the
      `neuectl` binary per ADR 0003: `version`, `windows`, `outputs`, `focused-window`,
      `action`, `actions`, `subscribe` (window events), `quit`.
      *Accept:* headless test drives the compositor through the socket; `neue-ipc` tests
      pass on macOS.
      *Done:* wire details in ADR 0007. Actions so far: `quit`, `close-window`,
      `focus-window`; `spaces`, `reload-config` and `screenshot` wait for their tasks.
      The harness gives every test server an IPC socket (`Server::ipc()`).
- [x] **1.4 tty backend** · Opus. DRM/KMS atomic, GBM, libinput, udev, libseat; single
      output; VT switching; cursor plane.
      *Accept:* in the VM on DRM under seatd, `foot` runs and keyboard/pointer work
      (owner at the VM window confirms input).
      *Done:* cargo feature `tty`, now the default outside a Wayland session; built-in arrow
      cursor plus client cursor surfaces; plain `GlesRenderer`, no overlays yet (ADR 0001
      amendment 2). Verified in the VM with scripted input (Parallels key events, ydotool).
      Owner confirmed keyboard and pointer by hand in the VM. VM steps:
      `docs/checks/phase-1.md` §4.
- [x] **1.5 Floating window management** · Opus. Focus and stacking rules, interactive move
      and resize (xdg `move`/`resize`), window placement, minimise/zoom/close as actions,
      xdg-activation focus stealing rules.
      *Accept:* headless tests for focus/stacking; `neuectl action` covers each operation.
      *Done:* rules in ADR 0008 (dialogs stay above parents, focus returns to the parent,
      work area from layer-shell zones, placement centre/cascade/remember, zoom = xdg
      maximized, activation tokens only from the focused client, popup grabs). New
      actions `minimize-window`, `unminimize-window`, `zoom-window`, `move-window`,
      `resize-window`; `WindowInfo` gains `minimized`/`zoomed`/`demands_attention`/`parent`;
      new `window_changed` event. Headless tests in `tests/wm.rs` script pointer input via
      `State::move_pointer`/`press_button`. Owner checked move, resize, zoom and minimise
      by hand in the VM; unminimise only through `neuectl` (no dock yet).
- [x] **1.6 `neue-config` + compositor config and hot reload** · Sonnet. New portable crate
      `neue-config` with the `compositor.toml` schema (keyboard layouts and xkb options,
      pointer, keybindings, output scale), layering and the inotify reload per ADR 0004;
      `reload-config` and `config_error` in `neuectl`.
      *Accept:* changing the xkb layout in the file applies without restart (headless test
      plus VM check).
      *Done:* loader, schema and chord syntax in `neue-config`; details in ADR 0009.
      `compositor.toml` has `[keyboard]` (layouts, variants, model, options, repeat),
      `[pointer]` (natural scroll, scroll factor), `[keybindings]` (default `Super+Q` =
      `quit`; `"none"` unbinds) and `[outputs."NAME"] scale`. New `neuectl reload-config`,
      `reload-config` action and `config_error`/`config_reloaded` events (topic `config`).
      Headless tests in `tests/config.rs` (layout switch, rejected files, bindings). Owner
      confirmed the hot reload by hand in the VM (`docs/checks/phase-1.md` §5). Not done: libinput pointer
      settings (acceleration, tap-to-click) need tty device handling and hardware, so they
      wait for a later task; real output scaling is 1.7.
- [x] **1.7 Multi-monitor and scaling** · Opus. Output layout from config, hotplug,
      fractional-scale-v1, viewporter, per-output scale, windows moving between outputs.
      *Accept:* headless test with two outputs at scales 1 and 1.5; VM hotplug by resizing
      the Parallels window (owner).
      *Done:* rules in ADR 0010. New `outputs.rs` owns globals,
      layout (`[outputs."NAME"] position`, else left to right), scales, window rescue and
      the new `outputs` IPC topic (`output_added`/`_changed`/`_removed`); surfaces get the
      scale of the output they overlap most. tty lights every connector on its own CRTC
      and rescans on udev change events (a resized VM window is a modeset). Headless
      backend has `State::headless_add_output`/`_remove_output`/`_set_mode`; tests in
      `tests/outputs.rs`. Checked in the VM: tty start-up and a live scale 1.5 reload with
      `foot`; the owner resized the Parallels window three times and got one
      `output_changed` per resize with the new mode. A second display can't be tested in
      Parallels, so plugging and unplugging is covered only by the headless tests and
      waits for real hardware. Not done: mode/transform/enable settings, EDID names, several GPUs (later task, see 1.15).
- [x] **1.8 Clipboard and DnD** · Sonnet. Data device (core part exists since 1.1), primary selection, ext-data-control,
      DnD with icon surfaces.
      *Accept:* `wl-copy`/`wl-paste` round trip in a headless test; DnD of a file between two
      GTK apps in the VM.
      *Done:* rules in ADR 0011. Primary selection, `ext-data-control` and also
      `wlr-data-control` (what `wl-clipboard` uses today), drag icons drawn on tty and winit
      with offsets following commits. Headless tests in `tests/selection.rs` cover clipboard,
      primary selection and data-control round trips (the `wl-copy`/`wl-paste` wire path),
      selection replacement, a full DnD drag between two clients with an icon, and Escape
      cancelling a drag (the compositor takes the key; Smithay doesn't end the grab when a
      client destroys its source). Owner checked clipboard, primary selection and file DnD
      between GTK apps (nautilus, gedit, gnome-text-editor) by hand in the VM
      (`docs/checks/phase-1.md` §7). Not done: touch DnD.
- [x] **1.9 Cursors** · Sonnet. XCursor theme loading (pure-Rust `xcursor` crate),
      cursor-shape-v1, per-scale cursor sizes, hardware cursor plane on tty.
      *Accept:* correct cursors on resize edges and text fields in the VM.
      *Done:* rules in ADR 0012. `xcursor` themes from `XCURSOR_THEME`/`XCURSOR_SIZE`
      (config key is a follow-up), nearest nominal size at `size × output scale`, animated
      cursors, built-in arrow as fallback at any size; `cursor-shape-v1`; resize grabs force
      the edge's resize cursor (`State::set_cursor_override`); the winit backend sets the
      host window's cursor. The cursor plane was already used since 1.4. Headless tests in
      `tests/cursor.rs` (shape requests, stale serials, resize override) and unit tests in
      `cursor.rs`. Owner checked cursors by hand in the VM (`docs/checks/phase-1.md` §8) and found them fine. Not done:
      cursor theme artwork (Phase 3), move-grab cursor, tablet cursors.
- [x] **1.10 Server-side title bars** · Opus. xdg-decoration and kde server-decoration;
      title bar and traffic lights drawn with `neue-paint` on `vello_cpu`, nine-sliced and
      cached per state/scale, uploaded via `MemoryRenderBuffer`; title text through parley
      with a bundled placeholder font. Touches `neue-paint` too.
      *Accept:* golden tests for the title bar (focused/unfocused × light/dark × 1x/2x);
      a GTK3 app shows our chrome in the VM; title bar render time logged.
      *Done:* rules in ADR 0013. Server-side by default, client-side when a client asks;
      the bar is 22 px above the client geometry and placement/zoom/moves keep it on
      screen. Three slices (left with buttons, 1 px middle, right) per style and scale,
      the title per window; `render::output_elements` stacks each bar with its window in
      both backends. `neue-paint` gained `text` (parley without system fonts, Fira Sans
      Regular under the OFL as placeholder), `Command::Glyphs` and
      `materials::title_bar`; goldens `titlebar-*`. Headless tests in
      `tests/decorations.rs`. Checked in the VM on the tty with `prlctl capture`: mousepad
      (GTK3, KDE protocol) and foot (xdg-decoration) show the bar, focus changes its
      style, zoom fits the client below it; cold slice render ≈0.5 ms, a title 0.05–0.4 ms
      (debug log). Owner checked the bars by hand in the VM (`docs/checks/phase-1.md` §9) and
      found them fine. Not done: appearance setting (light only), scale-2 check in the VM,
      interaction (done in 1.11).
- [x] **1.11 Chrome interaction** · Sonnet. Traffic-light hover/press and actions, drag to
      move, double-click to zoom/minimise (per config), invisible resize edges.
      *Accept:* headless tests on hit-testing; VM check by the owner.
      *Done:* rules in ADR 0014. `interaction::hit_test` (pure, unit-tested) and
      `State::chrome_hit_at` find lights, bar and 5 px edges outside the frame; chrome has no
      client pointer focus. Left press starts `TrafficLightGrab` (pressed look, action on
      release over the button), `TitleBarGrab` (move after 3 px, double click otherwise) or
      the existing `ResizeGrab`; resize cursors on hover. `[windows] title-bar-double-click`
      (`zoom`/`minimize`/`none`) in `compositor.toml`. `neue-paint`: `TitleBarStyle` gains
      `hover` (glyphs) and `pressed`; goldens `titlebar-{hover-*,pressed-*}`. Headless tests
      in `tests/chrome.rs`. Owner checked it by hand in the VM (`docs/checks/phase-1.md` §10) and found it fine.
      Not done: touch and tablet input on chrome, hover refresh without pointer motion,
      keyboard-driven resize.
- [x] **1.12 Shell-facing protocols** · Sonnet. wlr-layer-shell, ext-session-lock,
      ext-idle-notify, idle-inhibit, ext-foreign-toplevel-list, xdg-foreign, xdg-dialog-v1
      (modal flag only; sheets come later), xdg-toplevel-icon, xdg-system-bell,
      keyboard-shortcuts-inhibit.
      *Accept:* a headless test per protocol binding; `swaylock` or another ext-session-lock
      client locks and unlocks in the VM.
      *Done:* rules in ADR 0015. `layer.rs` (layer shell: layout through Smithay's
      `LayerMap`, exclusive zones refit zoomed windows, exclusive/on-demand keyboard focus,
      hit-testing and drawing above and below windows), `lock.rs` (session lock: lock surfaces
      only, input and bindings off, survives a dead locker) and `shell_protocols.rs` (idle
      notify and inhibit, foreign toplevel list with `window-<id>` identifiers, xdg-foreign,
      xdg-dialog modal flag, toplevel icon, system bell, shortcuts inhibit). 20 headless
      tests in `tests/shell_protocols.rs` on a new `shell.rs` test-client module.
      Owner checked it by hand in the VM (`docs/checks/phase-1.md` §11: `swaylock`, `waybar`,
      `fuzzel`, `swaybg`, `swayidle`) and found it fine. Not done: idle inhibit by
      visibility, layers of a removed output are closed not moved, per-output exclusive
      focus, lock surface reconfigure on mode change.
- [x] **1.13 Input protocols** · Sonnet. pointer-constraints, relative-pointer,
      pointer-gestures, text-input-v3, input-method-v2, touchpad gestures config.
      *Accept:* headless tests; an IME (e.g. fcitx5) commits text into a GTK app in the VM.
      *Done:* rules in ADR 0016. `input_protocols.rs` (constraints follow keyboard focus,
      lock/confine in `move_pointer_by` and `move_pointer_towards`, relative motion,
      gestures via `State::gesture`, input method popups), `[touchpad]` in `compositor.toml`
      applied through libinput in `tty.rs`. 15 headless tests in `tests/input_protocols.rs`.
      Owner checked it by hand in the VM (`docs/checks/phase-1.md` §12) and found it fine.
      Not done: gestures, touchpad config and pointer lock on real hardware, escape hatch
      from a lock, popup constraining.
- [x] **1.14 Presentation and sync** · Opus. linux-drm-syncobj-v1 (explicit sync),
      presentation-time, tearing-control, security-context-v1 filtering of privileged globals.
      *Accept:* headless tests; explicit sync exercised on the NVIDIA PC (owner, see 1.18).
      *Done:* rules in ADR 0017. `presentation.rs` (feedback carried with the DRM frame and
      presented at vblank with the kernel timestamp; winit on submit; headless on its timer;
      tearing-control implemented by us, hints recorded but not acted on: Smithay has no
      async flips yet), `sync.rs` (commits wait on the syncobj acquire point or the dmabuf's
      implicit fences; syncobj global only on tty with `syncobj_eventfd`), `security.rs`
      (sandboxed clients lose session lock, layer shell, data control, input method,
      foreign toplevel list and the security-context manager). 9 headless tests in
      `tests/presentation.rs`. Owner checked it by hand in the VM (`docs/checks/phase-1.md` §13,
      steps 1–3) and found it fine. **Still to do:** explicit sync on the NVIDIA PC (`docs/checks/phase-1.md` §13
      step 4). Not done: tearing, explicit sync untested on hardware,
      per-app sandbox policy.
- [x] **1.15 Direct scanout and VRR** · Opus. Direct scanout for full-screen surfaces,
      adaptive sync per output (config), damage review. Split in three (owner agreed);
      several GPUs are not part of it (see the 1.7 notes) and get their own task later.
  - [x] **1.15a Full-screen windows** · Opus. `xdg_toplevel` set/unset fullscreen: the
        window fills its output (or the one it asks for) without a title bar, above the
        `top` layer, ignoring exclusive zones; unset restores its place and size; it
        follows its output on unplug and mode changes. A stopgap until full-screen Spaces
        (Phase 4), ADR.
        *Accept:* headless tests (configure size, restore, stacking, unplug); `mpv --fs`
        in the VM (owner).
        *Done:* rules in ADR 0018. `fullscreen.rs` (state in `WmData`, output choice,
        centring on a black backdrop, refit on mode/scale/unplug, the "covered output" rule
        that drops the `top` layer below the windows unless another app's window is on
        top), `has_title_bar` hides the bar, `fullscreen` in `WindowInfo` and the
        `fullscreen-window` action. 8 headless tests in `tests/fullscreen.rs`. VM check
        (`docs/checks/phase-1.md` §14) still to do by the owner. Not done: exclusive `top`
        keyboard surfaces under a full-screen window, animation, menu-bar reveal.
  - [x] **1.15b Draw only when needed (damage review)** · Opus. tty stops polling at the
        refresh rate: outputs are redrawn on commits, input, chrome, layer, lock and config
        changes; frame callbacks of empty frames are timed from an estimated vblank;
        animated cursors get their own timer.
        *Accept:* an idle desktop renders nothing (trace counter); no visual regressions in
        the VM checklist (owner).
        *Done:* rules in ADR 0019. `redraw.rs` (per-output state: idle, scheduled, waiting
        for a vblank or an estimated one; unit tests), driven by `tty.rs`;
        `State::queue_redraw_all` on Wayland client traffic (one hook covers commits,
        destroys, titles, layers and lock), commits, input, `neuectl` traffic, config
        apply, hotplug and session resume; empty frames send their callbacks at the next
        refresh boundary after the last vblank; animated cursors arm a per-output timer.
        Each render logs a `trace` event with running counts. VM check
        (`docs/checks/phase-1.md` §15) still to do by the owner. Not done: per-output redraw
        requests (all outputs are marked), winit still redraws continuously.
  - [x] **1.15c Direct scanout and VRR** · Opus. Primary-plane scanout of full-screen
        surfaces (overlay planes stay off, NVIDIA), scanout tranche in per-surface dmabuf
        feedback, `[outputs."NAME"] adaptive-sync = "off" | "on" | "fullscreen"` (default
        `off`) checked against the connector, shown in `neuectl outputs`.
        *Accept:* config tests headless; scanout and VRR confirmed on real x86_64 hardware
        (owner, steps in `docs/checks/phase-1.md`); flagged in the PR.
        *Done:* rules in ADR 0020. `scanout.rs` (frame flags, per-output render and
        scanout feedback, sent after each render), `vrr.rs` (wanted state from mode,
        connector support and the full-screen rule; unit tests), staged with `use_vrr` before
        each tty render; presentation feedback reports a variable refresh while active.
        `OutputInfo.adaptive_sync { mode, supported, active }` (additive), `neuectl outputs`
        shows `vrr …`. Config parse tests, a headless reload test, neuectl format test.
        Hardware check (`docs/checks/phase-1.md` §16) still to do by the owner. Not done:
        overlay planes, rotated outputs, tearing, several GPUs.
- [x] **1.16 Screencopy** · Split into four PRs (a to d, in order). ext-image-copy-capture
      (output and toplevel sources), `neuectl screenshot`.
      *Accept (whole task):* `neuectl screenshot` PNG matches a headless golden; `grim`
      works in the VM.
  - [x] **1.16a Headless GLES renderer** · Opus. Optional GLES renderer for the headless
        backend on EGL surfaceless/device (ADR 0006); CI installs Mesa (software GL). Renders
        a headless output to an offscreen buffer.
        *Accept:* a test with a scripted client's shm window reads back the expected pixels;
        passes `cargo xtask check` on both Linux CI jobs.
        *Done:* `Options::headless_renderer` gives the headless backend a GLES renderer on
        Mesa's software EGL device only (no surfaceless fallback, no GPU), so pixels match
        everywhere (ADR 0021). `State::headless_render` draws an output on demand into a
        per-output renderbuffer with damage tracking and returns ARGB8888 pixels plus
        damage; no pointer or drag icon yet. `tests/render.rs`.
  - [x] **1.16b ext-image-copy-capture, output sources** · Sonnet (Opus if the renderer
        interface needs changes). `ext-image-capture-source` (output) and
        `ext-image-copy-capture`: shm and dmabuf buffers, damage tracking. Check that the
        pinned Smithay rev has the helpers before relying on them; ADR for the decision.
        *Accept:* a scripted client captures an output and the pixels match a golden.
        *Done:* Smithay's helpers drive the protocol; `screencopy.rs` draws each session
        into its own `OffscreenOutput` (moved from headless into `render.rs`) and copies
        into shm (readback) or dmabuf (blit, tty only). Frames wait for damage and
        report damage since the session's last frame; `paint_cursors` works on tty
        (`tty::pointer_elements`, shared `render::ScreenElement`); privileged globals
        (ADR 0022). `tests/screencopy.rs`, golden `tests/golden.rs` (neue-comp joins
        `GOLDEN_CRATES`). Not done: cursor sessions, dmabuf on winit/headless.
  - [x] **1.16c Toplevel sources** · Sonnet. `ext-foreign-toplevel-list` and the toplevel
        capture source (offscreen render of one window).
        *Accept:* a scripted client captures one window and the pixels match a golden.
        *Done:* the foreign toplevel list was already there (1.12). Smithay's toplevel
        source global (unrestricted clients only); sessions now have a `Subject` (output
        or window) and a window capture draws its surfaces on a transparent background
        at the geometry size (no title bar or cursor); resize sends new constraints,
        closing the window stops the session, minimised windows work (ADR 0023).
        `tests/screencopy.rs`, golden `screencopy-toplevel`.
  - [x] **1.16d `neuectl screenshot` + VM check** · Sonnet. IPC request that captures an
        output and encodes a PNG (ADR 0003 amendment); `grim` steps in
        `docs/checks/phase-1.md` §17, run by the owner, covering what CI can't: dmabuf captures
        and `grim -c` (cursor) on the tty backend (ADR 0022).
        *Accept:* PNG from `neuectl screenshot <path>` matches a headless golden; `grim`
        works in the VM (owner).
        *Done:* `screenshot` request `{path, output?, cursor?}` (ADR 0024, amends ADR 0003):
        the event loop draws the output with the code screencopy uses and reads it back, a
        thread encodes (`Pixmap::into_png`, no new dependency) and writes the file, and the
        reply comes through a calloop channel. A connection with a screenshot in flight
        answers nothing else until then, so replies stay ordered. New codes
        `unknown_output`, `capture_failed`. `neuectl screenshot PATH [--output] [--cursor]`.
        `tests/golden.rs` (`neuectl_screenshot`, same golden as 1.16b) and `tests/ipc.rs`.
        **Owner:** run `docs/checks/phase-1.md` §17 in the VM (grim, `grim -c`, dmabuf with
        `wl-mirror`); not done by the agent.
- [x] **1.17 Global-menu protocol side** · Sonnet. `org_kde_kwin_appmenu` v2 and `gtk_shell1`
      v5 per ADR 0005 (`gtk_shell1` XML in a new `neue-protocols` crate); menu addresses in
      `neuectl windows`.
      *Accept:* Kate (with a test registrar) and gnome-text-editor show addresses in
      `neuectl windows` in the VM.
      *Done:* ADR 0025. New `neue-protocols` crate (GTK's `gtk-shell.xml` vendored, `server`
      and `client` features); `appmenu.rs` in `neue-comp` serves both globals, validates bus
      names and object paths, and keeps one address per toplevel. `WindowInfo.menu`
      (`dbus_menu` or `gmenu`, amends ADR 0003), `window_changed` on change, `neuectl
      windows` prints it. `gtk_shell1.capabilities` is 0 until the Phase 3 menu bar shows
      the menus (GTK would otherwise hide its own). `tests/appmenu.rs`.
      **Owner:** ran `docs/checks/phase-1.md` §18 in the VM: Kate `[menu dbusmenu :1.11
      /MenuBar/2]`, GNOME Text Editor `[menu gmenu :1.1 app=/org/gnome/TextEditor
      win=/org/gnome/TextEditor/window/1]` (no `menubar=`: it only has a primary menu).
- [x] **1.18 xwayland-satellite on demand** · Sonnet. Reserve X display sockets, spawn
      `xwayland-satellite` on first connection, export `DISPLAY` (ADR 0001).
      *Accept:* `xeyes` (or another X11 app) appears in the VM; nothing X-related runs before.
      *Done:* ADR 0026. `xwayland.rs` reserves the lowest free display (lock file, filesystem
      and abstract socket), watches both sockets on the loop, spawns `xwayland-satellite :N
      -listenfd A -listenfd B` on the first connection and watches again when it exits
      (after 1 s if it died young). `--command` children get `DISPLAY`; no satellite on
      `PATH` means no display and a warning. Options `--no-xwayland` and
      `--xwayland-satellite <path>`. 4 headless tests in `tests/xwayland.rs` with a shell
      script as a stand-in. **Owner:** ran `docs/checks/phase-1.md` §19 steps 1 and 2 in the VM with
      satellite 0.8.3: nothing X-related before the first client, then `xeyes` appears.
      **Still to do:** §19 steps 3 (restart) and 4 (clean exit); the compositor does not set its own `DISPLAY`, so task 1.19 must import it into
      the session. Not done: a `compositor.toml` switch.
- [x] **1.19 `neue-session` and systemd units** · Sonnet. New `session/neue-session` crate:
      environment import, `graphical-session.target`, units for `neue-comp`, and a greetd /
      wayland-sessions entry.
      *Accept:* the session starts from greetd in the VM and logs out cleanly.
      *Done:* ADR 0027. `neue-session` (std only) sets the session environment in the user
      manager and runs `systemctl --user start --wait neue-session.target`; units
      `neue-session.target` and `neue-comp.service` (`Type=notify`); `neue-comp --notify`
      imports `WAYLAND_DISPLAY`/`DISPLAY` and sends `READY=1`; `wayland-sessions` entry and a
      VM `greetd` config in `session/`. Unit tests with a fake `systemctl`. **Owner:** ran
      `docs/checks/phase-1.md` §20 steps 1 to 5 in the VM from greetd: all pass (login, environment,
      second start refused, clean logout).
      Not done: a SIGTERM handler (needs `nix`, so `ExecStop=neuectl quit` instead).
- [x] **1.20 First PKGBUILD + `cargo xtask package`** · Sonnet. PKGBUILD for `neue-comp`,
      `neuectl` and `neue-session` with vendored git deps; `xtask package <crate>`.
      *Accept:* package builds in the OrbStack amd64 and arm64 machines and installs in the VM.
      *Done:* ADR 0028. Split PKGBUILD `distro/pkgbuilds/neue-comp` (three packages, release
      build without winit); `cargo xtask package <crate> [--arch aarch64|x86_64|all]` packages
      the working tree and runs `makepkg` in the OrbStack machines. Both architectures build.
      **Diverged:** git deps are fetched by `cargo fetch --locked`, not vendored (ADR 0028).
      **Owner:** installed the packages in the VM (`docs/checks/phase-1.md` §21): passes.
- [x] **1.21 Daily-drive pass** · Opus with the owner. foot/Alacritty, Firefox, a Qt app, a
      Flatpak, an X11 app on the NVIDIA PC and the VM; fix or file bugs. (No AMD or Intel
      PC is available, so that column was dropped.)
      *Accept:* the Phase 1 exit criterion below, with a results table in
      `docs/research/phase1-daily-drive.md`.
      *Done:* the VM (checked over ssh with `neuectl`, input by hand by the owner) and the
      NVIDIA PC (RTX 2080, `nvidia-open`, run by the owner) pass. Three bugs were fixed:
      `neue-comp` sends `xdg_toplevel.configure_bounds` (B1), `ExecStop=-neuectl quit` (B2),
      and Smithay is pinned to a fork commit that stops the popup log spam (B5, ADR 0029).
      Open: B3 and B6 (cosmetic, at exit). Not covered: HiDPI, multi-monitor, a full day
      of use.
- **Exit:** daily-drivable with third-party apps (foot/Alacritty, Firefox, Flatpaks) on the
  PC and the VM.
