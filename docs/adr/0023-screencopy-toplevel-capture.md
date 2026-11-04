# 0023: Screencopy of toplevels with ext-image-copy-capture (task 1.16c)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
1.16b added output capture (ADR 0022). Screen recorders and the screencast portal also
capture single windows through `ext-foreign-toplevel-image-capture-source-v1`. The
foreign toplevel list already exists (1.12), with identifiers of the form `window-<id>`.

## Decision
1. **Smithay's `ToplevelCaptureSourceState`** provides the global, with the same
   `is_unrestricted` filter as the other capture globals (ADR 0017). The source stores
   the toplevel's weak `ForeignToplevelHandle`; the window is found again through its
   identifier (`window-<id>` to `State::window_by_id`), so mapped and minimised windows
   both work.
2. **One session type, two subjects.** A session's `Subject` is an output or a toplevel;
   `resolve` turns it into what to draw and the capture size, or `None` if it is gone.
   Sessions, frames, damage tracking and the shm/dmabuf copy are shared with output
   captures.
3. **What a window capture shows.** The window's surfaces (toplevel, subsurfaces and
   popups) with the top-left corner of its window geometry at the origin, on a
   transparent background (`OffscreenOutput::with_clear`). No title bar and no cursor
   (`paint_cursors` is ignored), and nothing that overlaps the window.
   Popups that stick out of the geometry are cut off.
4. **Size and scale.** The window geometry's size, scaled by the scale of the first
   output the window is on (any output when minimised). A size change sends new
   constraints and fails the waiting frame with `buffer_constraints`, like a mode change
   on an output. We notice it when the session next captures, not eagerly.
5. **Frames wait for damage.** The backends already call `capture_waiting_frames(output)`
   after drawing an output; it now also retries the frames of windows on that output
   (every output's draw for a minimised window).
6. **End of a session.** When the window goes (`retire_foreign_toplevel`), its sessions
   are dropped, which sends `stopped`.

## Consequences
- A window's pixels are drawn a second time per session, like outputs.
- A minimised window that does not commit sends no frames after the first. That is
  correct: nothing changed.
- Windows whose size is zero (no buffer yet) cannot be captured, so such a session is
  stopped straight away.

## Alternatives considered
- **Crop the output capture to the window.** That would show windows above it, and
  nothing for minimised windows or windows outside the output.
- **Include the title bar.** The portal and recorders expect the client's content, as on
  other compositors. We can add a bar-inclusive option later.
