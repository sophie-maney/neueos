# 0075: The compositor, the shell and the cursor read `appearance.toml`

- **Status:** Accepted
- **Date:** 2026-11-05
- **Builds on:** 0004 (config format), 0009 (compositor config), 0012 (cursors), 0074 (settingsd)

## Context
ADR 0074 made `appearance.toml` the one place for dark mode, accent, contrast, reduced
motion, fonts and the cursor, but left three readers for a follow-up: the compositor's
title bars, the shell, and the compositor-drawn cursor (still `XCURSOR_THEME`/`XCURSOR_SIZE`).

## Decision
1. **`AppearanceConfig::variant()` and `::theme()`** in `neue-config` map the file's
   `mode`, `accent`, `contrast` and `reduced-motion` to `neue_theme::Variant` and
   `Theme`, so every reader (and later every app) converts the same way. This adds the edge
   `neue-config → neue-theme`; `neue-theme` is a leaf (it only depends on `color`), so
   there is no cycle and `neue-config` stays portable.
2. **Compositor:** `ConfigRuntime` holds the applied `AppearanceConfig`. The inotify watcher
   that already covers the layer directories now tells `compositor.toml` and
   `appearance.toml` apart, and a debounced reload re-reads only the file(s) that changed.
   `neuectl reload-config` re-reads both. A changed appearance sets `Chrome::variant` (title
   bars re-render, their cache is keyed by style) and, on the tty backend, switches the cursor
   theme and size; then every output is redrawn. `config_reloaded` is sent with
   `file: "appearance.toml"` only when the value changed; a rejected file keeps the previous
   look and sends `config_error`, as for `compositor.toml`.
3. **Cursor:** `[cursor] theme/size` replace the defaults. `XCURSOR_THEME` and
   `XCURSOR_SIZE`, when set, still win (a dev override; nothing sets them in a session).
   The winit backend leaves the cursor to the host window system.
4. **Shell:** reads `appearance.toml` before the event loop starts, so the first frame has
   the right theme, and a background thread watches the layer directories with inotify
   (`rustix`, already in the tree) and sends the new config as a message; the shell then
   calls `Cx::set_theme`, which repaints every window. No D-Bus client is needed; the
   settings client comes with Preferences (3.24).

## Consequences
- Setting `appearance.mode` through `neue-settingsd` (or editing the file) darkens the
  title bars and the shell's bar and menus without a restart.
- `reduced-motion` is read into the shell's `Theme`; the compositor's own animations don't
  consult it yet.
- Other kit apps still start light: they need the same two calls (`start_theme`, follow
  edits). That belongs in `neue-kit`'s `run` and is a separate task.
- Tests: `neue-config` (mapping), `neue-comp` `tests/config.rs` (hot reload, broken file
  keeps the look), `neue-shell` (watcher).

## Alternatives considered
- **Subscribing to `org.neueos.Settings1.Changed` in the shell and compositor:** needs a
  D-Bus client in the compositor (single-threaded calloop) and ties the session to
  settingsd running (it isn't started yet, 3.27); the file is always there.
- **The conversion in each consumer:** three copies, and every app needs it.
