# 0088: Media keys and the keyboard backlight: an MPRIS client in neue-dbus, the active player, `leds` through logind

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.14d, the last part of the system keys (ADR 0086). The compositor already hands the
shell the media keys and the keyboard brightness keys (`neue_shell_v1.system_key`); the
shell logged and ignored them. They need a backend each: media players speak MPRIS on the
session bus, and a keyboard backlight is an LED in sysfs that logind can set.

## Decision
1. **`neue_dbus::mpris`**: `Mpris` lists the players (`org.mpris.MediaPlayer2.*` names
   from `ListNames`, each with its owner and `PlaybackStatus`) and sends a `Command`
   (`PlayPause`, `Pause`, `Next`, `Previous`, `Stop`) to the active one. Zbus calls, no
   `mpris` or `playerctl` crate (ADR 0005). Calls run on a worker (`cx.spawn`), never on the
   event loop, and the session connection has a 1 s method timeout so a hung player can't
   hold a key for long.
2. **The active player** is a playing one; among the playing, or if none plays, the one that
   played last; players the memory doesn't know keep the bus's order after those that it
   does (`mpris::pick`, pure). **The memory** (the last 8 unique names that played, newest
   first) is filled by a thread (`Mpris::watch`) that follows `PropertiesChanged` on
   `/org/mpris/MediaPlayer2` and notes a change to `Playing`, and by the key presses
   themselves: a player found playing when a key is pressed counts as played (a player that
   was already playing when it appeared sends no signal).
3. **Keys:** the Play key is `PlayPause` (it is the play/pause key on keyboards), the Pause
   key is `Pause`, then Next, Previous and Stop. No player: nothing happens. No OSD (media
   keys have no level to show). Media keys don't repeat.
4. **Keyboard backlight:** the first `/sys/class/leds/*::kbd_backlight` (by name) with a
   range (`Backlight::find_keyboard`), set through logind `SetBrightness("leds", name, n)`
   like the screen's (the same `Login`, now with a `subsystem`). It steps like the screen's
   (1/16 of the range, at least one raw unit; keyboards have two or three levels, so each
   press moves one) but goes down to 0: a keyboard backlight is meant to go dark, unlike a
   panel (`Backlight::floor`). The OSD shows a new keyboard glyph (`osd::Glyph::Keyboard`: an
   outline with two rows of keys and a space bar under three rays) and the level, labelled
   "Keyboard brightness".

## Consequences
- `levels::Backlight` now serves both backlights; the shell's `osd.rs` keeps the last
  asked-for value for each.
- Players that are on the bus but ignore calls cost a timeout per key press at most once
  (the call that finds them out), since `players()` leaves non-answering players out.
- Eject is still ignored. Players on the system bus and `playerctld`-style multiplexing are
  out of scope.
- `neue-dbus`'s `mock_mpris` example stands in for a player in the VM (`playerctl status`
  follows the keys).
- Not checked: a real keyboard backlight (the VM has none; unit tests against a fake sysfs
  folder only).
