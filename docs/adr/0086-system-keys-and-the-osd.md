# 0086: System keys and the OSD: repeat in the driver, a short-lived overlay, brightness through logind

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.14a, the first part of volume, brightness and OSDs (3.14b brings PipeWire, 3.14c the
volume extra). neue-comp already routes the volume, brightness and media keys to the shell
(`neue_shell_v1.system_key`, ADR 0066), pressed and released, and leaves repeat to the
shell. The shell needs to receive them through the kit, change a level, and show it on
the rounded-square OSD; the brightness needs a backend that works without root.

## Decision
1. **`ShellEvent::SystemKey { key, state, modifiers }`** in `neue_platform::desktop`, with
   `SystemKey` (the protocol's 14 keys) and `SystemKeyState::{Pressed, Repeated,
   Released}`. **The Wayland driver repeats** the stepping keys (`SystemKey::repeats`:
   volume and both brightness pairs, not mute or media keys) with a calloop timer at
   `wl_keyboard.repeat_info`'s delay and rate (600 ms and 25/s until the compositor says),
   one key at a time; a new press or the held key's release stops it. The shell then
   treats a repeat like a press. Releases are reported but not used yet.
2. **`neue_kit::desktop::Osd`**: a widget that paints `neue_paint::materials::osd` (the
   HUD material of ADR 0059 with 20 px corners, `hud_rounded`, plus a speaker with 0–3
   waves, a muted speaker or a sun, and 16 steps lit in quarters) and fades its opacity
   on the STANDARD spring (reduced motion: no fade). `Handle<Osd>::{show, hide}`;
   `Osd::fade_in` starts transparent. AccessKit: a `Meter` with the label "Volume",
   "Volume muted" or "Brightness", the level as 0–100, live polite, hidden while hidden.
   Metrics, timings and colours are tokens in `neue_theme::materials::osd`.
3. **The OSD is an overlay layer surface that exists only while shown.** The shell opens it
   on the first key (anchored to the bottom edge, 140 px up, no keyboard, `output: None`
   so the compositor picks the active output), keeps it while keys come, starts the fade
   1.5 s after the last one and closes it 0.7 s later. The next key opens it again on
   whichever output is active then. A counter discards a stale fade's end. One hold timer
   runs at a time however fast keys repeat.
4. **Two small kit and platform additions for it:**
   - `LayerSpec::pointer` (default true). False sets an empty input region, so clicks go
     through the OSD to what is below.
   - `Window::blur_behind_rounded(radius)` asks for blur cut to the material's corners
     (`blur_behind` is the radius-0 case; HUD windows keep theirs).
   - **Layer-surface roots don't fill the theme's background.** The driver already gives
     layer surfaces a transparent background; the root now leaves what its content
     doesn't paint clear. The bar and the desktop paint their whole surface, so they are
     unchanged; the OSD's corners show what is below.
5. **Brightness: logind's `Session.SetBrightness`** (`neue_dbus::login::Login::set_brightness`)
   on the screen backlight, which needs no privileges for the active session. The
   backlight is chosen from `/sys/class/backlight` as systemd and GNOME do (firmware, then
   platform, then raw; by name within a type) and read from sysfs on each new press;
   repeats step from the value last asked for, since logind's write may not have reached
   sysfs. Steps are 1/16 of the range (Shift+Alt: a quarter step), at least one raw unit,
   and never 0 going down (many panels turn off at 0). No backlight (desktops, the VM):
   the keys do nothing.
6. **Volume is an in-memory level until 3.14b.** Up and down step it and unmute; Mute
   toggles. Keyboard backlight, mic mute and media keys are logged and ignored for now
   (3.14b and 3.14d).

## Consequences
- Pure logic (stepping, the backlight lookup) lives in the portable `neue_shell::levels`
  with tests against a fake sysfs folder; the process side (`src/osd.rs`) is Linux-only.
- Goldens `shell-osd-{volume,muted,brightness}` (light/dark × 1x/2x) in the gallery's
  Shell page. No existing golden changed.
- A shell surface that wants the theme's background behind partial content must paint it
  itself now.
- Repeat for system keys follows the compositor's rate; the shell can't change it.
