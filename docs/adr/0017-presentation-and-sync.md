# 0017: Presentation and sync (task 1.14)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
Video players, games and toolkits pace frames with `wp_presentation` feedback; NVIDIA's
driver and recent Mesa want explicit sync; games ask for tearing; Flatpak gives sandboxed
apps their own socket through `wp_security_context_v1` and expects the compositor to hide
privileged globals from them. Smithay implements presentation, syncobj and security
context; tearing control it does not.

## Decision
1. **Globals:** `wp_presentation` (v2, `CLOCK_MONOTONIC`), `wp_tearing_control_manager_v1`
   (our own small implementation on Smithay's `Dispatch2`), `wp_security_context_manager_v1`,
   and `wp_linux_drm_syncobj_manager_v1` on the tty backend only, only when the GPU supports
   `syncobj_eventfd` (Linux 6.6; Smithay's `supports_syncobj_eventfd`). Code:
   `presentation.rs`, `sync.rs`, `security.rs`.
2. **Presentation feedback** follows the output that shows a surface:
   - **tty:** after rendering, surfaces are marked with their primary scan-out output
     (Smithay's render-element states). When a frame is queued, the feedback of the
     surfaces it shows travels with it as the DRM frame data and is marked presented at its
     vblank, with the kernel's monotonic timestamp and sequence (`Vsync | HwClock |
     HwCompletion`), or our clock and `Vsync` when the kernel gives none. A render without
     damage queues nothing and leaves the feedback for a later frame.
   - **winit:** presented when the frame is submitted to the host (`Vsync`); the host does
     not tell us more.
   - **headless:** the frame timer presents what each output paces (same rule as frame
     callbacks) with no flags and a tick counter as the sequence.
   - Refresh is the mode's interval truncated to whole nanoseconds. Superseded feedback is
     discarded by Smithay when the next commit lands; feedback of destroyed surfaces too.
3. **Tearing hints** are double-buffered per surface (`TearingCachedState`) and readable
   with `State::wants_tearing`. They are **not acted on**: Smithay's DRM compositor has no
   asynchronous page flips at our revision, so every flip waits for vblank. A second
   control on one surface is the `tearing_control_exists` error; destroying the control
   reverts to vsync on the next commit.
4. **Buffer sync:** a pre-commit hook on every surface blocks a commit with a dmabuf until
   it is ready: on its syncobj acquire point when the client set one, else on the
   dmabuf's implicit fences (`Dmabuf::generate_blocker`). Each blocker has a calloop source
   that clears it, so the loop never waits on the GPU. Release points are signalled by
   Smithay when the buffer is released. shm buffers are not blocked.
5. **Security context:** a client that connects through a context's listener carries the
   `SecurityContext` in its `ClientState`. `security::is_unrestricted` is the filter of the
   privileged globals: `wp_security_context_manager_v1` itself (no nesting),
   `ext_session_lock_manager_v1`, `zwlr_layer_shell_v1` (a sandboxed app could cover the
   screen or fake a lock screen), `ext_data_control_manager_v1` and
   `zwlr_data_control_manager_v1` (clipboard snooping), `zwp_input_method_manager_v2`
   (keystrokes) and `ext_foreign_toplevel_list_v1` (other apps' titles). Every privileged
   global added later (screencopy in 1.16, virtual input, output management) must use the
   same filter. Visible on purpose: idle notify and inhibit, keyboard-shortcuts inhibit
   (VM and remote-desktop apps), pointer constraints, xdg-foreign (portals), activation,
   syncobj and presentation. A listener lasts until the sandbox closes `close_fd`.

## Consequences
- `State` gains `clock`, `presentation_state`, `tearing_control_state` and
  `security_context_state`; `ClientState` gains `security_context`; `TtyData` gains
  `syncobj_state` (the `DrmSyncobjHandler` reads it from the backend).
- The tty `FrameData` is now `OutputPresentationFeedback`.
- Implicit-sync blocking is new for all backends: a client whose GPU work is slow now
  shows its previous frame instead of a half-drawn one.
- 9 headless tests in `tests/presentation.rs` on a new `presentation.rs` test-client module
  (feedback, tearing, security context, no syncobj without DRM). No new dependencies.

## Not done
- Tearing (asynchronous flips): waits for Smithay support; then only for a full-screen
  surface on its own plane (direct scanout, task 1.15).
- Explicit sync could not be exercised headless or in the OrbStack VM (no DRM device);
  the owner checks it on the NVIDIA PC (`docs/dev-setup.md` §13).
- No per-app policy on the security context's `app_id` (e.g. letting a trusted sandboxed
  app keep a privileged global); `neuectl` does not show which clients are sandboxed.
- Feedback for the cursor and drag-icon surfaces is not collected (they rarely ask).
- Variable refresh (`Refresh::Variable`) comes with VRR (task 1.15).

## Alternatives considered
- **Advertising syncobj everywhere and failing per import:** clients would pick explicit
  sync and then stall; Smithay's docs and other compositors gate it on
  `syncobj_eventfd`.
- **Taking feedback even when a render has no damage** (anvil does): feedback of an
  unchanged surface would claim a frame that showed nothing new. Leaving it pending until
  a real frame or the next commit is closer to the spec.
- **Hiding everything but a core list from sandboxed clients:** too blunt; Flatpak apps
  legitimately need idle inhibit, shortcuts inhibit and pointer constraints.

## Revisit when
- Smithay's DRM compositor gains asynchronous page flips (act on tearing hints).
- Direct scanout and VRR land (task 1.15): zero-copy flags and variable refresh.
- Screencopy lands (task 1.16): its globals take the privileged filter.
