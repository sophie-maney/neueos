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
