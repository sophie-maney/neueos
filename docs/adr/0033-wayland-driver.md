# 0033: neue-platform's Wayland driver: one app API, paced by frame callbacks

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Task 2.5 turns the 2.2 sctk spike into the Linux driver. The spike took a fixed list of
surfaces and a closure for actions; the winit driver (ADR 0032) has an `App`/`Ctx` API
with windows opened and closed at run time. Apps (and neue-kit, task 2.7) must run
unchanged on both. The spike also lacked cursor shapes, key repeat, a key table and
tests, and could block: Mesa's EGL and Vulkan swapchains wait inside present for the
previous frame's callback, which never comes while a surface is hidden.

## Decision
1. **One app API at the crate root.** `App`, `Ctx`, `WindowId` and `WindowSpec` move from
   `winit` to the crate root, with a crate-private `Host` trait behind `Ctx` and a shared
   `Registry` (open windows, the open queue, ids never reused, the exit flag).
   `neue_platform::run` and `neue_platform::Error` are the host's driver. `WindowSpec`
   gains `app_id` and `kind`: `WindowKind::Toplevel` or `WindowKind::Layer(LayerSpec)`
   (layer, anchor, margin, exclusive zone, keyboard mode, in our own types, not sctk's).
   The winit driver opens a toplevel for a layer spec and logs a warning.
2. **`wayland::run_with_connection`** runs on a connection the caller made, so headless
   compositor tests hand the driver one end of a socket pair. `wayland::run` connects
   from the environment.
3. **The app lives outside the event-loop state.** calloop and wayland-client need a
   `'static` state type, so handlers queue actions and close requests, and the loop hands
   them to the app after each dispatch, with the signal drain (ADR 0032 §4).
4. **Drawing is paced by our own frame callbacks, per surface.** A surface presents only
   when its last callback has fired, with a new callback request in the same commit, so
   the swapchain's own wait is always over and present never blocks. A surface that
   misses a draw is marked stale and drawn when its callback fires. If wgpu skips a frame
   (occluded, timeout), the driver commits the surface itself so the callback still
   arrives. The loop dispatches with no timeout.
5. **Input.** Key repeat comes from sctk's calloop timer (the compositor's rate and delay),
   or from `wl_keyboard` v10 repeats. Physical keys, named keys and locations come from
   tables ported from winit 0.30's xkb keymap (Apache-2.0 OR MIT). Ctrl-letter keys
   report their letter. Cursors use `wp_cursor_shape_v1`, falling back to the xcursor
   theme through sctk's `ThemedPointer`. The driver counts multiple clicks (500 ms, 4 px)
   and reports wheel steps as lines.
6. **Close and teardown.** A toplevel's close request goes to `App::on_close_requested`
   (closes by default). The loop ends on `Exit`, `Ctx::exit`, or when no window is open
   or queued. Windows drop popups newest first, then the wgpu surface, then the role
   object and `wl_surface`; viewports and fractional-scale objects are destroyed.
   A layer surface the compositor closes is closed without asking the app.

No new dependencies: sctk's `ThemedPointer` and repeat timer, `wayland-cursor` and
`xkeysym` were already in the tree through sctk.

## Consequences
- The 2.4 demo (`examples/demo.rs`) runs on both drivers; `sctk_spike` is gone.
- `compositor/neue-comp/tests/platform.rs` maps a driver window on the headless
  compositor with its GLES renderer and checks a pixel of its wgpu-drawn background.
  wgpu uses Mesa's GL through `libwayland-egl`, which CI now installs.
- Still missing (task 2.6): `xdg_popup.reposition` and per-kind positioners, clipboard,
  xdg-activation, AccessKit. Surrounding text waits for Masonry (task 2.14).
