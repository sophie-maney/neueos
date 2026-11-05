# 0080: The kit's command key is Super (⌘) on every host

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
PLAN.md Q1: Super is the command key (⌘). ADR 0079 made the compositor send Super to neue-kit
apps unchanged (they bound `neue_window_manager_v1`) and Control for Super to everyone else.
The kit still took Ctrl as its command key on Linux and Cmd (META) only on macOS hosts, so
Super+C did nothing in a kit app and Ctrl+C did the copy. ADR 0042 §4 recorded that split.

## Decision
1. **META is the command key everywhere.** Text fields, text views, tables and collections
   (select all, copy, cut, add to selection) read `mods.meta()` on every host. Ctrl is no
   longer a command key: Ctrl+A, Ctrl+C and Ctrl+X in a kit app do nothing.
2. **Paste** (neue-platform, Wayland driver): Super+V (with Shift too) or Shift+Insert.
   Ctrl+V is not paste. The winit driver already used META.
3. **Menus** draw ⌘ (spoken "Cmd") for `Shortcut::command` on every host; DBusMenu
   names it `Super`. The per-thread "command is Ctrl" override for goldens is gone, so
   goldens now show ⌘.
4. **What stays platform-specific:** the word key (Option on macOS, Ctrl elsewhere) and
   Ctrl+Home/End to the ends of a text view on Linux. The line-end jumps (⌘←/→,
   ⌘⌫/⌦ in fields and text views) and text-end jumps (⌘↑/↓) now work on Linux too.
5. ADR 0042 §4 is amended: its "Cmd on macOS hosts and Ctrl elsewhere" reads "Super (⌘)
   on every host".

## Consequences
- A compositor binding on Super+key wins over the app (ADR 0079), so such a chord never
  reaches a kit app; Super+arrows and Super+click may be taken by the compositor.
- Ctrl-click no longer toggles rows in tables and collections; Cmd (Super)-click does.
- Third-party menus showing ⌘ is 3.10c.

## Alternatives considered
- Keep Ctrl as well as Super in the kit: two ways to copy, and Ctrl+key chords would stop
  matching what the menus show; rejected for consistency with the Mac.
