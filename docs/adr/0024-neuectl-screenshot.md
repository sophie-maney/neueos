# 0024: `neuectl screenshot` (task 1.16d)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
ADR 0003 lists `screenshot` among the Phase 1 requests ("writes a PNG to a client-given
absolute path; encoding runs off the event loop") and ADR 0007 deferred it to task 1.16.
The screencopy sessions of ADRs 0022 and 0023 serve Wayland clients; a `neuectl` request
has no Wayland client to talk to. Replies had so far always been written synchronously, in
the handler that parsed the request.

## Decision
1. **Request.** `{"request": "screenshot", "params": {"path": "/abs/file.png", "output":
   "NAME", "cursor": true}}`. `path` is required and absolute (`invalid_params`
   otherwise: the compositor's working directory is not the client's, so `neuectl`
   makes relative paths absolute itself). `output` defaults to the first output.
   `cursor` defaults to false and has the meaning of `paint_cursors` in ADR 0022: it
   only does something on the tty backend. Reply: `{path, output, width, height}`, sent
   once the file is written. An existing file is replaced.
2. **Error codes (additive).** `unknown_output` and `capture_failed` (no renderer, the
   output has no mode, drawing or reading back failed, or the file could not be written;
   the message says which).
3. **Same drawing code as screencopy.** `State::capture_output_now` draws the output into
   a fresh `OffscreenOutput` and reads it back, with the element list shared with
   screencopy sessions (`output_capture_elements`). So a screenshot is pixel-identical to
   an `ext-image-copy-capture` frame of the same output, and the golden of 1.16b covers
   both. The draw and readback run on the event loop, like a screencopy frame (ADR 0022
   consequences).
4. **PNG encoding and the write run on a thread.** The pixels move to a short-lived
   `neue-screenshot` thread, which converts them, encodes with `neue-paint`'s
   `Pixmap::into_png`, writes the file and reports through a calloop channel. No new
   dependency: `png` is already in the tree through `vello_common`, and `neue-paint` was
   already a compositor dependency. Captures are opaque, so the PNG is RGB.
5. **Deferred replies.** A connection with a screenshot in flight is marked `waiting`:
   requests already buffered, or sent meanwhile, are left unread in its line buffer
   until the reply is queued, then answered in order. This keeps ADR 0003's "exactly one
   reply per request, in order" without ids reordering. Other connections are unaffected.
   If the client disconnects meanwhile, the finished file stays and the reply is dropped.
6. **Limits.** Images are at most 65535 pixels per side (`Pixmap` uses `u16`); larger
   outputs fail with `capture_failed`. Toplevel screenshots are not offered: scripts
   needing one use `grim` with `-g` from `neuectl windows`, or a Wayland client.

## Consequences
- `cargo xtask` and agents can verify visual changes with one command, and
  `neuectl screenshot` is covered by CI: `neuectl_screenshot` in `tests/golden.rs`
  compares the PNG with the 1.16b golden, and `tests/ipc.rs` covers errors and reply
  ordering.
- The deferred-reply mechanism is reusable for other slow requests.
- The `cursor` path on the tty backend, and dmabuf captures, are only checked by the
  owner in the VM (`docs/dev-setup.md` §17).

## Alternatives considered
- **Encode on the event loop.** Simple, but a 4K PNG takes tens of milliseconds and
  ADR 0003 forbids blocking the loop.
- **Have `neuectl` act as a Wayland screencopy client.** It would need a Wayland
  connection and a PNG encoder in the CLI, and could not run over the socket alone
  (SSH, tests, agents).
- **Ids and out-of-order replies.** The client allows one request in flight, so a
  waiting connection costs nothing, and ordered replies stay the documented rule.
- **A `png` dependency in `neue-comp`.** Not needed; `Pixmap::into_png` is enough.
