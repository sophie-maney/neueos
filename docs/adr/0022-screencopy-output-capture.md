# 0022: Screencopy of outputs with ext-image-copy-capture (task 1.16b)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Screenshots (`grim`), recorders (`wf-recorder`) and the screencast portal need a capture
protocol. PLAN.md picks `ext-image-copy-capture-v1` with `ext-image-capture-source-v1`
(not wlr-screencopy). 1.16b covers output sources; toplevel sources follow in 1.16c.
ADR 0021 left two questions open: how captures show the cursor, and how goldens deal
with llvmpipe differences between Mesa versions.

## Decision
1. **Smithay's helpers.** The pinned Smithay rev has `wayland::image_capture_source`
   (output and toplevel source managers) and `wayland::image_copy_capture` (sessions,
   frames, buffer checks against the constraints). We use them as they are; they do no
   drawing, which is ours (`screencopy.rs`). No new crates.
2. **Privileged.** Both globals are created with `is_unrestricted`: sandboxed clients
   don't see them (ADR 0017). The portal, which runs outside the sandbox, does.
3. **One offscreen buffer per session.** Each session keeps an `OffscreenOutput` (the
   renderbuffer and damage tracker the headless backend already used, moved to
   `render.rs` and shared). A frame draws the output's elements into it, then copies the
   result into the client's buffer. The damage reported is what changed since the
   session's previous frame. Since the client's buffer always gets the whole image, we
   ignore the client's `damage_buffer` hints and don't track buffer ages per client
   buffer.
4. **Frames wait for damage.** A frame that would report no damage waits, as the protocol
   allows (except for a session's first frame, which always has full damage). Backends
   call `State::capture_waiting_frames(output)` after drawing an output: tty after a
   queued frame, winit after each frame, headless on each frame-timer tick. A recording
   client therefore gets frames at the rate the screen changes, without spinning.
5. **Buffers.**
   - **shm:** ARGB8888 and XRGB8888 (same layout in memory). The frame is read back
     (`ExportMem`) and written row by row into the pool. This is the one `unsafe` block,
     a bounds-checked `copy_nonoverlapping` through the raw pointer Smithay hands out.
     No reference into client memory is ever made, and Smithay catches SIGBUS.
   - **dmabuf:** offered only by the tty backend (it has a render node), in
     ARGB/XRGB/ABGR/XBGR8888 with the modifiers the renderer can draw into. The frame is
     blitted (`glBlitFramebuffer`) into the client's dmabuf, then we wait for the GPU so
     the client can read it as soon as `ready` arrives. The headless and winit backends
     offer shm only.
6. **Geometry.** A capture is the output's current mode after its transform, drawn
   upright (`transform` event: normal) at the output's scale. A mode or transform change
   sends new constraints, and a waiting frame fails with `buffer_constraints`. When an
   output leaves the layout, its sessions get `stopped`. `report_outputs` is the hook,
   since every output change goes through it.
7. **Cursor.** With `paint_cursors`, captures include the cursor and drag icon exactly as
   the tty backend draws them: the pointer-element code moved out of `tty::render` into
   `tty::pointer_elements`, and `TtyElement` became the shared `render::ScreenElement`.
   On winit the host draws the cursor and on headless nothing does, so captures there
   never show one. Cursor sessions (`create_pointer_cursor_session`) are not supported:
   Smithay's default rejects them. The portal falls back to embedded cursors.
8. **Without a renderer** (headless without `Options::headless_renderer`) there are no
   constraints, so sessions are stopped straight away.
9. **Goldens** are blessed on Linux (`cargo xtask linux cargo xtask golden --bless`) and
   compared with a per-channel tolerance of 2, like neue-paint's. `neue-comp` joins
   `GOLDEN_CRATES`. On macOS its golden test compiles to nothing.

## Consequences
- Every frame of every session draws the output a second time (cheap with damage
  tracking, but a full draw per session start). Readback and the dmabuf blit block the
  compositor until the GPU is done. That's fine for screenshots and modest recording;
  async readback (fences on the event loop) is future work if recording at 4K shows up
  in profiles.
- The dmabuf path and cursor painting run only on tty, so CI doesn't cover them. The
  owner checks them in the VM with `grim` (1.16d).
- Rotated outputs get upright captures. That isn't tested yet, because headless outputs
  have no transform.

## Alternatives considered
- **Copy from the backend's own frame** (the scanout buffer). This saves a draw, but the
  frame includes the cursor whether or not the client asked for it, needs per-backend
  readback of GBM/EGL surfaces, and headless has no such frame.
- **Draw straight into the client's buffer.** That needs a damage tracker per client
  buffer (buffer ages) and a GL import of shm buffers as render targets, which GLES
  can't do. The intermediate buffer keeps one path for shm and dmabuf.
- **wlr-screencopy as well.** grim ≥ 1.5 and wl-mirror speak the ext protocol,
  but the wf-recorder packaged for Arch Linux ARM (checked in the VM, task 1.16d) only
  speaks wlr-screencopy and fails with "compositor doesn't support
  wlr-screencopy-unstable-v1". We can add wlr later if recording needs it.
- **Answer every frame at once.** This is simpler, but recording clients would then spin
  at full CPU on a static screen.
