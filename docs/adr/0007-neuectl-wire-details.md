# 0007: `neuectl` wire details (task 1.3)

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
ADR 0003 fixes the channel, framing and envelope of the `neuectl` protocol but leaves
open how windows are identified, which events and error codes exist, how actions are
described, and how the compositor serves connections without blocking. Task 1.3 had to
choose. Additive changes to these are allowed by ADR 0003; this records the first set.

## Decision
- **Window ids:** `u64` starting at 1, assigned when a toplevel is created (stored in the
  Smithay `Window` user data), never reused within a compositor run. JSON: a bare number.
- **Types:** `WindowInfo { id, title, app_id, x, y, width, height, focused }` (flat
  integers, not tuples), `OutputInfo { name, make, model, x, y, width, height,
  refresh_mhz, scale }`, `VersionInfo { compositor, version, protocol }`,
  `ActionInfo { name, description, window }`. `window` is `none`, `optional` (defaults to
  the focused window) or `required`.
- **Requests in 1.3:** `version`, `windows`, `outputs`, `focused-window` (`null` if none),
  `actions`, `action`, `subscribe`, `quit`. `spaces`, `reload-config` and `screenshot`
  from ADR 0003 arrive with their tasks (`screenshot`: ADR 0024); until then they are `unknown_request`.
- **Actions in 1.3:** `quit`, `close-window` (sends `xdg_toplevel.close`; the client
  decides), `focus-window`. Window-management actions join in 1.5.
- **Topics and events:** one topic, `windows`, with `window_opened` (a `WindowInfo`),
  `window_closed {window}` and `focus_changed {window|null}`. Events are sent after the
  state change. A focus event is sent only when the focused window actually changes.
  No `title_changed` yet: Smithay has no title callback, and nothing needs it.
- **Error codes:** `invalid_request` (bad JSON, no `id` or `request`; reply `id` is `null`
  if none), `unknown_request`, `invalid_params`, `unknown_action`, `unknown_window`,
  `no_window`, `line_too_long` (connection is then closed), `internal`. Clients map codes
  they don't know to `other`.
- **`quit`:** replies `null`, then stops the loop; the reply is written before exit.
- **Serving:** the listener (level-triggered) and each connection (edge-triggered, read
  and write interest) are calloop sources; callbacks read until `WouldBlock`, answer every
  complete line, and write until `WouldBlock`; the next writable edge resumes a partial
  write. A peer with more than 1 MiB of unsent output is dropped (ADR 0003's "falls behind"
  rule, applied to every connection).
- **Socket file:** the directory is created or reset to 0700 and the socket to 0600. A
  socket file nobody answers on is replaced; one that answers makes start-up fail. It is
  removed on exit. `NEUE_SOCKET` is exported to spawned children, and removed from their
  environment when the compositor has no socket.
- **Client:** `neue-ipc::Client` is blocking, one request in flight, replies checked
  against the request id. Subscribing consumes the client and yields an event iterator.
- **`neuectl`:** text output by default, `--json` for the raw payload; `subscribe`
  (alias `events`) prints one JSON line per event in both modes. `--socket` overrides the
  path. Parsing uses `clap` (derive); `serde`, `serde_json` and `tempfile` (dev only) are
  also new, all pure Rust and MIT/Apache.

## Consequences
- Tests, agents and `cargo xtask vm` drive the session with these names; renaming one is a
  protocol break.
- The compositor depends on `neue-ipc` and `serde_json`; no async runtime or zbus.
- Window ids are not stable across restarts. Scripts should look windows up by `app_id`
  and `title`.

## Alternatives considered
- **Using the Wayland object id as the window id:** reused after destruction, so a stale
  id could hit the wrong window.
- **Level-triggered connections with write interest toggled on demand:** calloop's
  `Generic` doesn't let a callback change its interest, so edge-triggered is simpler.
- **A thread per connection:** breaks the single-threaded rule for state access (ADR 0001).
