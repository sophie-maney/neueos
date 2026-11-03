# 0009: Compositor config details (task 1.6)

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
ADR 0004 fixed the format, layering, error policy and reload model. Task 1.6 implements
them for `compositor.toml` and needed a few choices that ADR leaves open.

## Decision
1. **Crate:** `neue-config` (portable) holds the loader and one module per file
   (`compositor` so far). Its only new dependency is `toml` 1.x (MIT OR Apache-2.0, already
   approved in ADR 0004). `toml_edit` waits for `neue-settingsd`. The crate does no logging;
   `load` returns `Loaded { config, unknown_keys }` or a `ConfigError` with file, line and
   column.
2. **Defaults are the lowest layer.** The loader serialises `T::default()` and merges the
   file layers over it. Without this a `[keybindings]` table in a file would replace the
   default bindings instead of adding to them.
3. **Validation per layer.** Each layer is deserialised on its own (every field has a
   default, so a partial file works), which gives errors a position in the right file; the
   merged table is deserialised after that. Ranges are checked in serde (`deserialize_with`),
   so they carry positions too. Anything only the compositor can judge (key names, action
   names, whether xkb can build the keymap) is checked in `neue-comp` before anything is
   applied; those errors name the user's file without a position.
4. **Unknown keys** are found by comparing the input with the re-serialised config. Keys
   accepted only through `#[serde(alias)]` would be reported wrongly; the check needs
   alias support before the first alias is added.
5. **Key bindings:** `[keybindings]` maps chords (`"Super+Shift+Q"`) to action names from
   `neuectl actions`. Chord keys are rewritten to a canonical spelling before layers merge,
   so `"super+q"` in one layer overrides `"Super+Q"` in another. The value `"none"` removes
   a binding made by a lower layer. A binding takes its key's release with it, so clients
   never see a release without a press. VT switching (Ctrl+Alt+Fn) is not configurable.
   Letters match case-insensitively (ASCII only) against the layout's shifted keysym.
   Defaults: `Super+Q` = `quit`.
6. **Watching:** inotify through `rustix` (already a dependency, `fs` feature, no new
   crate) as a calloop source on each existing layer directory, `CLOSE_WRITE`, `MOVED_TO`,
   `MOVED_FROM`, `CREATE` and `DELETE`, filtered by file name. The first event arms a 100 ms
   timer; later events in that window are absorbed. The user directory is created if
   missing so a first file is noticed.
7. **Protocol (additive to ADR 0003/0007):** request `reload-config` and a same-named
   action (so it can be bound); error code `config_error`; topic `config` with events
   `config_error` (`file`, optional `line` and `column`, `message`) and `config_reloaded`
   (`file`), the latter so the shell can dismiss an error and tests can wait without
   sleeping. `neuectl reload-config`; `neuectl subscribe` now streams both topics.
8. **Applying:** only what changed. `[keyboard]` goes through `set_xkb_config` (which
   leaves the old keymap if the new one fails) and `change_repeat_info`. `[pointer]` is
   read on every scroll event (`natural-scroll` flips the sign, `scroll-factor`
   multiplies). `[outputs."NAME"] scale` sets the output's scale; real per-output scaling
   (fractional-scale-v1, viewporter, hot-plug) is task 1.7. Options carry a `Layers`
   value, so tests point the compositor at a temporary directory and never read the real
   config.
9. **Start-up:** a config that fails to load leaves the built-in defaults active. The error
   is logged and emitted, but a subscriber that connects later does not see it (the shell
   will need a way to ask; revisit with Phase 3).

## Consequences
- libinput-level pointer settings (acceleration, tap-to-click, left-handed) are not in
  the schema yet: they need device handling in the tty backend and a hardware check.
- Setting `XKB_DEFAULT_*` in the environment no longer affects the keyboard: the config
  always names its layout (default `us`).
- The compositor writes nothing but the (empty) user config directory.

## Alternatives considered
- **`notify` crate:** a new dependency for one inotify watch; rustix already has the
  syscalls and the code is about 40 lines.
- **`serde_ignored` for unknown keys:** exact, but another dependency for a warning.
- **Replace-on-conflict for `[keybindings]` (arrays style):** a user file would have to
  repeat every default binding.
