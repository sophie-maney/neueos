# Architecture decision records

One file per decision: `NNNN-short-title.md`. Write one for every new dependency (especially
C), protocol, IPC change, cross-crate interface, or change to a decision in `PLAN.md`.

Format: **Status** (Proposed, Accepted, Superseded by NNNN), **Date**, **Context**,
**Decision**, **Consequences**, **Alternatives considered**, and optionally **Revisit when**.
Keep each under about two pages; link to `docs/research/` for evidence instead of copying it.
Don't edit an accepted ADR's decision; supersede it with a new one.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-smithay.md) | Smithay as the compositor library | Accepted |
| [0002](0002-toolkit-stack.md) | Toolkit stack: neue-paint on vello_cpu/vello_gpu, Masonry for the widget tree | Accepted (gate passed 2026-11-04) |
| [0003](0003-ipc.md) | IPC boundaries and the `neuectl` protocol | Accepted |
| [0004](0004-config-format.md) | Configuration format and reload | Accepted |
| [0005](0005-global-menu.md) | Global-menu strategy | Accepted |
| [0006](0006-compositor-test-harness.md) | Compositor test harness | Accepted |
| [0007](0007-neuectl-wire-details.md) | `neuectl` wire details (window ids, events, errors) | Accepted |
| [0008](0008-floating-window-management.md) | Floating window management rules | Accepted |
| [0009](0009-compositor-config-details.md) | Compositor config details (loader, bindings, watcher, events) | Accepted |
| [0010](0010-outputs-and-scaling.md) | Outputs: layout, hotplug and scaling | Accepted |
| [0011](0011-selections-and-dnd.md) | Clipboard, primary selection, data-control and DnD icons | Accepted |
| [0012](0012-cursors.md) | Cursors: XCursor themes, cursor-shape-v1, resize cursors | Accepted |
| [0013](0013-server-side-title-bars.md) | Server-side title bars and text in `neue-paint` | Accepted |
| [0014](0014-chrome-interaction.md) | Chrome interaction: hit-testing, traffic lights, drag, resize edges | Accepted |
| [0015](0015-shell-facing-protocols.md) | Shell-facing protocols: layer shell, session lock, idle, toplevel list and friends | Accepted |
| [0016](0016-input-protocols.md) | Input protocols: pointer constraints, relative pointer, gestures, text input, input method | Accepted |
| [0017](0017-presentation-and-sync.md) | Presentation and sync: presentation-time, tearing hints, explicit sync, security context | Accepted |
| [0018](0018-full-screen-windows.md) | Full-screen windows (stopgap until full-screen Spaces) | Accepted |
| [0019](0019-draw-only-when-needed.md) | Draw only when needed: per-output redraw state, estimated vblank | Accepted |
| [0020](0020-direct-scanout-and-vrr.md) | Direct scanout (primary plane) and adaptive sync per output | Accepted |
| [0021](0021-headless-gles-renderer.md) | Headless GLES renderer on Mesa's software EGL device | Accepted |
| [0022](0022-screencopy-output-capture.md) | Screencopy of outputs with ext-image-copy-capture | Accepted |
| [0023](0023-screencopy-toplevel-capture.md) | Screencopy of toplevels with ext-image-copy-capture | Accepted |
| [0024](0024-neuectl-screenshot.md) | `neuectl screenshot`: capture to PNG, deferred replies | Accepted |
| [0025](0025-global-menu-protocol-side.md) | Global-menu protocol side: appmenu v2, `gtk_shell1` v5, menu addresses in `neuectl windows` | Accepted |
| [0026](0026-xwayland-satellite-on-demand.md) | xwayland-satellite on demand | Accepted |
| [0027](0027-session-and-systemd-units.md) | `neue-session` and the systemd user units | Accepted |
| [0028](0028-first-pkgbuild-and-xtask-package.md) | The first PKGBUILD and `cargo xtask package` | Accepted |
| [0029](0029-smithay-fork-pin.md) | Pin Smithay to a fork commit for the popup commit fix (B5) | Accepted |
| [0030](0030-masonry-fork-pin.md) | Pin Masonry to a fork and `imaging` to a git rev | Accepted |
| [0031](0031-neue-theme-tokens.md) | `neue-theme`: design tokens selected by a `Variant` | Accepted |
| [0032](0032-winit-driver.md) | neue-platform's winit driver: macOS only, one shared renderer | Accepted |
| [0033](0033-wayland-driver.md) | neue-platform's Wayland driver: one app API, paced by frame callbacks | Accepted |
| [0034](0034-wayland-popups-clipboard-a11y.md) | Wayland driver: popup placement, clipboard, activation, dialogs, AccessKit | Accepted |
| [0035](0035-neue-kit-app-model.md) | neue-kit app model: messages, retained widgets, handles, background tasks | Accepted |
| [0036](0036-spring-animation.md) | Spring animation: closed-form solver in neue-theme, stepped on Masonry frames | Accepted |
| [0037](0037-widget-pattern-goldens-default-button.md) | The widget pattern: gallery goldens, window root, default button | Accepted |
| [0038](0038-ui-font-dejavu-sans.md) | UI font: DejaVu Sans | Accepted |
| [0039](0039-translucent-materials.md) | Translucent materials: opaque fallback, theme alpha, Specimen widget | Accepted |
| [0040](0040-button-family-widgets.md) | The button family: checkbox, radio, bevel, recessed, segmented | Accepted |
| [0041](0041-value-controls.md) | Value controls: slider, stepper, progress bar, spinner | Accepted |
| [0042](0042-text-fields.md) | Text fields: our own editor widget, secure input, search field | Accepted |
| [0043](0043-scroll-view-virtual-list.md) | Scroll view with overlay scrollers, and the virtual list | Accepted |
| [0044](0044-text-view.md) | Text view: multi-line text as scroll view content | Accepted |
| [0045](0045-menus.md) | Menus: the menu model, menu layers and context menus | Accepted |
| [0046](0046-popup-buttons-combo-box-edit-menu.md) | Pop-up button, pull-down button, combo box and the edit menu | Accepted |
| [0047](0047-dbusmenu-export.md) | DBusMenu export: the neue-dbus crate and window menu bars | Accepted |
| [0048](0048-table-view.md) | Table view: header slot, table body on the virtual rows, shared state | Accepted |
| [0049](0049-outline-view.md) | Outline view: a table over a flattened tree, expansion by id, selection that follows rows | Accepted |
| [0050](0050-source-list.md) | Source list: an outline style with unselectable group headers, sidebar and selection materials | Accepted |
| [0051](0051-icon-type.md) | Icon type: glyphs and filled SVG-path vector icons, current or fixed light/dark paints | Accepted |
| [0052](0052-collection-view.md) | Collection view: a collection body on the virtual grid, ids for the app and indices inside, items that draw their selection | Accepted |
| [0053](0053-split-view.md) | Split view: one widget owns panes and dividers, lengths as state, a divider moves a pair, collapse | Accepted |
| [0054](0054-tab-view.md) | Tab view: one widget owns strip, box and stashed panels, synthetic tab nodes, Ctrl-Tab from inside | Accepted |
| [0055](0055-client-side-window-chrome.md) | Client-side chrome for kit windows; one window-chrome material of any height, the server-side bar its 22 px case | Accepted |
| [0056](0056-popovers.md) | Popovers: a kit layer that tells the Wayland driver its anchor through Masonry properties; window geometry instead of a positioner offset | Accepted |
| [0057](0057-sheets.md) | Sheets: one kit widget; a modal child window on Wayland, drawn in the parent with a scrim on macOS; the sheet owns Return, Escape and Tab | Accepted |
| [0058](0058-sheet-placement.md) | Sheet placement in neue-comp: modal dialogs with a parent hang from its chrome and follow it | Accepted |
| [0059](0059-hud-windows.md) | HUD windows: a third kind of kit chrome over the HUD material; a parent keeps its focused look while its HUD window has the keyboard | Accepted |
| [0060](0060-path-control.md) | Path control: one leaf widget that collapses its middle into an ellipsis; the popup style is a pop-up button over the reversed path | Accepted |
| [0061](0061-color-well.md) | Colour well: a bevel-button swatch that opens a popover panel built from kit widgets; the panel keeps the colour as HSB and reports to the well | Accepted |
| [0062](0062-date-picker.md) | Date picker: a kit civil-date type and two leaf widgets (textual field, calendar), each with its own builder and handle | Accepted |
| [0063](0063-window-theme-and-focus-leave.md) | A window carries its own theme properties; the Wayland driver never drops a keyboard leave | Accepted |
| [0064](0064-neue-window-protocol.md) | `neue_window_v1`: a window tells neue-comp where its own chrome ends, and sheets hang under it | Accepted |
| [0065](0065-imaging-fork-layer-texture-size.md) | Patch `imaging` from our fork so vello_gpu's layer textures can grow past 4096 px | Accepted |
| [0066](0066-neue-shell-protocol.md) | `neue_shell_v1`: the compositor spawns the shell on a private socket, restarts it, and is the only one to offer it windows, actions, key chords and system keys | Accepted |
| [0067](0067-shell-surfaces-in-platform-and-kit.md) | Shell surfaces: the platform reports outputs, places layer surfaces on them, binds `neue_shell_v1` for the shell and can run with no window; the kit exposes them | Accepted |
| [0068](0068-shell-skeleton-and-menu-bar.md) | The shell skeleton and menu bar: `MenuBar`/`MenuTitle` in the kit, DejaVu Sans Bold, jiff for the clock, logind for lock/restart/shut down, Log Out as the compositor's `quit` | Accepted |
| [0069](0069-desktop-background.md) | The desktop background: a `neue-desktop` background layer per output, a vector default picture, photos through the texture cache, `shell.toml`, `png` and `zune-jpeg` | Accepted |
| [0070](0070-neue-desktop-v0.md) | `neue-desktop` v0: desktop entries, app id matching, icon theme lookup, raster icons from PNG and SVG (`resvg`), launching with an activation token, the trash | Accepted |
| [0071](0071-backdrop-blur.md) | Backdrop blur: `ext-background-effect-v1`, a dual-Kawase framebuffer-effect element below each blurred surface, cached while nothing behind changes | Accepted |
| [0072](0072-backdrop-blur-clients.md) | Backdrop blur in clients: windows ask through `WindowSpec::blur`, the kit paints the translucent tint when the compositor offers blur, popups stay opaque | Accepted |
| [0073](0073-translucent-popups.md) | Translucent menus and popovers: palettes take a `Backdrop`, popup layers ask for blur with `PopupBlur`, the driver blurs the box (not the shadow or a popover's arrow), sheets stay opaque in the kit | Accepted |
| [0074](0074-neue-settingsd-v0.md) | neue-settingsd v0: `org.neueos.Settings1` over the schema's own key paths (get, set, reset, batched `Changed`), `appearance.toml`, GTK `settings.ini` and dconf via `ca.desrt.dconf.Writer`, zbus stays on async-io, `Store::in_memory` as the macOS mock | Accepted |
| [0075](0075-appearance-consumers.md) | The compositor (title bars, cursor) and the shell read and follow `appearance.toml`: `AppearanceConfig::theme()` in neue-config, per-file hot reload, `XCURSOR_*` kept as a dev override | Accepted |
| [0076](0076-dbusmenu-client-and-registrar.md) | The global menu's DBusMenu side: registrar and client in neue-dbus, one bus-wide signal stream, a shell worker with generations, replaceable app titles in the kit's `MenuBar`, icons read but not drawn | Accepted |
| [0077](0077-menu-bar-navigation.md) | Moving between the menu bar's menus: the bar switches on pointing and Left/Right, `neue_shell_v1` v2 `focus_menu_bar` from the `focus-menu-bar` action (Ctrl+F2), popup grabs for layer surfaces with the keyboard handed back, seats adopted before the first roundtrip | Accepted |
| [0078](0078-gmenu-client.md) | The global menu's GMenuModel side: `org.gtk.Menus`/`org.gtk.Actions` client in neue-dbus feeding the same menu trees, re-read on any `Changed`, app menu under the app's name, `gtk_shell1.capabilities` = 3 (bits, not the enum values) | Accepted |
| [0079](0079-super-as-command.md) | Super as ⌘: `KeyFocus` rewrites Super to Control in the modifiers sent to third-party clients; neue-kit clients (bound `neue_window_manager_v1`), `[super-as-command] exceptions` (terminals) and shortcut inhibitors get Super; bindings still win | Accepted |
| [0080](0080-kit-command-key.md) | The kit's command key is Super (⌘, META) on every host: fields, views, tables, Wayland paste (Super+V) and menu glyphs; Ctrl is no longer a command key; amends 0042 §4 | Accepted |
| [0081](0081-command-key-in-third-party-menus.md) | ⌘ in third-party menus: `neue_shell_toplevel_v1.command_key` (version 3, manager and toplevel) says whether Super reaches the app as Control; the shell shows such apps' Control shortcuts with ⌘; shortcut inhibitors are not tracked | Accepted |
| [0082](0082-fallback-menu-and-menu-search.md) | The fallback menu (App, File, Edit, Window from one description; Edit as `send_shortcut` chords, terminals get Ctrl+Shift+C/V) and Help's menu search (a popover with a search field, over any menu source); `Popover::focus`; `send_shortcut` ends a popup keyboard grab | Accepted |
| [0083](0083-battery-and-bluetooth-extras.md) | Battery, Bluetooth and network extras in the shell: a worker thread per service (UPower, BlueZ) on the system bus, snapshots on change signals plus a 5 s poll, absent or unreachable means hidden, BlueZ calls on their own threads and applied only through BlueZ's signals; the network worker debounces NetworkManager's bursts (300 ms, 2 s at most) | Accepted |
| [0084](0084-status-notifier-tray.md) | The status-icon tray: `neue_dbus::status_notifier` with a StatusNotifierWatcher the shell owns (items kept as `bus/path`, forgotten when their connection leaves), a threadless `Host` driven by the caller's signal streams, lenient `ItemInfo` reading with ARGB→RGBA pixmaps, actions and DBusMenu through `MenuClient`; overlay icons and the legacy Ayatana interface ignored | Accepted |
