# 0006: Compositor test harness

- **Status:** Accepted; point 5 amended by ADR 0021 (optional headless renderer)
- **Date:** 2026-11-03

## Context
Task 1.2 needs a headless backend and a way to drive the compositor with scripted Wayland
clients, so later tasks (focus and stacking, IPC, protocols, screencopy) can be tested in
CI on Linux without a GPU, a seat or a host compositor. Open points: in-process or
subprocess, how clients connect, how tests observe compositor state before `neuectl`
exists (1.3), and whether the headless backend renders.

## Decision
1. **In-process.** `neue_comp::Compositor` bundles the event loop, the Wayland display and
   `State`. The harness (`compositor/neue-comp/tests/harness/`) builds one on a background
   thread with `Options { listen: false, .. }` and runs it.
2. **Clients connect over a socket pair**, inserted with `State::insert_client`. No
   listening socket and no `XDG_RUNTIME_DIR`, so tests run in parallel without collisions.
3. **Tests observe state directly:** `Server::with_state(|state| ...)` sends a closure to
   the compositor thread over a calloop channel and returns its result. `State::windows()`
   is the first query; `neuectl` (1.3) builds on the same queries.
4. **Scripted clients use `wayland-client` + `wayland-protocols`** (MIT, dev-dependencies
   only), with blocking roundtrips after every step, so a test reads as a script. Shm
   buffers come from `memfd_create` (`rustix`, Apache-2.0 OR MIT).
5. **The headless backend does not render yet.** It has one virtual output
   (`HEADLESS-1`, 1920×1080@60), a timer that sends frame callbacks, and no linux-dmabuf
   global, so clients use shm. An optional GLES renderer on an EGL surfaceless/device
   display is added when pixels are needed (screencopy goldens, task 1.16).

## Consequences
- Tests need no GPU, no Mesa and no running session; they take milliseconds.
- `libwayland-client` must be installed where `cargo xtask check` runs on Linux: with
  `--all-features`, the dev-only `winit` feature switches `wayland-backend` to the dlopened
  system library for the whole build, test clients included. CI installs
  `libwayland-client0`; `docs/dev-setup.md` §3.1 installs `wayland` in OrbStack. Builds
  without `winit` use the pure-Rust backend and need nothing.
- Client steps block on the compositor; a compositor hang hangs the test. CI jobs have a
  45-minute timeout. `with_state` fails after 10 s.
- Tests that need input inject it through `State` methods (task 1.5 onward), since the
  headless backend has no input devices.

## Alternatives considered
- **Subprocess compositor (`neue-comp --headless`) driven only through `neuectl`.** Closer
  to production, but needs IPC first and makes every state assertion a protocol feature.
  Still possible later for end-to-end tests; the binary already accepts `--headless`.
- **Listening socket in a temporary `XDG_RUNTIME_DIR`.** Needs process-global env changes
  (`unsafe` in edition 2024) or a socket path option; socket pairs avoid both.
- **Rendering with EGL in the headless backend now.** Nothing in 1.2 needs pixels, and CI
  would need Mesa's software EGL device on both Linux runners. Deferred to 1.16.

## Revisit when
Screencopy (1.16) needs pixels, or tests need multiple outputs (1.7).
