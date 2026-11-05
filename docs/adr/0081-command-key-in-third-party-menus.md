# 0081: ⌘ in third-party menus: `neue_shell_toplevel_v1.command_key` (version 3)

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
ADR 0079 sends third-party apps Control for Super, so the user types Super+C where the
app's menu says Ctrl+C. The global menu (ADR 0076, 0078) shows the app's menus with the
shortcuts the app exports, which are Control shortcuts: they disagree with the keys the
user presses. Task 3.10c makes the bar show ⌘ there. The shell can't work out by itself
whether a window is remapped: that depends on the compositor's `[super-as-command]`
settings and on whether the client is a neue-kit app (bound `neue_window_manager_v1`), and
the shell sees neither.

## Decision
1. **Event.** `neue_shell_toplevel_v1.command_key(key)`, since 3, with the enum
   `command_key`: `super` (the app takes Super itself) and `ctrl` (Super reaches it as
   Control). It is part of every toplevel's first batch (before `done`) and sent again,
   in a batch of its own, when the answer changes.
2. **Versions.** `neue_shell_manager_v1` goes to version 3, and so does
   `neue_shell_toplevel_v1`: the compositor creates a toplevel with the bound manager's
   version (as it did before, when the toplevel interface was still version 1 and a
   manager of version 2 made version-2 objects), so an event with `since="3"` needs the
   toplevel interface at 3. A shell that bound version 1 or 2 is sent no `command_key`
   and shows Control shortcuts as they are; neue-shell binds up to 3.
3. **What it says.** The answer is `State::window_super_as_ctrl`: `super_as_ctrl` of
   ADR 0079 without the keyboard-shortcuts inhibitor. An inhibitor comes and goes with
   focus and the surface that holds it is a VM or remote-desktop viewer, whose own menus
   are rarely the point; tracking it would turn every focus change into a snapshot diff.
   It is recomputed with the rest of the window's snapshot when the window changes (a new
   app id) and, for all windows, when `compositor.toml` is reloaded
   (`shell_command_keys_changed`).
4. **neue-platform** keeps it in `Toplevel::super_as_ctrl` (false for a compositor too
   old to say). **neue-shell** passes the active window's flag to the menu conversion:
   with it, a Control modifier in a DBusMenu or GMenu shortcut is shown as ⌘
   (`Shortcut::command`), and a Super next to it is the same key and shown once. Alt,
   Shift and the key are unchanged. When the active window changes or the flag does, the
   bar's menus are built again from the menus already read; while a menu is open the
   change waits until it closes, as new menus do.

## Consequences
- Firefox's File ▸ New Tab reads ⌘T, foot's menus (none exported) and kit apps' keep
  what they export.
- A real Ctrl chord still works in a remapped app; the menu shows only the way the user
  is meant to type it.
- A remapped app's shortcut that uses Super itself (rare on Linux, and ADR 0079's
  rewrite would make it Control anyway) shows as ⌘.
- The bar does not show a shortcut inhibitor's effect (§3).

## Alternatives considered
- **A neuectl/IPC field only:** the shell is a Wayland client of the compositor and
  already follows windows through `neue_shell_toplevel_v1`; a second channel would need
  its own bookkeeping.
- **A shell-side list of exempt app ids:** duplicates `[super-as-command]` and can't know
  about neue-kit clients; it would drift from the compositor's answer.
- **Bump only the manager:** a server-created object carries its parent's version, so the
  toplevel interface's own version has to say what it sends.
