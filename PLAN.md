# neueOS — Project Plan

> A personal Linux desktop environment and Arch-based distribution that reinterprets the
> skeuomorphic design of OS X Mountain Lion with modern technology. Every first-party
> component is written in Rust.

Status: **planning** · Last updated: 2026-11-03 · Licence: GPL-3.0-or-later

---

## 1. Goals and non-goals

### Goals
- A complete desktop environment: Wayland compositor, menu bar with a global app menu, dock,
  Mission Control (Exposé + Spaces), Launchpad, Search (Spotlight-style), Notification Center,
  Dashboard, a file manager with Glance (QuickLook-style previews), and System Preferences.
- A full distribution: a pacman repository, a bootable ISO with a graphical installer, and a
  first-boot setup assistant. The installer detects AMD, Intel, NVIDIA and hybrid GPUs.
- Modern stack only: Wayland, PipeWire/WirePlumber, Flatpak (Flathub), systemd, Btrfs.
- **Design direction: "Mountain Lion soul, modern tech".** Linen, brushed and gradient
  chrome, glossy Aqua controls and the 3D glass dock shelf, rebuilt vector-first for HiDPI.
  Light and dark variants, accent colours, real-time blur, and smooth animations at 120 Hz
  and above.
- Runs on x86_64 (release target, real PCs) and aarch64 (development target, Parallels VM
  on the Mac), built from one codebase.

### Non-goals
- No X11 or Xorg code in neueOS. X11 apps run through **xwayland-satellite**, a separate
  Rust process that bridges them to Wayland. The compositor contains no X11 window manager.
- No Snap. No PulseAudio daemon (pipewire-pulse provides compatibility).
- No Apple assets, fonts, sounds or trademarked names. All artwork is original and only
  inspired by Mountain Lion.
- Not a general-purpose distro for other people. It is tuned for the owner's machines, but
  built cleanly enough that this could change later.

### What "written in Rust" means
- **First-party code** (compositor, shell, toolkit, apps, services, installer, tooling) is
  100% Rust. The only exceptions are shaders (WGSL/GLSL), protocol XML, and themes for
  third-party toolkits (GTK CSS, Kvantum SVG).
- **System dependencies** come from Arch and may be C/C++: kernel, Mesa, libinput, PipeWire,
  systemd, PAM, GStreamer, libalpm, libflatpak. We use them through safe Rust bindings.
- **Where a mature Rust option exists, the base system uses it:** greetd (login daemon),
  sudo-rs, xwayland-satellite, oo7 (Secret Service), zram-generator, fish shell, and
  alacritty_terminal (terminal core).

---

## 2. Decisions so far

| Topic | Decision |
|---|---|
| Compositor | Our own compositor, built on **Smithay** |
| UI toolkit | Our own toolkit, **neue-kit**, built on wgpu + **vello** and other Linebender crates |
| OS scope | Full distro: pacman repo, archiso ISO, Rust GUI installer, setup assistant |
| GPUs | Support AMD, Intel, NVIDIA and hybrid; the installer detects them and installs the right drivers |
| Shell features | Dock, Mission Control/Exposé, Spaces, menu bar, Launchpad, Search, Notification Center, Dashboard |
| First-party apps | Files, Glance, Preferences, Terminal, TextEdit, Preview, Software Center, Installer, Setup Assistant, Greeter |
| Global menu for apps that don't export one | **Fallback menu** (App/File/Edit/Window/Help backed by generic actions) |
| Third-party theming | Best effort: GTK3/GTK4 CSS, Kvantum (Qt), icons, cursors, sounds, Flatpak overrides |
| Disk and boot | Btrfs subvolumes, automatic pre-update snapshots, systemd-boot with UKI, optional LUKS2 |
| X11 apps | xwayland-satellite, started on demand, with no X11 code in neueOS itself |
| Naming | Our own names (working names are listed below and finalised in a naming pass) |
| Licence | GPL-3.0-or-later |
| Mac testing | **Parallels** (already installed) for graphical VMs, **OrbStack** (already installed) for headless builds |

### Working names (to be finalised)
These are user-facing names. Crates use the `neue-` prefix.

| Mountain Lion concept | neueOS working name | Crate |
|---|---|---|
| Window server | — | `neue-comp` |
| Finder | Files | `neue-files` |
| QuickLook | Glance | `neue-glance` |
| Mission Control / Exposé | Overview | in `neue-comp` |
| Spotlight | Search | `neue-search` (UI) + `neue-indexd` |
| Launchpad | Launcher | in `neue-shell` |
| Notification Center | Notifications | in `neue-shell` |
| Dashboard | Widgets | in `neue-shell` |
| System Preferences | Preferences | `neue-prefs` |
| Terminal | Terminal | `neue-term` |
| TextEdit | Write | `neue-write` |
| Preview | Viewer | `neue-viewer` |
| App Store | Software | `neue-software` |

---

## 3. Architecture

```
                         ┌───────────────────────────────────────────────┐
  greetd ─► neue-greeter │                 neue-session                  │ systemd --user units
                         │  (starts compositor, shell, services; graph.  │ graphical-session.target
                         │   session target, env import)                 │
                         └───────────────┬───────────────────────────────┘
                                         │
┌────────────────────────────────────────▼──────────────────────────────────────────┐
│ neue-comp (Smithay)                                                               │
│  DRM/KMS + GBM + libinput + udev + libseat   │  winit backend (nested, for dev)   │
│  GLES renderer: damage tracking, direct scanout, VRR, explicit sync               │
│  Window management: floating/stacking, Spaces, full-screen Spaces, Overview,      │
│  App Exposé, window animations (genie/scale, sheets), server-side ML chrome       │
│  Protocols: xdg-shell, layer-shell, xdg-decoration, xdg-dialog (sheets),          │
│  kde-appmenu, foreign-toplevel, ext-workspace, screencopy, session-lock, ...      │
│  Private: neue-shell-v1 (dock hints, minimize targets, app state)                 │
│  IPC: Unix socket + JSON (`neuectl`)                                              │
└──────┬──────────────────────┬──────────────────────────┬──────────────────────────┘
       │ Wayland              │ Wayland                  │ spawns on demand
┌──────▼───────────┐  ┌───────▼────────────────────┐  ┌──▼─────────────────┐
│ neue-shell       │  │ Apps (neue-kit)            │  │ xwayland-satellite │
│ menubar, dock,   │  │ Files, Prefs, Terminal,    │  └────────────────────┘
│ launcher, search │  │ Write, Viewer, Software…   │
│ UI, notif. center│  │ 3rd-party GTK/Qt/Flatpak   │
│ widgets, OSDs,   │  └────────────────────────────┘
│ tray (SNI)       │
└──────┬───────────┘
       │ D-Bus (zbus)
┌──────▼────────────────────────────────────────────────────────────────────────────┐
│ Services: neue-settingsd · neue-indexd · neue-glance · xdg-desktop-portal-neue ·  │
│ neue-polkit-agent · neue-updated (privileged, polkit) · oo7-daemon ·              │
│ system: PipeWire/WirePlumber, NetworkManager, BlueZ, UPower, UDisks2, logind,     │
│ power-profiles-daemon, fwupd, Flatpak                                             │
└───────────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Compositor (`neue-comp`)
- **Base:** Smithay, with calloop as the single-threaded event loop. Smithay's GLES renderer
  draws the scene, and our own GLES shaders add blur, shadows, linen and the Overview
  effects. Smithay's `MultiRenderer` handles hybrid and multi-GPU setups.
- **Backends:**
  - `tty`: DRM/KMS, GBM, libinput, udev and libseat. Used on real hardware and in the VM
    console.
  - `winit`: runs nested inside another Wayland session for fast development.
  - `headless`: used by tests.
- **Window model:** floating and stacking, like OS X. Spaces span each display; per-display
  Spaces and a menu bar on every display are on by default and configurable. Full-screen
  windows get their own Space. Minimise uses a genie or scale animation toward the window's
  dock target. Modal dialogs (`xdg-dialog-v1`) become **sheets** that slide out of the
  parent window's title bar.
- **Server-side chrome:** the compositor draws Mountain Lion title bars with traffic-light
  buttons for any client that accepts server-side decorations (`xdg-decoration`). neue-kit
  apps draw their own unified toolbars client-side, using the same `neue-paint` code so
  both look identical. Chrome is drawn on the CPU with `vello_cpu` into cached textures,
  which is cheap because it only changes when window state changes.
- **Overview (Mission Control):** live window thumbnails grouped by app, a Spaces strip
  along the top, drag-and-drop between Spaces, App Exposé, Show Desktop, hot corners,
  three- and four-finger touchpad gestures, and a ⌘-Tab style app switcher.
- **Wayland protocols:** xdg-shell, xdg-decoration, wlr-layer-shell, xdg-activation,
  fractional-scale-v1, viewporter, linux-dmabuf, linux-drm-syncobj-v1 (explicit sync, needed
  for NVIDIA), presentation-time, tearing-control, cursor-shape-v1, pointer-constraints,
  relative-pointer, pointer-gestures, text-input-v3 and input-method-v2, primary-selection,
  ext-data-control, ext-session-lock, ext-idle-notify, idle-inhibit, ext-foreign-toplevel-list,
  ext-workspace, ext-image-copy-capture, xdg-foreign, xdg-dialog-v1, xdg-toplevel-icon,
  xdg-system-bell, keyboard-shortcuts-inhibit, security-context-v1, kde appmenu,
  `gtk_shell1` (GTK menu export only, ADR 0005), and kde server-decoration (for older Qt).
  Colour management and HDR come later.
- **IPC:** `neuectl` speaks JSON over a Unix socket. It covers window, Space and output
  queries, actions, config reload, screenshots and event streams. It is also the hook that
  tests and coding agents use. Protocol and channel boundaries: ADR 0003.
- **Config:** TOML in `~/.config/neueos/`, hot-reloaded. Preferences writes it through
  `neue-settingsd`. File layout, layering and reload: ADR 0004.
- **Library:** Smithay from a pinned git rev, GLES renderer, no X11 features: ADR 0001.

### 3.2 Toolkit (`neue-kit` and friends)
| Crate | Responsibility |
|---|---|
| `neue-paint` | Its own recorded `Scene` (kurbo, peniko), **materials** (linen, metal, aqua glass, HUD), nine-slice, procedural textures. Replayed per backend: GPU (`vello_gpu` on wgpu), CPU (`vello_cpu`, for compositor chrome and golden tests), and Masonry's `imaging` painter (ADR 0002) |
| `neue-theme` | Design tokens: colours, metrics, typography, light and dark, accent colour, materials table |
| `neue-platform` | Windowing backends. **Wayland** (smithay-client-toolkit: xdg-shell, layer-shell, fractional scale, text-input-v3, DnD, clipboard, xdg-activation, xdg-dialog). **winit** (macOS and dev, so apps and widget galleries run natively on the Mac) |
| `neue-kit` | Widget tree on **`masonry_core`** (pinned git rev, hidden behind neue-kit's own API; ADR 0002), Masonry's layout, events, focus, animation (our springs on `on_anim_frame`), text through **parley** and fontique, accessibility through **AccessKit**, menus with DBusMenu export, an Elm-style app model, and async integration |
| `neue-desktop` | freedesktop specs: desktop entries, icon themes, MIME (shared-mime-info), thumbnails, trash, recent files, autostart |
| `neue-dbus` | zbus interfaces: DBusMenu, StatusNotifierItem/Watcher, Notifications, our private services |

- **Masonry decision (ADR 0002):** build on `masonry_core` with our own sctk and winit
  drivers in `neue-platform`. The first Phase 2 task is a gate: if Masonry's pins, `imaging`
  and our `vello_cpu`/`vello_gpu` pins can't be reconciled, or popups can't map to
  xdg-popup, fall back to a thin tree of our own on the same crates.
- Widget set (v1): push button (Aqua gel, default-pulse), bevel/recessed buttons, checkbox,
  radio, popup/pull-down, combo, segmented control, slider, stepper, text field, search
  field, secure field, text view, scroll view with overlay scrollbars, table and outline
  view, source-list sidebar, split view, tab view, unified toolbar, sheet, popover, HUD
  panel, progress bar and indeterminate barber pole, spinner, colour well, date picker,
  path control, and an icon/collection view.

### 3.3 Shell (`neue-shell`)
A single process that owns several layer-shell surfaces and shares one set of state:
the app registry, icon cache, settings and the window list from `neue-shell-v1`. If it
crashes, the compositor restarts it.
- **Menu bar:** a translucent gradient bar. Left side: the logo menu, the app's bold name,
  then the app's menus. Right side: menu extras (clock, volume, Wi-Fi/network, Bluetooth,
  battery, input source, user), the SNI tray, the Search magnifier, and the Notifications
  button.
- **Global menu pipeline:**
  1. The window maps to a DBusMenu service through the KDE appmenu Wayland protocol.
  2. The shell renders that menu.
  3. Sources (ADR 0005): neue-kit apps, Qt6 apps and Chromium via KDE appmenu + DBusMenu
     (needs our `com.canonical.AppMenu.Registrar`), and GTK3/GTK4 `GtkApplication` apps via
     `gtk_shell1` + `org.gtk.Menus`. The shell owns the registrar and both D-Bus clients;
     the compositor only stores per-window addresses.
  4. Every other app gets the **fallback menu**:
     - App menu: About, Hide, Quit.
     - File: New Window, Close.
     - Edit: Undo, Cut, Copy, Paste, Select All. These are sent to the app as synthesized
       key chords; the compositor owns input, so it can inject them.
     - Window: Minimise, Zoom, Bring All to Front.
     - Help: searches the menus, like Mountain Lion's Help search.
- **Dock:** the 3D glass shelf, with reflections in it, plus a 2D mode for the left and
  right edges. Includes magnification, running-app indicators, bounce on launch and
  attention, minimised-window tiles, Stacks (folders as fan or grid), Trash, drag to
  add/remove, and per-app context menus with window lists.
- **Notifications:** an `org.freedesktop.Notifications` server, banners and alerts, a linen
  slide-out Notification Center that pushes the desktop aside (as in Mountain Lion), and
  Do Not Disturb.
- **Launcher (Launchpad):** a full-screen icon grid with blurred backdrop, pages, folders
  and drag to rearrange.
- **Search UI (Spotlight):** a menu-bar search field with categorised results (apps, files,
  documents, folders, calculator, unit conversion, settings panes) and a Glance preview.
- **Widgets (Dashboard):** its own Space, or an overlay. Built-in Rust widgets: clock,
  weather, calculator, calendar, notes, system monitor. A plugin API (likely WASM) comes
  later.
- **OSDs:** the Mountain Lion rounded-square HUD for volume, brightness and eject.

### 3.4 Services
| Service | Role |
|---|---|
| `neue-settingsd` | Settings source of truth on D-Bus. Writes compositor config, GTK `settings.ini` and dconf keys, Qt/Kvantum config, and the portal colour scheme |
| `xdg-desktop-portal-neue` | Portal backend: FileChooser (reuses Files components), Screenshot, ScreenCast (PipeWire), Settings, Access, Notification, Background, GlobalShortcuts, Inhibit, Account, Wallpaper. **Critical for Flatpak** |
| `neue-indexd` | Search indexer: tantivy full-text index, inotify/fanotify watching, text extractors (plain text, Markdown, PDF, ODF/OOXML, EXIF/ID3 metadata) |
| `neue-glance` | Glance D-Bus service. Also implements the GNOME `NautilusPreviewer` interface so third-party apps can trigger previews |
| `neue-polkit-agent` | Mountain Lion-style authentication dialog |
| `neue-updated` | Privileged helper behind polkit. Runs pacman (libalpm) and Flatpak transactions, takes a snapshot before every update, and handles fwupd |
| Base (external) | oo7-daemon (keyring), PipeWire + WirePlumber, NetworkManager, BlueZ (via `bluer`), UPower, UDisks2, power-profiles-daemon, switcheroo-control |

### 3.5 Apps
| App | Notes |
|---|---|
| **Files** | Icon, list, column and Cover Flow views. Sidebar (favourites, devices via UDisks2, network). Tabs, Get Info, tags (xattrs), Trash, archives, Glance on Space. Network shares are a later decision (GVfs over D-Bus vs. a Rust FUSE approach) |
| **Glance** | Previewer plugins: images (image, resvg, AVIF/HEIF), PDF (**hayro** pure-Rust renderer, with pdfium as fallback), text and code (syntect or tree-sitter), Markdown, audio and video (GStreamer via gstreamer-rs), fonts, archives (listing), folders. Shares thumbnailers with Files |
| **Preferences** | Pane grid, as in Mountain Lion. Panes: General, Desktop & Screen Saver, Dock, Mission Control, Spaces, Notifications, Displays, Sound, Network, Bluetooth, Keyboard, Trackpad/Mouse, Users & Groups, Software Update, Startup Disk, Sharing, Accessibility, Date & Time, Snapshots |
| **Terminal** | Built on `alacritty_terminal`. Profiles (Homebrew, Pro, Novel… original versions of these), tabs, splits |
| **Write** | Plain and rich text (Markdown and RTF-ish), built on parley's editing support |
| **Viewer** | Images and PDF: thumbnails sidebar, annotations (later), and export |
| **Software** | App Store-style front end. Flathub via AppStream (libflatpak bindings), pacman updates via `neue-updated`, firmware via fwupd |
| **Installer** | Runs in the live ISO session (see §5) |
| **Setup Assistant** | First boot: language, keyboard, Wi-Fi, user account, appearance, privacy |
| **Greeter / Lock** | Linen login window. The greeter runs under greetd inside `neue-comp --greeter`. The lock screen is an ext-session-lock client |

### 3.6 Third-party integration
- **Theming:**
  - GTK3: full CSS theme.
  - GTK4: an override via `~/.config/gtk-4.0/gtk.css`. libadwaita apps only partially
    follow it.
  - Qt5/6: a Kvantum theme (SVG assets) plus the plasma-integration platform theme, which
    also enables the global menu.
  - An icon theme, an XCursor theme and a freedesktop sound theme.
- **Flatpak:**
  - Flathub enabled by default.
  - The GTK theme ships as Flatpak extensions (`org.gtk.Gtk3theme.*`).
  - `flatpak override` gives apps read access to the GTK4 CSS.
  - Portals provide file dialogs.
- **Shortcuts:** see open question Q1, about Super acting as ⌘.

---

## 4. Design system
- **Materials, all procedural or vector so they scale to any DPI:**
  - Linen: Notification Center, login, Overview background.
  - Window chrome gradient: unified title bar and toolbar.
  - Aqua gel controls in blue and graphite.
  - Glass dock shelf.
  - Translucent menu bar.
  - Dark HUD.
  - Paper and leather textures for app-specific skeuomorphism (Write, Widgets).
- **Modern additions:**
  - A dark variant of every material.
  - An accent colour system.
  - Backdrop blur.
  - Spring-based motion tuned to Mountain Lion timings.
  - Reduced-motion and increased-contrast modes.
- **Typography:** choose an open humanist sans in the spirit of Lucida Grande (candidates:
  Fira Sans, Source Sans 3, IBM Plex Sans, Noto Sans). Monospace for Terminal: candidates
  are JetBrains Mono and Fira Code. See Q2.
- **Icons:** original, Mountain Lion-style detailed perspective icons in SVG, packaged as a
  freedesktop icon theme. Until coverage is complete, fall back to an existing theme
  (Papirus).
- **Deliverables:**
  - `docs/design/`: the token reference, materials spec and motion spec.
  - A **widget gallery app** that runs natively on macOS and Linux.
  - Golden-image tests for every widget, in every state, in light and dark.

---

## 5. The OS: distribution, installer, hardware

### 5.1 Base
- **x86_64:** official Arch Linux. This is the release target.
- **aarch64:** Arch Linux ARM. Used for the Parallels VM; a best-effort target built from
  the same PKGBUILDs.
- Kernel: `linux` (optionally `linux-zen`), plus systemd, PipeWire, WirePlumber,
  NetworkManager, Flatpak, greetd, sudo-rs, zram-generator, Btrfs tooling.

### 5.2 Packages and repo
- `distro/pkgbuilds/`: one PKGBUILD per component, plus meta-packages:
  - `neueos-desktop`: the full DE.
  - `neueos-base`: system defaults, configs and pacman hooks.
  - `neueos-gpu-{amd,intel,nvidia,hybrid}`.
- **Repo:** a signed pacman repo, `[neueos]`, for x86_64 and aarch64. Hosted as GitHub
  Releases assets or static hosting. Packages are signed with a dedicated GPG key.
- **Build machines:** OrbStack Arch machines (native arm64, plus amd64 under Rosetta) for
  local builds. GitHub Actions on x86_64 and arm64 runners for CI and releases.

### 5.3 ISO and installer
- **ISO:** an archiso profile in `distro/iso/` that boots a live neueOS session, which
  auto-starts the Installer. Built for x86_64 and aarch64.
- **Installer (Rust, neue-kit):**
  - Language, keyboard and disk selection.
  - Btrfs subvolumes: `@`, `@home`, `@snapshots`, `@var_log`, `@cache`.
  - Optional LUKS2.
  - Install via pacstrap.
  - systemd-boot with UKI.
  - Microcode detection.
  - **GPU detection** from PCI IDs in sysfs:
    - AMD: mesa, vulkan-radeon.
    - Intel: mesa, vulkan-intel, intel-media-driver.
    - NVIDIA Turing and newer: nvidia-open, nvidia-utils, with DRM modeset/fbdev enabled.
    - Older NVIDIA: warn, and offer nouveau/NVK or the legacy driver.
    - Hybrid: switcheroo-control, plus "Open on discrete GPU" in the Dock and Files
      context menus.
  - A **non-interactive mode** with a TOML answer file, so VMs and CI can install unattended.
- **Install engine:** `neue-install-core` is a library. The GUI and the CLI both use it.

### 5.4 Updates and snapshots
- A pacman hook and `neue-updated` take a Btrfs snapshot before every transaction.
- Software and Preferences show the snapshots.
- systemd-boot entries for booting into a snapshot are a stretch goal.
- A "Time Machine" style backup app is a later phase.

---

## 6. Development and test environment

### 6.1 What runs where
| Task | Where |
|---|---|
| Toolkit, theme, widget gallery, most app UI | **macOS, natively** (winit backend on Metal). The fastest loop |
| Unit and golden tests for portable crates | macOS and Linux |
| Compositor and Linux-only crates: build, unit and headless tests | **OrbStack** Arch machine (arm64; amd64 via Rosetta) |
| Full graphical session (compositor on DRM, shell, portals) | **Parallels** VM running Arch Linux ARM |
| Nested compositor iteration | Inside the Parallels VM, under a stock Wayland session (winit backend) |
| ISO and installer smoke tests | Parallels (aarch64 ISO), QEMU with KVM in CI (x86_64 ISO), QEMU TCG on the Mac (slow, rare) |
| Real performance, NVIDIA/AMD/Intel, multi-monitor, VRR | **Your x86_64 PCs**, on a spare disk or partition |

### 6.2 Software you need
- **Already installed:** Parallels Desktop 27, OrbStack, rustup (Rust 1.98), UTM (optional
  fallback), GitHub CLI.
- **Optional:** `qemu` from Homebrew, for scripted headless ISO boot tests.
- **No VMware needed.** Parallels on Apple silicon cannot run x86_64 guests at usable speed,
  so x86_64 graphics testing happens on the PCs.

### 6.3 Phase 0 environment checks (resolved)
All resolved in Phase 0; details in `docs/research/` and `docs/dev-setup.md`.
1. **Arch Linux ARM in Parallels:** installed via archboot with GRUB (UEFI); see
   `arch-arm-parallels.md` and `dev-setup.md`.
2. **GPU in the guest:** virgl, OpenGL ES 3.0 (no compute); Vulkan is llvmpipe only. VM FPS
   is not a performance signal (`vm-graphics.md`).
3. **anvil:** works on DRM (after replacing archboot's kmscon with getty) and nested in weston.
4. **vello:** `vello_gpu` renders on virgl GL in the VM and on Metal on the Mac
   (`vm-graphics.md`, `paint-poc.md`).
5. **Global menus:** coverage table in `global-menu.md`; strategy in ADR 0005.
6. **Masonry:** `masonry.md`; decision in ADR 0002.

---

## 7. Repository layout (target)
```
neueos/
├── Cargo.toml              # workspace, shared deps and lints
├── AGENTS.md  PLAN.md  LICENSE  README.md
├── crates/                 # shared libraries
│   ├── neue-paint/  neue-theme/  neue-platform/  neue-kit/
│   ├── neue-desktop/  neue-dbus/  neue-config/  neue-ipc/
│   └── neue-protocols/     # private Wayland protocol XML + bindings
├── compositor/             # neue-comp, neuectl
├── shell/                  # neue-shell
├── services/               # settingsd, indexd, glance, portal, polkit-agent, updated
├── apps/                   # files, prefs, term, write, viewer, software, setup, installer, greeter, gallery
├── session/                # neue-session, systemd units, greetd config
├── themes/                 # gtk/, kvantum/, icons/, cursors/, sounds/, wallpapers/
├── distro/                 # pkgbuilds/, iso/, repo tooling, pacman hooks
├── xtask/                  # cargo xtask: build, test, package, iso, vm
└── docs/                   # adr/, design/, protocols/
```

---

## 8. Roadmap
Phases run in order. Each phase ends with a usable state. Sizes are relative (S, M, L, XL).

### Phase 0: Foundations (M)
One task = one thread = one PR. Do them roughly in this order; 0.1 → 0.2 → 0.3 are
sequential, 0.4 can start once R2 is in, and 0.6 / 0.7 can run in parallel.

- [x] **0.0 Research spikes (R1–R4)** · Opus thread launching Sonnet subagents.
      Desk research only, results in `docs/research/`:
      R1 global menus on Wayland per toolkit, R2 Arch Linux ARM on Parallels,
      R3 Masonry evaluation, R4 vello_cpu maturity.
      *Accept:* four notes, each with a recommendation, open questions and sources.
- [x] **0.1 Workspace skeleton** · Sonnet. `LICENSE` (GPL-3.0-or-later), root
      `Cargo.toml` (edition 2024, `[workspace.dependencies]`, `[workspace.lints]`),
      `rust-toolchain.toml`, `rustfmt.toml`, `.gitignore`, and one stub portable crate
      (`crates/neue-paint`) plus one stub Linux-only crate (`compositor/neue-comp`, gated
      with `cfg(target_os = "linux")`).
      *Accept:* `cargo fmt --check`, `cargo clippy --workspace -- -D warnings` and
      `cargo test --workspace` pass on macOS.
- [x] **0.2 `cargo xtask` skeleton** · Sonnet. `xtask/` crate with `check` (fmt + clippy
      + tests, skipping Linux-only crates on macOS through a crate list, not comments),
      and `linux` / `linux-amd64` that run a command in the OrbStack machines. Other
      subcommands print "not implemented yet". Update "Commands" in `AGENTS.md`.
      *Accept:* `cargo xtask check` passes on macOS; `cargo xtask linux cargo check`
      works once 0.4 is done.
- [x] **0.3 CI** · Sonnet. GitHub Actions running `cargo xtask check` on macOS (aarch64),
      Linux x86_64 and Linux arm64, with a pinned toolchain and caching.
      *Accept:* all three jobs green on the PR; required checks enabled on `main`.
      (Jobs green in PR #6. Enabling required checks on `main` is an owner step in GitHub settings.)
- [x] **0.4 Dev machines + `docs/dev-setup.md`** · Sonnet writes, **owner runs**.
      Exact steps for the OrbStack Arch machines (arm64 and amd64) and the Parallels
      Arch Linux ARM VM (route chosen from R2), file sync (Parallels Tools or
      virtiofs/SSH + rsync), and the Rust toolchain in each.
      *Accept:* owner has all three machines; the doc matches what was actually done.
      (Owner ran `docs/dev-setup.md`; results recorded at its end. GRUB_UEFI used, since
      archboot offers no systemd-boot.)
- [x] **0.5 VM graphics checks** · Opus, owner at the VM. Record `eglinfo` (virgl vs.
      llvmpipe), run Smithay's `anvil` on the VM's DRM device and nested under a stock
      Wayland session, and run a vello demo in the VM. Results in
      `docs/research/vm-graphics.md`.
      *Accept:* anvil shows a window on DRM in the VM (or a documented blocker and
      fallback).
      (virgl GLES 3.0, Vulkan only llvmpipe. anvil works on DRM and nested in weston once
      archboot's kmscon is replaced by getty. `vello_gpu` renders on virgl GL. Run over SSH and
      checked with `prlctl capture`; a few checks at the VM window are listed as open questions.)
- [x] **0.6 `neue-paint` proof of concept** · Opus. Procedural linen and an Aqua push
      button (normal + pressed, light + dark) as `neue-paint` code; an example that
      shows them in a winit window via vello on macOS; a test that renders them with
      `vello_cpu` and compares against a committed golden PNG at scale 1 and 2.
      *Accept:* example runs on the Mac; golden test passes in `cargo xtask check`.
      (Recorded `Scene` replayed into `vello_cpu` and `vello_gpu`; the example uses
      `vello_gpu` on wgpu behind the `gpu` feature. Notes for ADR 0002 in
      `docs/research/paint-poc.md`.)
- [x] **0.7 Global-menu verification** · Sonnet writes the checklist from R1, **owner
      runs it** in the VM under a stock Plasma Wayland session (Qt6, GTK3, Chromium/
      Electron, Firefox). Results added to `docs/research/global-menu.md`.
      *Accept:* a coverage table with tested/observed status per toolkit.
      (Run by Claude over SSH in the VM's Plasma session on 2026-11-03, with no input injection;
      results in `global-menu-checklist.md`, summary in `global-menu.md`. Open items listed there.)
- [x] **0.8 ADRs 0001–0005** · Opus. 0001 Smithay, 0002 toolkit stack and Masonry (from
      R3, R4, 0.6), 0003 IPC, 0004 config format, 0005 global-menu strategy (from R1,
      0.7). Then split Phase 1 into tasks.
      *Accept:* five ADRs merged; Phase 1 has a task list.
      Plan corrections from R1–R4 to settle in these ADRs (then fix §3.2, §6.3):
      - GTK3/GTK4 on Wayland export menus via `gtk_shell1` + `org.gtk.Menus`, not
        `appmenu-gtk-module` (X11-only), so neue-comp also needs `gtk_shell1`.
      - Linebender's vello compute crate is now experimental; `vello_cpu` +
        `vello_gpu` (ex-`vello_hybrid`) are the maintained pair. `neue-paint` should
        own a small command list and replay it per backend.
      - Masonry has its own layout (not taffy), no springs, and a paint API moving to
        the `imaging` crate; crates.io release lags `main` by a year.
      (ADRs in `docs/adr/`; §3.1, §3.2, §3.3 and §6.3 updated, Phase 1 split below.)
- **Exit:** CI is green on 3 platforms. Smithay runs in the VM. Proof-of-concept renders
  on the Mac.

### Phase 1: Compositor MVP (L)
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
      `docs/dev-setup.md` §3.
- [x] **1.2 Headless backend + test harness** · Opus. Headless backend (EGL surfaceless when
      pixels are needed), a scripted test client on `wayland-client`, and a first test
      that maps an `xdg_toplevel` and asserts the window list. Runs in CI on Linux.
      *Accept:* the test passes in `cargo xtask check` on both Linux CI jobs.
      *Done:* in-process harness (`compositor/neue-comp/tests/harness/`) with socket-pair
      clients and `Server::with_state`; headless backend has no renderer yet, EGL moves to
      1.16 (ADR 0006). Linux checks need `libwayland-client` (dev-setup §1.2).
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
      The owner hasn't confirmed by hand yet. VM steps: `docs/dev-setup.md` §4.
- [ ] **1.5 Floating window management** · Opus. Focus and stacking rules, interactive move
      and resize (xdg `move`/`resize`), window placement, minimise/zoom/close as actions,
      xdg-activation focus stealing rules.
      *Accept:* headless tests for focus/stacking; `neuectl action` covers each operation.
- [ ] **1.6 `neue-config` + compositor config and hot reload** · Sonnet. New portable crate
      `neue-config` with the `compositor.toml` schema (keyboard layouts and xkb options,
      pointer, keybindings, output scale), layering and the inotify reload per ADR 0004;
      `reload-config` and `config_error` in `neuectl`.
      *Accept:* changing the xkb layout in the file applies without restart (headless test
      plus VM check).
- [ ] **1.7 Multi-monitor and scaling** · Opus. Output layout from config, hotplug,
      fractional-scale-v1, viewporter, per-output scale, windows moving between outputs.
      *Accept:* headless test with two outputs at scales 1 and 1.5; VM hotplug by resizing
      the Parallels window (owner).
- [ ] **1.8 Clipboard and DnD** · Sonnet. Data device (core part exists since 1.1), primary selection, ext-data-control,
      DnD with icon surfaces.
      *Accept:* `wl-copy`/`wl-paste` round trip in a headless test; DnD of a file between two
      GTK apps in the VM.
- [ ] **1.9 Cursors** · Sonnet. XCursor theme loading (pure-Rust `xcursor` crate),
      cursor-shape-v1, per-scale cursor sizes, hardware cursor plane on tty.
      *Accept:* correct cursors on resize edges and text fields in the VM.
- [ ] **1.10 Server-side title bars** · Opus. xdg-decoration and kde server-decoration;
      title bar and traffic lights drawn with `neue-paint` on `vello_cpu`, nine-sliced and
      cached per state/scale, uploaded via `MemoryRenderBuffer`; title text through parley
      with a bundled placeholder font. Touches `neue-paint` too.
      *Accept:* golden tests for the title bar (focused/unfocused × light/dark × 1x/2x);
      a GTK3 app shows our chrome in the VM; title bar render time logged.
- [ ] **1.11 Chrome interaction** · Sonnet. Traffic-light hover/press and actions, drag to
      move, double-click to zoom/minimise (per config), invisible resize edges.
      *Accept:* headless tests on hit-testing; VM check by the owner.
- [ ] **1.12 Shell-facing protocols** · Sonnet. wlr-layer-shell, ext-session-lock,
      ext-idle-notify, idle-inhibit, ext-foreign-toplevel-list, xdg-foreign, xdg-dialog-v1
      (modal flag only; sheets come later), xdg-toplevel-icon, xdg-system-bell,
      keyboard-shortcuts-inhibit.
      *Accept:* a headless test per protocol binding; `swaylock` or another ext-session-lock
      client locks and unlocks in the VM.
- [ ] **1.13 Input protocols** · Sonnet. pointer-constraints, relative-pointer,
      pointer-gestures, text-input-v3, input-method-v2, touchpad gestures config.
      *Accept:* headless tests; an IME (e.g. fcitx5) commits text into a GTK app in the VM.
- [ ] **1.14 Presentation and sync** · Opus. linux-drm-syncobj-v1 (explicit sync),
      presentation-time, tearing-control, security-context-v1 filtering of privileged globals.
      *Accept:* headless tests; explicit sync exercised on the NVIDIA PC (owner, see 1.18).
- [ ] **1.15 Direct scanout and VRR** · Opus. Direct scanout for full-screen surfaces,
      adaptive sync per output (config), damage review.
      *Accept:* scanout and VRR confirmed on real x86_64 hardware (owner); flagged in the PR.
- [ ] **1.16 Screencopy** · Sonnet (Opus for the renderer part). ext-image-copy-capture
      (output and toplevel sources), `neuectl screenshot`. First gives the headless backend
      an optional GLES renderer on EGL surfaceless/device (ADR 0006); CI then needs Mesa.
      *Accept:* `neuectl screenshot` PNG matches a headless golden; `grim` works in the VM.
- [ ] **1.17 Global-menu protocol side** · Sonnet. `org_kde_kwin_appmenu` v2 and `gtk_shell1`
      v5 per ADR 0005 (`gtk_shell1` XML in a new `neue-protocols` crate); menu addresses in
      `neuectl windows`.
      *Accept:* Kate (with a test registrar) and gnome-text-editor show addresses in
      `neuectl windows` in the VM.
- [ ] **1.18 xwayland-satellite on demand** · Sonnet. Reserve X display sockets, spawn
      `xwayland-satellite` on first connection, export `DISPLAY` (ADR 0001).
      *Accept:* `xeyes` (or another X11 app) appears in the VM; nothing X-related runs before.
- [ ] **1.19 `neue-session` and systemd units** · Sonnet. New `session/neue-session` crate:
      environment import, `graphical-session.target`, units for `neue-comp`, and a greetd /
      wayland-sessions entry.
      *Accept:* the session starts from greetd in the VM and logs out cleanly.
- [ ] **1.20 First PKGBUILD + `cargo xtask package`** · Sonnet. PKGBUILD for `neue-comp`,
      `neuectl` and `neue-session` with vendored git deps; `xtask package <crate>`.
      *Accept:* package builds in the OrbStack amd64 and arm64 machines and installs in the VM.
- [ ] **1.21 Daily-drive pass** · Opus with the owner. foot/Alacritty, Firefox, a Qt app, a
      Flatpak, an X11 app on the NVIDIA PC, an AMD/Intel PC and the VM; fix or file bugs.
      *Accept:* the Phase 1 exit criterion below, with a results table in
      `docs/research/phase1-daily-drive.md`.
- **Exit:** daily-drivable with third-party apps (foot/Alacritty, Firefox, Flatpaks) on the
  PC and the VM.

### Phase 2: Toolkit and design system v1 (XL)
- [ ] neue-paint materials, neue-theme tokens (light and dark, accent colour).
- [ ] neue-kit core: widget tree, layout, text (parley), IME, accessibility (AccessKit),
      animation, menus with DBusMenu export.
- [ ] Widget set v1 (§3.2). Widget gallery app on macOS and Linux. Golden tests.
- [ ] `docs/design/` specs.
- **Exit:** the gallery shows the whole widget set, in light and dark, identically on
  macOS and in the VM.

### Phase 3: Shell MVP and session (L)
- [ ] Menu bar: global menu pipeline, fallback menu, menu extras (clock, volume via
      pipewire-rs, network, Bluetooth, battery), SNI tray.
- [ ] Dock: shelf, magnification, indicators, minimise targets, Trash.
- [ ] Notifications server and banners. OSDs.
- [ ] Greeter (greetd), lock screen, polkit agent.
- [ ] Portal backend v0: Settings, Screenshot, ScreenCast, FileChooser (simple).
- [ ] `neue-settingsd` and Preferences v0 (Appearance, Dock, Displays, Sound, Keyboard,
      Trackpad).
- [ ] Packages plus `neueos-desktop`. A dev ISO with a CLI install script.
- **Exit:** log in from the greeter on your PC and use neueOS as the daily desktop.

### Phase 4: Overview and Spaces (L)
- [ ] Spaces (per display), full-screen Spaces, ext-workspace.
- [ ] Overview (Mission Control), App Exposé, Show Desktop, hot corners, gestures,
      app switcher.
- [ ] Genie/scale minimise, open/close animations, sheets (xdg-dialog).
- **Exit:** the Overview hits a steady 120 fps on AMD, Intel and NVIDIA PCs.

### Phase 5: Files and Glance (XL)
- [ ] Files: views, sidebar, UDisks2, Trash, Get Info, tags, tabs, thumbnails.
- [ ] Glance service and previewers. Full FileChooser portal.
- **Exit:** daily file management without third-party file managers.

### Phase 6: Search, Notification Center, Launcher, Widgets (L)
- [ ] `neue-indexd` and the Search UI.
- [ ] Notification Center panel.
- [ ] Launcher.
- [ ] Widgets.

### Phase 7: Apps (XL)
- [ ] Terminal.
- [ ] Write.
- [ ] Viewer.
- [ ] Software, with `neue-updated` and snapshots.

### Phase 8: Distribution (L)
- [ ] Third-party theming: GTK, Kvantum, icons, cursors, sounds, Flatpak extensions.
- [ ] Signed repo and release CI.
- [ ] GUI Installer, GPU detection, Setup Assistant.
- [ ] Release ISOs for x86_64 and aarch64.
- **Exit:** install neueOS on a fresh PC from the ISO without using a terminal.

### Later
HDR and colour management, a Time Machine style backup, network shares in Files, a Widgets
plugin API, AirDrop-like local sharing, Handoff-like phone integration, and a screen saver.

---

## 9. Quality bars
- **Performance:** no dropped frames during animations at the display's refresh rate
  (120 Hz or more). Direct scanout for full-screen windows. Shell cold start under 300 ms.
  Idle CPU about 0%.
- **Robustness:** a shell or service crash never takes down the compositor or the session.
  The compositor restarts the shell.
- **Testing:**
  - Unit tests.
  - Golden-image tests (vello_cpu).
  - Compositor integration tests on the headless backend with scripted Wayland clients.
  - VM end-to-end tests driven by `neuectl` (including screenshots).
- **Accessibility:** AccessKit/AT-SPI in every first-party app, keyboard navigation
  everywhere, and reduced-motion and contrast settings.
- **Security:** minimal privileged code (only `neue-updated` and the installer), all of it
  behind polkit. `security-context-v1` restricts privileged protocols for sandboxed apps.

---

## 10. Risks
| Risk | Mitigation |
|---|---|
| The scope is enormous (a toolkit, a compositor and a distro) | Strict phases. Daily-drivable from Phase 3 onward with third-party apps filling the gaps |
| A custom toolkit is hard: text editing, IME, accessibility | Use parley, AccessKit and possibly Masonry. Avoid inventing new text or accessibility stacks |
| Global menu coverage (GTK4/libadwaita and Firefox likely won't export menus) | The fallback menu. Verify each toolkit in Phase 0 and document the coverage |
| NVIDIA quirks (explicit sync, GBM, suspend) | Test on the NVIDIA PC from Phase 1. nvidia-open only for Turing and newer |
| Parallels and Arch Linux ARM support (Parallels Tools, GPU acceleration) | Fallbacks: virtiofs/SSH, software rendering, UTM |
| Arch Linux ARM lags x86_64 Arch | aarch64 is a dev target only. Releases are judged on x86_64 |
| Smithay and Linebender API churn | Pin versions in the workspace. Upgrade deliberately in dedicated PRs |
| Artwork volume (icons, materials) | Procedural materials first. Icons arrive progressively, with Papirus as the fallback |

---

## 11. Open questions (non-blocking)
1. **⌘ semantics:** should Super act as ⌘ globally, with the compositor remapping Super+C
   to Ctrl+C for third-party apps (except in terminals)? Recommendation: yes, with a
   per-app exceptions list.
2. **UI font:** choose from the candidates in §4 after a side-by-side rendering in the
   gallery.
3. **Final product names** (§2) and the neueOS logo and boot splash.
4. **Repo hosting:** GitHub Releases or a separate static host or domain.
5. **Network shares in Files:** GVfs over D-Bus (pragmatic) or a pure-Rust route.
