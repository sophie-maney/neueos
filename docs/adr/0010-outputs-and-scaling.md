# 0010: Outputs, layout, hotplug and scaling (task 1.7)

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
Until 1.6 every backend had exactly one output at `(0, 0)`, and `[outputs."NAME"] scale`
only changed the number `wl_output` reported. Task 1.7 needs several outputs, a layout,
hotplug, real per-output scaling (fractional-scale-v1, viewporter) and windows that move
between outputs. Several rules had to be chosen.

## Decision
1. **One place for outputs.** Backends create `Output`s and call `State::add_output`,
   `State::remove_output` and `State::output_mode_changed` (module `outputs.rs`). The state
   owns the `wl_output` globals, the layout, scales, window rescue and IPC events, so the
   rules are the same on every backend and the headless tests cover them. A removed
   output's global is disabled at once and destroyed 10 s later, so clients binding it in
   between don't get a protocol error.
2. **Layout.** `[outputs."NAME"] position = [x, y]` (new, logical pixels, each in
   -32768..32767) places an output. Outputs without one go to the right of everything
   placed so far, in connection order, top edges aligned with the topmost output. Overlaps
   between configured positions are allowed; it's the user's layout. Re-laid out after
   every hotplug, mode change and config reload.
3. **Scale.** `[outputs."NAME"] scale`, else 1 (no automatic DPI guess yet: EDID needs
   libdisplay-info, a C library). Each window belongs to the output it overlaps most (its
   *primary output*). All its surfaces, popups included, get that output's scale through
   `wp_fractional_scale_v1.preferred_scale` and `wl_surface.preferred_buffer_scale` (rounded
   up). Windows not yet mapped get the scale of the output under the pointer, where they
   will appear. Scales are refreshed once per event-loop iteration; Smithay only sends
   changes. `wp_viewporter` is advertised (Smithay applies it when rendering). Preferred
   buffer transform stays `normal` until direct scanout (1.15) makes it worth anything.
4. **Frame callbacks** go to a window only from its primary output, so it is paced by one
   display. Windows on no output get them from the first output so they don't stall.
5. **Windows between outputs.** Interactive and `move-window` moves keep the title bar
   inside the work area of the output that the middle of the window's top edge lands on
   (or the nearest output), instead of the window's current output. Popups are
   constrained to the primary output.
6. **Hotplug.** Windows whose primary output is removed move to the first remaining output
   at the same offset from its corner, clamped into its work area. After any layout change,
   zoomed windows are refitted to their output's work area, and windows on no output are
   clamped onto the first one (minimised windows too, so they come back visible). A pointer
   left on no output is put in the middle of the first one.
7. **Pointer.** Relative motion that leaves the layout lands on the nearest output's edge
   (so the pointer never sits in a gap). Absolute devices (tablets, VM pointers) map onto the
   bounding box of all outputs, then onto the nearest output.
8. **tty.** One GPU, every connected connector at its preferred mode on its own CRTC, each
   with its own render and vblank cycle. A udev `change` event on the GPU (and session
   resume) rescans connectors: new ones are lit, unplugged ones dropped (Smithay turns the
   CRTC off and modifiers are restored), and a changed preferred mode, such as a resized VM
   window, is applied with `use_mode`. CRTCs come from the connector's current encoder if
   free, else the first free compatible one. The cursor is drawn on every output it is
   within 128 px of.
9. **Headless.** Starts with `HEADLESS-1`; `State::headless_add_output`,
   `headless_remove_output` and `headless_set_mode` simulate hotplug for tests.
10. **Protocol (additive to ADR 0003/0007):** topic `outputs` with `output_added`
    (an `OutputInfo`), `output_changed` (an `OutputInfo`, after mode, scale or position
    changes) and `output_removed` (`name`). They are derived by diffing against the last
    report, so each change is sent once. `neuectl subscribe` streams this topic too. The
    `Event` type loses `Eq` (an `OutputInfo` holds an `f64`).

## Consequences
- Windows that end up partly off an output after a scale or mode change stay there as long
  as part of them is on an output. OS X pushes them fully on screen; revisit if it bothers
  in practice.
- Output names are connector names, so moving a monitor to another port loses its settings.
  Matching by EDID make/model/serial waits for libdisplay-info (ADR needed).
- Mode selection, rotation, mirroring and disabling outputs are not configurable yet
  (`[outputs]` will grow `mode`, `transform` and `enabled`). wlr-output-management for
  `wdisplays`-style tools comes with the Displays settings pane.
- Several GPUs (laptops with hybrid graphics) remain task 1.15.

## Alternatives considered
- **`smithay-drm-extras`' `DrmScanner`** for connector diffing: another dependency for
  about 60 lines; it also wants libdisplay-info for EDID.
- **Highest scale among overlapped outputs** (sway) instead of the largest overlap: sharper
  on a straddling window but renders more pixels; OS X assigns a window to one display, which
  matches the largest overlap.
- **Tracking the primary scanout output from render feedback** (anvil) for frame callbacks
  and scale: more precise with occlusion, but needs per-element render state that the
  headless backend doesn't have.
