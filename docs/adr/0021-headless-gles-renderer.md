# 0021: Headless GLES renderer on Mesa's software EGL device (task 1.16a)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Screencopy (1.16b–d) needs pixels from the headless backend so tests can compare captures
with goldens. ADR 0006 left the headless backend without a renderer and named "EGL
surfaceless/device" as the way to add one. Open points: which EGL platform, when the
backend renders, what it renders into, and whether every test pays for it.

## Decision
1. **Opt-in.** `Options::headless_renderer` (default off). Only tests that read pixels
   start a renderer (`Server::start_rendering()` in the harness); the others keep running
   without Mesa and stay fast. Start-up fails with a clear error if it is on and Mesa's
   software device is missing.
2. **Mesa's software EGL device only.** `EGLDevice::enumerate()` filtered by
   `EGL_MESA_device_software` (llvmpipe), then `EGLDisplay` → `EGLContext` →
   `GlesRenderer`, as in the tty backend. No hardware device and no surfaceless fallback:
   the surfaceless platform picks a GPU when one exists, and pixels would then differ
   between CI, OrbStack and the VM. Deterministic goldens matter more than speed here.
3. **On demand.** `State::headless_render(name)` draws one output and reads it back. The
   frame timer keeps sending frame callbacks without rendering, so tests that don't ask
   for pixels cost nothing extra.
4. **One `GlesRenderbuffer` per output** (RGBA8, core GLES 3), made on first render and
   remade when the mode changes, with its own `OutputDamageTracker`. The buffer is reused,
   so renders after the first use buffer age 1 and only redraw damage; the damage is
   returned with the pixels, ready for ext-image-copy-capture's damage events (1.16b).
5. **Read back as ARGB8888** (`wl_shm`'s mandatory format, B G R A in memory), top row
   first, rows packed. Smithay already flips y when drawing into an offscreen target, so
   no row reversal is needed; a test checks the orientation.
6. **Same elements as the real backends:** `render::output_elements` (windows, title bars,
   layer surfaces, lock surface) and the shared `render::CLEAR_COLOR`. The pointer and a
   drag icon are left out; 1.16b decides how captures show the cursor.
7. Still **no linux-dmabuf global** on headless: clients use shm, as before.

## Consequences
- Linux CI installs `libegl1 libegl-mesa0 libgl1-mesa-dri`; OrbStack machines already have
  `mesa` (dev-setup §1.2). No new crates: Smithay's `backend_egl` and `renderer_gl` were
  already enabled, and EGL/GLES are dlopened.
- The `render` module is no longer gated on the `winit`/`tty` features.
- Two more `unsafe` constructor calls, with the same SAFETY reasoning as the tty backend.
- llvmpipe output can change between Mesa versions (blending, filtering at fractional
  scale). Goldens must be blessed on CI's Mesa or compared with a small tolerance; 1.16b
  picks one.

## Alternatives considered
- **Surfaceless platform (`EGL_MESA_platform_surfaceless`).** Works without device
  enumeration, but uses the GPU when there is one: non-deterministic pixels.
- **Render on every frame-timer tick.** Closer to a real output, but costs CPU in every
  rendering test for frames nobody reads.
- **Pixman (software) renderer.** Smithay has one, but the real backends use GLES and
  `OutputElement` is built for `GlesRenderer`; a second renderer type would test different
  code than what users see.
- **Always on.** Every compositor test would need Mesa and pay for renderer start-up.
