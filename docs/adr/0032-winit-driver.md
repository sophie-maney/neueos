# 0032: neue-platform's winit driver: macOS only, one shared renderer

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.4 replaces `masonry_winit` with our own `RenderRoot` driver, so apps and the widget
gallery run natively on the Mac (PLAN.md §3.2) without Masonry's driver, its global
animation clock, its `copypasta` clipboard, or its separate wgpu setup. ADR 0002 §4 already
says winit is for macOS and sctk for Linux. Two findings settle where winit may build:
- `ui-events-winit` 0.3 (the winit → `ui-events` translation Masonry expects) depends on
  winit with default features, which on Linux include X11 (`x11-dl`, `x11rb`);
- `arboard`, the obvious clipboard crate, has an X11-only Linux backend.
The hard rules forbid X11 crates outside neue-comp's dev-only `winit` feature (ADR 0001).

## Decision
1. `neue_platform::winit` is **macOS only** (`#[cfg(target_os = "macos")]`), and so are its
   dependencies (`winit`, `ui-events-winit`, `accesskit_winit`, `arboard`). Linux apps use
   the Wayland driver; the crate itself stays portable and builds on all three targets.
2. Both drivers render through one module, `neue_platform::gpu` (moved from the 2.2
   spike): vello_gpu into an offscreen texture, blitted to the surface. A `pre_present` hook
   lets winit call `pre_present_notify`; `Outdated`/`Lost` surfaces are reconfigured and
   retried once, `Occluded`/`Timeout` frames are skipped quietly.
3. App-facing shape: `winit::run(&mut dyn App, Options)`. `App` has `on_start`,
   `on_action` and `on_close_requested` (closes by default), and gets a `Ctx` to open and
   close windows, reach a window's `RenderRoot`, and exit. `WindowId`s are ours, never
   reused. `Options` (default properties, fallback font) moved to the crate root and is
   shared with the Wayland driver. Task 2.5 aligns the Wayland driver with this shape;
   task 2.7 wraps it in neue-kit.
4. Behaviour:
   - one signal channel per window; signals are drained after every event until no
     window has any and no window is queued;
   - windows are created hidden (AccessKit's adapter must attach before showing), painted,
     then shown and redrawn;
   - animation frames are timed per window, not globally as in `masonry_winit`;
   - Masonry layers are painted as in-window overlays (only the Wayland driver makes
     popups);
   - Cmd-V is answered with `TextEvent::ClipboardPaste`, and `ClipboardStore` writes the
     pasteboard (arboard, no default features: text only);
   - the loop exits on `RenderRootSignal::Exit`, `Ctx::exit`, or when the last window
     closes.
5. New dependencies, all MIT OR Apache-2.0, pure Rust (arboard uses the `objc2` crates
   already in the tree through wgpu's Metal backend): `accesskit_winit` 0.34 (no
   `accesskit_unix`), `ui-events-winit` 0.3, `arboard` 3.

## Consequences
- No X11 crate reaches a Linux build of neue-platform (checked with `cargo tree`).
- Not done yet: Metal's `presentsWithTransaction` during live resize (`masonry_winit`
  does it with `unsafe`; we accept some resize jitter for now), proxies for background
  tasks (task 2.7), system fonts (ADR 0030: no CJK glyphs in the demo yet).
- The AGENTS.md platform matrix lists the winit driver as macOS-only.
