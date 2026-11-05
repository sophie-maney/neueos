# 0074: neue-settingsd v0: `org.neueos.Settings1`, `appearance.toml`, GTK and dconf

- **Status:** Accepted
- **Date:** 2026-11-05
- **Builds on:** 0004 (config format), 0009 (compositor config), 0047 (zbus runtime)

## Context
Task 3.6. PLAN.md §3.4 makes `neue-settingsd` the settings source of truth on D-Bus, and
ADR 0004 makes it the only writer of the neueOS TOML files. Preferences (3.24), the portal
(3.21) and the shell need one interface to read, change and follow settings, and GTK, GNOME
and Qt apps read their look from `settings.ini` and dconf, not from our files. Open points:
the key model, the interface, where appearance lives, the runtime, how dconf is written,
and what Preferences uses on macOS.

## Decision
1. **`appearance.toml` joins `neue-config`** (the file ADR 0004 §2 planned):
   `mode` (light, dark), `accent` (blue, graphite), `contrast` (standard, increased),
   `reduced-motion`, `[fonts]` (`ui`, `ui-size`, `monospace`, `monospace-size`; DejaVu,
   10 pt) and `[cursor]` (`theme`, `size`; `default`, 24, as ADR 0012). Dark mode lives
   here, not in `compositor.toml`: every component reads it. `neue_config::load_with_user`
   loads a file with given text as its user layer, for checking an edit before it is
   written and for the in-memory store.
2. **Keys are the schema's own paths**, not a hand-kept list: the file name without
   `.toml`, then the dotted TOML path (`appearance.mode`,
   `compositor.pointer.natural-scroll`, `compositor.keybindings."Super+Q"`). A set edits
   the user file with `toml_edit` (comments, order and a replaced value's decor survive),
   re-loads every layer with the edited text and rejects the edit if it fails the schema
   (`InvalidValue`) or the key comes back unknown (`UnknownKey`); only then is the file
   written (temp file and rename). An integer for a float key is converted. A user file
   that doesn't parse is never overwritten (`BrokenFile`). `Reset` removes the key from
   the user file, so a lower layer or the default applies. Every key in every file is
   served; new schema fields become keys with no change here.
3. **Interface** `org.neueos.Settings1` at `/org/neueos/Settings`, name
   `org.neueos.Settings` on the session bus:
   - `Get(s key) → v`: the effective (merged) value; a table gives `a{sv}`. An optional
     key without a value is `UnknownKey`.
   - `GetAll() → a{sv}`: every key with a value, flattened to leaves.
   - `Set(s key, v value)`, `Reset(s key)`.
   - signal `Changed(a{sv} changed, as unset)`: sent after a set or reset, and after a
     hand edit of any layer's file (inotify on the layer directories, 100 ms debounce, as
     in neue-comp), so Preferences and the shell follow edits from anywhere.
   - errors `org.neueos.Settings1.Error.{UnknownKey, InvalidValue, BrokenFile, Io}`.

   Values: `b`, `x`, `d`, `s`; arrays `av`; tables `a{sv}`. Any integer type and nested
   variants are accepted on `Set`.
4. **GTK:** `settings.ini` in `$XDG_CONFIG_HOME/gtk-3.0/` and `gtk-4.0/`, `[Settings]`
   `gtk-application-prefer-dark-theme`, `gtk-font-name`, `gtk-cursor-theme-name`,
   `gtk-cursor-theme-size`, `gtk-enable-animations`. Only these lines are ours; other
   lines are kept, files that wouldn't change aren't written.
5. **dconf** through `ca.desrt.dconf.Writer.Change` on the session bus (the `dconf`
   package's D-Bus-activated service; no C library): `org.gnome.desktop.interface`
   `color-scheme` (`default`, `prefer-dark`), `accent-color` (`blue`; graphite is
   `slate`), `enable-animations`, `font-name`, `monospace-font-name`, `cursor-theme`,
   `cursor-size`, and `org.gnome.desktop.a11y.interface high-contrast`. The changeset
   (GVariant `a{smv}`, little-endian, normal form) is encoded by hand in about 80 lines:
   zvariant's GVariant support is a feature we would only need for this, and the encoder
   is tested byte for byte against the spec's framing rules.
6. **Outputs follow the TOML.** GTK and dconf are written when an appearance key changes
   and once at start-up (the files may have been edited while the service was down).
   They are never read back (ADR 0004). Failures are logged, not returned: the TOML
   write already succeeded.
7. **Runtime:** tokio (current-thread; `rt`, `macros`, `sync`, `time`; MIT, already in
   the tree) for the service, as AGENTS.md asks, but **zbus stays on its async-io
   executor**. ADR 0047 allowed services zbus's `tokio` feature; in practice Cargo
   unifies features across a workspace build, so it would switch zbus to tokio for
   neue-kit apps and neue-dbus too, which have no tokio runtime. New workspace
   dependencies: `toml_edit` 0.25 (ADR 0004 approved it), `tokio` 1.54,
   `futures-util` 0.3 (tests only); all MIT or MIT OR Apache-2.0 and already in
   `Cargo.lock`.
8. **Portable library, Linux service.** `neue_settingsd::Store` (keys, edits, reload),
   `gtk` and `dconf` build and test on macOS; `daemon` and the service binary are
   Linux-only (on macOS the binary says so). **The mock is `Store::in_memory()`**: schema
   defaults and a user layer in memory, same keys, validation and errors, nothing read or
   written. Preferences (3.24) puts it and a D-Bus client behind one app-facing type;
   that client is part of 3.24.

## Consequences
- Tests: 21 on macOS (keys, store, GTK, dconf bytes, `appearance.toml`), plus 2 on Linux
  over a peer-to-peer connection (Get, Set, `Changed`, errors, Reset, GetAll, values).
  In the VM: `busctl --user call … Set sv appearance.mode s dark` writes
  `appearance.toml`, both `settings.ini` files and dconf (`gsettings get
  org.gnome.desktop.interface color-scheme` is `'prefer-dark'`); a compositor key set the
  same way reloads the compositor (`config_reloaded`); a hand edit sends `Changed`.
- The compositor's title bars, the shell and the cursor read `appearance.toml` since ADR
  0075.
- The service isn't started by the session yet; D-Bus activation or a user unit comes
  with 3.27.
- Qt/Kvantum config and the portal's colour scheme (PLAN.md §3.4) come with the Kvantum
  theme and 3.21.
- Setting a key's value to what a lower layer already gives still writes it into the user
  file (it pins the value); `Reset` is the way back to "follow the system".

## Alternatives considered
- **A hand-kept key list with types:** duplicates the schema and drifts from it; the
  schema plus `load` already knows every type and range.
- **One signal per key (`Changed(s, v)`):** a hand edit of several keys would arrive as
  a burst; one batched signal like `PropertiesChanged` is easier to apply at once.
- **D-Bus properties per setting:** keys aren't fixed (keybindings, outputs) and
  properties can't report validation errors well.
- **The `dconf` command-line tool or the dconf C library:** a process per change, or C;
  the writer interface is two methods.
- **zbus's `tokio` feature in the service:** see §7.
