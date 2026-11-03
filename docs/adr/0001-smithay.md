# 0001: Smithay as the compositor library

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
`neue-comp` must be a Rust Wayland compositor that does stacking window management,
server-side Mountain Lion chrome, Overview/Spaces effects, and runs on DRM/KMS on real
hardware (AMD, Intel, NVIDIA, hybrid), nested for development, and headless for tests
(PLAN.md §3.1). Task 0.5 (`docs/research/vm-graphics.md`) ran Smithay's `anvil` in the
Parallels VM: atomic KMS, GBM, EGL and the GLES renderer work on virgl (GLES 3.0) under
seatd, both on DRM and nested in weston. The VM has no hardware Vulkan (llvmpipe only).

## Decision
1. **Build `neue-comp` on Smithay**, with calloop as its single-threaded event loop.
2. **Version:** depend on Smithay from a **pinned git rev** of `main`, not crates.io. The
   last release (0.7.0, 2025-06) is 16 months behind `main`, and 0.5 used `c7a26ad`
   successfully. The rev lives in root `[workspace.dependencies]`; bumps happen in their
   own PRs (`deps: bump smithay to <rev>`) with the headless tests and a VM smoke run.
   Move to crates.io releases if upstream starts releasing regularly again.
3. **Features:** enable only what we use: `backend_drm`, `backend_gbm`, `backend_egl`,
   `backend_libinput`, `backend_udev`, `backend_session_libseat`, `backend_winit`,
   `renderer_gl`, `renderer_multi`, `desktop`, `wayland_frontend`. **Never** enable
   `xwayland` or `x11rb_event_source` (AGENTS.md: no X11 code).
4. **Renderer:** Smithay's `GlesRenderer` (GLES 3.0 is the floor; it's what the VM has),
   wrapped in `MultiRenderer` for hybrid and multi-GPU. Our effects (blur, shadows, linen,
   Overview transforms) are GLES shaders as custom render elements. Chrome bitmaps come
   from `vello_cpu` via `neue-paint` and are uploaded with `MemoryRenderBuffer`
   (`Abgr8888`, premultiplied; see ADR 0002). No Vulkan renderer for now.
5. **Backends:** `tty` (DRM/KMS + GBM + libinput + udev + libseat), `winit` (nested, dev),
   `headless` (tests: no outputs to KMS; renders through EGL on a surfaceless/device
   platform when a test needs pixels, which works on llvmpipe in CI and OrbStack).
6. **X11 apps:** `neue-comp` only reserves the X display sockets and spawns
   `xwayland-satellite` on the first connection, handing it the listening fds. It never
   parses X11 protocol. Same approach as niri.
7. **C libraries accepted by this ADR**, all through the safe bindings Smithay already
   uses: libinput, libudev, libseat, libgbm and libEGL/libGLESv2 (Mesa or the vendor
   driver), libxkbcommon, and libdisplay-info if Smithay enables it. DRM itself goes
   through `drm-rs` ioctls. Any other C library still needs its own ADR. In particular,
   Smithay's pixman renderer is **not** used.
8. **Reference code:** anvil (MIT), niri (GPL-3.0-or-later) and cosmic-comp (GPL-3.0-only)
   are reading material. Copying from anvil and niri is licence-compatible; record the
   origin in a comment. Do not copy from cosmic-comp (GPL-3.0-only is not compatible with
   our "or later").

## Consequences
- One event loop, no async runtime in the compositor; IPC and config reload must be calloop
  sources (ADR 0003, ADR 0004).
- Git dependencies need `cargo vendor` or `cargo fetch --locked` in the PKGBUILD; packaging
  tasks must account for this.
- Smithay API churn is ours to absorb on every bump. Keep Smithay types out of crates other
  than `neue-comp` so the blast radius stays in one crate.
- GLES-only means no compute shaders in the compositor. Effects must be fragment-shader
  based (multi-pass blur, not compute).
- `neue-comp` stays Linux-only (`LINUX_ONLY_CRATES`); pure logic that tests want on macOS
  (layout maths, IPC types, config schema) lives in portable crates.

## Alternatives considered
- **wlroots via bindings:** C core (violates Rust-only without a large ADR), and the Rust
  bindings are unmaintained.
- **From scratch on wayland-server + drm-rs:** months of work Smithay already did (DRM
  leasing, explicit sync, dmabuf feedback, input handling) for no design gain.
- **Forking niri or cosmic-comp:** both are scrolling/tiling-first with their own
  architecture; cosmic-comp's licence is incompatible. Reading them is enough.
- **crates.io Smithay 0.7:** too far behind `main` (16 months of fixes and protocol work).

## Revisit when
Smithay resumes regular releases; or we need a Vulkan renderer (HDR, colour management).
