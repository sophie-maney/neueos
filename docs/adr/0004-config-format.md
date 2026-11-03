# 0004: Configuration format and reload

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
PLAN.md §3.1 and AGENTS.md already say: TOML in `~/.config/neueos/`, schema in
`neue-config`, every field documented, new fields with defaults, hot reload, and
Preferences writing through `neue-settingsd`. Open points: file layout, layering with
system defaults, what happens on errors, how writes preserve hand edits, and how readers
notice changes.

## Decision
1. **Format:** TOML 1.0, parsed with `toml` (serde) and edited with `toml_edit`. Both are
   MIT OR Apache-2.0.
2. **Files, one per domain**, so a change reloads only what it affects:
   | File | Contents | Readers |
   |---|---|---|
   | `compositor.toml` | outputs, input (keyboard layouts, pointer, touchpad), keybindings, window management, hot corners, Overview | `neue-comp` |
   | `shell.toml` | menu bar, dock, notifications, launcher, widgets | `neue-shell` |
   | `appearance.toml` | light/dark/auto, accent, fonts, reduced motion, contrast | everyone |

   More files only when a new domain needs one (e.g. `search.toml` for `neue-indexd`).
3. **Layering:** built-in defaults (in the schema) < `/usr/share/neueos/<file>` (distro) <
   `/etc/neueos/<file>` (admin) < `$XDG_CONFIG_HOME/neueos/<file>` (user). Tables merge
   key by key; arrays replace. A missing file at any level is fine.
4. **Schema:** `neue-config` (portable) defines one serde struct per file. Every field has a
   doc comment, a default (`#[serde(default)]`) and a stable kebab-case key. Renames keep
   the old key with `#[serde(alias)]`. There is no version field; the schema only grows.
   Enums use kebab-case strings.
5. **Errors never stop the session:**
   - Unknown keys: logged as warnings (so newer configs work on older builds).
   - A file that doesn't parse, or has a value of the wrong type or out of range: the whole
     file is rejected and the last good config stays active (or defaults at startup),
     the error with line and column is logged, and the compositor emits a `config_error`
     event (ADR 0003) so the shell can show a notification.
6. **Writes:** `neue-settingsd` is the only program that writes these files. It edits the
   user file with `toml_edit` so comments and ordering survive, writes to a temp file in the
   same directory, and renames it into place. Users may still hand-edit the files.
7. **Reload:** readers watch the **directories** (not the files, so renames are caught) with
   inotify, debounce for 100 ms, re-parse, diff against the active config, and apply only
   what changed. In the compositor the watcher is a calloop source (no blocking).
   `neuectl reload-config` forces a reload.
8. **Apps:** first-party apps don't read the files. They get appearance through
   xdg-desktop-portal's Settings interface (`org.freedesktop.appearance` plus an
   `org.neueos.appearance` namespace served by our portal), which also works inside
   Flatpak. On macOS, apps use built-in defaults or a mocked settings source.
9. **Docs:** the default config for each file is generated from the schema (doc comments
   become TOML comments) by an `xtask` subcommand and shipped to `/usr/share/neueos/`.

## Consequences
- The compositor can start and run without `neue-settingsd`.
- Keybindings are TOML tables (`"Super+Q" = "quit-app"`); action names are the same as
  `neuectl action` names, so one list serves both.
- Merging layers means "remove a default list entry" needs replacement semantics; arrays
  replace, which is simple and documented.
- GTK `settings.ini`, dconf, Qt/Kvantum config are outputs that `neue-settingsd` derives
  from these files; they are never read back.

## Alternatives considered
- **KDL (niri), RON:** fewer tools and editors know them; TOML is already the plan and is
  familiar from Cargo.
- **One `neueos.toml`:** every edit would wake every reader, and one syntax error would
  break all components at once.
- **dconf/GSettings as the store:** C library and binary database; not hand-editable.
- **D-Bus-only settings (settingsd as source, no files):** the compositor would depend on a
  service being up, and the config wouldn't be diffable or easy to back up.
