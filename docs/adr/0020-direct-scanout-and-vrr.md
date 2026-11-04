# 0020: Direct scanout and adaptive sync (task 1.15c)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Every client buffer was copied into the compositor's swapchain, even a full-screen video
or game that covers the whole output: a copy per frame for nothing, and extra latency.
Smithay's DRM compositor can put a client buffer straight onto a plane when the frame
flags allow it, and test-commits each plane assignment before using it. Displays with
variable refresh (FreeSync, VESA Adaptive-Sync) can follow a game's frame rate instead of
the mode's fixed one; DRM exposes this as the connector's `vrr_capable` and the CRTC's
`VRR_ENABLED` property, which Smithay wraps (`vrr_supported`, `use_vrr`).

## Decision
1. **Primary plane only.** The tty backend renders with
   `ALLOW_PRIMARY_PLANE_SCANOUT | ALLOW_PRIMARY_PLANE_SCANOUT_ANY |
   ALLOW_CURSOR_PLANE_SCANOUT`. A buffer that covers the output and has nothing visible
   above it except the cursor (a full-screen window, ADR 0018) is scanned out; anything
   else is composited, as before. Any format may try: the atomic test commit rejects what
   the plane cannot show, and Smithay composites then. Overlay planes stay off: they break
   on NVIDIA (anvil works around it), and underlays and per-plane damage are not worth it
   for Phase 1.
2. **Per-surface dmabuf feedback.** Each output builds two feedbacks when it is lit: the
   default (the renderer's formats on the render node, also the global default) and a
   scanout one whose first tranche (flag `scanout`, target the GPU's primary node) holds
   the primary plane's formats and modifiers that the renderer can also import, so a
   buffer that stops fitting the plane can still be composited. After each render a
   surface gets the scanout feedback if Smithay scanned it out or tried to (format or
   test commit refused), else the default (`select_dmabuf_feedback`). Smithay only sends a
   feedback when it changes.
3. **`[outputs."NAME"] adaptive-sync = "off" | "on" | "fullscreen"`**, default `off`
   (kebab-case key like every other one; the plan's `adaptive_sync` was a typo).
   - `on`: variable rate whenever the connector can do it. If it needs a modeset to
     switch (`VrrSupport::RequiresModeset`), that one modeset happens when it is applied.
   - `fullscreen`: variable only while a full-screen window covers the output (the
     ADR 0018 rule) and the session is unlocked; needs a connector that switches without a
     modeset, else it stays off with a warning (a modeset per full-screen toggle would
     blank the display each time; `on` is the way out).
   - Unsupported connectors (and the nested and headless backends) warn once per change
     of the setting and stay fixed-rate. A failed `use_vrr` marks the connector as
     unsupported so it is not retried every frame.
   Support is read once when a connector is lit. The wanted state is computed before every
   render and staged with `use_vrr`; it goes out with the next queued frame.
4. **IPC (additive, ADR 0003):** `OutputInfo.adaptive_sync = { mode, supported, active }`
   (`#[serde(default)]` so older replies still parse). A switch emits `output_changed`.
   `neuectl outputs` appends `vrr off`, `vrr on (active)`, `vrr fullscreen (unsupported)`…
5. **Timing under VRR.** Presentation feedback reports `Refresh::Variable` with the mode's
   interval (the fastest the output goes) while VRR is active. The estimated vblank of an
   empty frame (ADR 0019) keeps using the mode's interval: with VRR a flip goes out as
   soon as it is queued, so callbacks of no-damage commits are paced at the maximum rate,
   which is what a client throttled by frame callbacks expects. The open point in ADR 0019
   is closed by this.

## Consequences
- Full-screen video and games skip the copy (presentation feedback shows `zero_copy`).
- Whether a plane accepts a buffer is up to the driver; it can only be confirmed on real
  hardware (`docs/checks/phase-1.md` §16).
- Overlay planes, buffer transforms for rotated outputs and tearing (async flips, ADR 0017)
  are later work. Several GPUs need feedback per GPU (later task).
