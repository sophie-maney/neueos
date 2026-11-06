# neueOS — Project Plan

> A personal Linux desktop environment and Arch-based distribution that reinterprets the
> skeuomorphic design of OS X Mountain Lion with modern technology. Every first-party
> component is written in Rust.

Status: **planning** · Last updated: 2026-11-05 · Licence: GPL-3.0-or-later

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
- **Library:** Smithay from a pinned git rev, GLES renderer, no X11 features: ADR 0001. The
  rev is on our fork while it carries a patch (ADR 0029).

### 3.2 Toolkit (`neue-kit` and friends)
| Crate | Responsibility |
|---|---|
| `neue-paint` | Its own recorded `Scene` (kurbo, peniko), **materials** (linen, metal, aqua glass, HUD), nine-slice, procedural textures. Replayed per backend: GPU (`vello_gpu` on wgpu), CPU (`vello_cpu`, for compositor chrome and golden tests), and Masonry's `imaging` painter (ADR 0002) |
| `neue-theme` | Design tokens (ADR 0031): semantic colours, metrics, radii, typography, motion (springs, reduced motion), light and dark, accent colour, increased contrast, materials table |
| `neue-platform` | Windowing backends. **Wayland** (smithay-client-toolkit: xdg-shell, layer-shell, fractional scale, text-input-v3, DnD, clipboard, xdg-activation, xdg-dialog). **winit** (macOS only, so apps and widget galleries run natively on the Mac; ADR 0032) |
| `neue-kit` | Widget tree on **`masonry_core`** (pinned git rev, hidden behind neue-kit's own API; ADR 0002), Masonry's layout, events, focus, animation (our springs on `on_anim_frame`; ADR 0036), text through **parley** and fontique, accessibility through **AccessKit**, menus with DBusMenu export, an Elm-style app model over retained widgets with typed handles, and background tasks that report with messages (ADR 0035) |
| `neue-desktop` | freedesktop specs: desktop entries, icon themes, MIME (shared-mime-info), thumbnails, trash, recent files, autostart |
| `neue-dbus` | zbus interfaces: DBusMenu, StatusNotifierItem/Watcher, Notifications, our private services |
| `neue-audio` | PipeWire client (pipewire-rs, ADR 0087): sinks and sources, the default ones, volume and mute, on its own thread. Linux only |

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
- **Typography:** DejaVu Sans, a humanist sans in the spirit of Lucida Grande (chosen over
  Fira Sans, Source Sans 3, IBM Plex Sans and Noto Sans; ADR 0038). Monospace for
  Terminal: candidates are JetBrains Mono and Fira Code. See Q2.
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
│   ├── neue-desktop/  neue-dbus/  neue-audio/  neue-config/  neue-ipc/
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

Each phase gets split into PR-sized tasks before work on it starts. The split lives in its
own file, `docs/plan/phase-N.md`, with the tasks, models, acceptance criteria and "Done"
notes; this section keeps only a summary per phase. Unsplit phases list their scope here.

### Phase 0: Foundations (M) · done
Workspace, `cargo xtask`, CI, dev machines, research spikes and the neue-paint proof of
concept. Tasks: [`docs/plan/phase-0.md`](docs/plan/phase-0.md).
- **Exit:** CI is green on 3 platforms. Smithay runs in the VM. Proof-of-concept renders
  on the Mac.

### Phase 1: Compositor MVP (L) · done
`neue-comp` on Smithay: tty and nested backends, `neuectl`, config, outputs, input,
selections, cursors, server-side title bars, shell-facing protocols, screencopy, global-menu
addresses, xwayland-satellite, the session and packages. Tasks:
[`docs/plan/phase-1.md`](docs/plan/phase-1.md).
- **Exit:** daily-drivable with third-party apps (foot/Alacritty, Firefox, Flatpaks) on the
  PC and the VM.

### Phase 2: Toolkit and design system v1 (XL)
The Masonry gate, `neue-theme`, the `neue-platform` drivers, the `neue-kit` app model and
animation, the widget gallery and widget set v1, DBusMenu export and the `docs/design/`
specs. Tasks: [`docs/plan/phase-2.md`](docs/plan/phase-2.md). The Masonry gate (2.1, 2.2)
passed: Masonry stays (ADR 0002 amendment). The icon and collection view (2.20) is split
into the icon type (2.20a), grid virtualisation (2.20b) and the collection view (2.20c).
The split and tab views (2.21) are split into the split view (2.21a) and the tab view
(2.21b). Window chrome (2.22) is split into the unified chrome material and the ADR
(2.22a), kit window chrome (2.22b), toolbar items (2.22c) and resize edges for
client-side windows in neue-comp (2.22d). Sheet, popover and HUD panel (2.23) are split into the
popover (2.23a), the kit sheet (2.23b), sheet placement in neue-comp (2.23c) and the HUD
panel (2.23d). Colour well, date picker and path control (2.24) are split into the path
control (2.24a), the colour well (2.24b) and the date picker (2.24c). The owner's VM and
macOS run of 2026-11-05 added fixes 2.30–2.38 (before the exit check, 2.26); 2.29 was
withdrawn, as its glitch occurs only in the VM (now 2.30).
- **Exit:** the gallery shows the whole widget set, in light and dark, identically on
  macOS and in the VM.

### Phase 3: Shell MVP and session (L)
The `neue-shell-v1` protocol, the shell with menu bar (global menu, fallback menu, menu
extras, tray), backdrop blur, notifications and OSDs, the dock, lock screen, greeter and
polkit agent, `neue-settingsd`, the portal backend v0, Preferences v0, packages with
`neueos-desktop`, and an x86_64 dev ISO with a CLI installer. Super acts as ⌘ (Q1). Tasks:
[`docs/plan/phase-3.md`](docs/plan/phase-3.md). Owner checks are collected in one runbook
at the end of the phase (3.29), which also carries Phase 2's exit check (2.26); fixes from
it are numbered from 3.31. The shell skeleton (3.2) is split into shell surfaces in
neue-platform and neue-kit (3.2a), the shell and its menu bar (3.2b) and the desktop
background (3.2c). Super as ⌘ is split into the compositor remap (3.10a), the kit's
command key (3.10b) and ⌘ in third-party menus (3.10c).
Menu extras for battery and Bluetooth are split into the D-Bus clients (3.11a), icon
extras in the kit (3.11b) and the shell wiring (3.11c). The network extra follows the
same split: NetworkManager client (3.12a), icon extra in the kit (3.12b), shell wiring (3.12c).
The status-icon tray follows it too: StatusNotifier watcher, host and item client in
`neue-dbus` (3.13a), the tray area in the bar (3.13b), shell wiring (3.13c).
Volume, brightness and OSDs are split into system keys, brightness and the OSD (3.14a),
the PipeWire volume backend (3.14b), the volume extra (3.14c), and media keys and
the keyboard backlight (3.14d).
Notifications are split into the D-Bus server (3.15a), the banner and alert widgets
(3.15b), activation tokens for the shell (3.15c) and the shell wiring (3.15d, itself
split into a content-sized layer surface, the kit's notification host, the shell wiring
and the bar extra: 3.15d1–d4).
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
- [ ] Widgets (with the paper and leather materials, moved from Phase 2).

### Phase 7: Apps (XL)
- [ ] Terminal.
- [ ] Write (uses the paper material from Phase 6).
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
1. **⌘ semantics:** *decided: yes* (2026-11-05). Super acts as ⌘; the compositor remaps
   Super+C to Ctrl+C for third-party apps, with a per-app exceptions list (terminals by
   default). Tasks 3.10a–c.
2. **UI font:** *decided: DejaVu Sans* (ADR 0038). The monospace face for Terminal is
   still open (JetBrains Mono or Fira Code).
3. **Final product names** (§2) and the neueOS logo and boot splash.
4. **Repo hosting:** GitHub Releases or a separate static host or domain.
5. **Network shares in Files:** GVfs over D-Bus (pragmatic) or a pure-Rust route.
