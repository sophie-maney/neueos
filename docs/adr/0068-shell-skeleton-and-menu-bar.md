# 0068: The shell skeleton and the menu bar

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 3.2b. The shell (PLAN.md §3.3) is one process with a layer surface per output,
started and restarted by the compositor (ADR 0066), using the platform's outputs and
`neue_shell_v1` support (ADR 0067). Its first piece is the menu bar: the logo menu, the
active app's name in bold, and the clock. Several things were missing: a menu-bar widget
(the kit had the material in neue-paint but no widget), a bold face of the UI font, a way
to read the local time zone, and the logind calls behind the logo menu.

## Decision
1. **Crate.** `shell/neue-shell`: a portable library (`bar`, `clock`, `windows`, `about`)
   and a Linux-only binary (`src/shell.rs`; on other hosts `main` exits with an error). The
   gallery's Shell page and the goldens use the library on macOS.
2. **Kit widgets.** `neue_kit::desktop::MenuBar` (titles from the left, extras from the
   right, over `neue_paint::materials::menu_bar`, `Backdrop::Opaque` until the blur of
   3.4/3.5) and `MenuTitle` (text, bold text or the logo mark; opens its menu right below
   itself, highlighted while open or focused; Space, Return, Down and AccessKit's Click open
   it; AccessKit roles `MenuBar` and `MenuItem` with `has_popup`/`expanded`). They live in
   the kit because they need Masonry and the kit's menu internals, which app crates can't
   reach (ADR 0035). Hover-switching between open menus waits for the global menu (3.7).
3. **Bold.** DejaVu Sans Bold 2.37 (same release and licence as the Book face, ADR 0038;
   sha256 checked against the upstream tarball) ships next to it in
   `crates/neue-paint/fonts/`. `neue_platform::Options` gains `extra_fonts`, registered in
   every window after `font`; neue-kit passes the bold face and its golden fixture registers
   it too. The kit's text lines take a bold flag (`FontWeight::BOLD`). Masonry doesn't
   synthesise bold, so a real face is needed.
4. **Logo mark.** An original vector mark (a ring around a lowercase "n" arch) in
   `neue_paint::materials::menu_bar::logo`, sized by `LOGO_SIZE` and `LOGO_STROKE` in
   `neue-theme`, embossed like the bar's labels. No Apple artwork.
5. **Logo menu.** About This Computer (a small panel from os-release and `/proc`),
   Preferences… (disabled until 3.24), Lock Screen, Restart, Shut Down, Log Out. Lock,
   Restart and Shut Down are logind calls over zbus (`neue_dbus::login`: `Session.Lock` on
   `session/auto`, `Manager.Reboot(true)`, `Manager.PowerOff(true)`), run on a background
   task. **Log Out is the compositor's `quit` action** (`ShellRequest::RunAction`), not
   `Session.Terminate`: the session ends when the compositor exits (ADR 0027), and quitting
   lets it release the seat cleanly. No confirmation dialogs yet.
6. **Clock.** `jiff` 0.2 (pure Rust, MIT OR Unlicense) reads the system time zone. The bar
   shows `Thu 09:41` (24-hour; locale and 12-hour settings come with Preferences); its menu
   shows the full date. The shell wakes at the next minute but at least every 5 s, so it
   catches up after a suspend.
7. **App name.** Until desktop entries (3.3) give the real name: the last part of a
   reverse-DNS app id (`org.neueos.Gallery` → Gallery), else the id capitalised, else the
   title; "Desktop" when no window is active.
8. **Bars.** One per output on the top layer with an exclusive zone of the theme's bar
   height and `LayerKeyboard::None`, so clicking the bar never takes focus from the app it
   names; menus are grabbing popups and get the keyboard while up. A bar opened in the same
   callback as a change can't be edited yet (queued windows have no widget tree), so the
   shell re-applies its content once the window exists (a `Sync` message from a zero-delay
   task).
9. **Starting.** `neue-comp.service` runs `neue-comp --tty --notify --shell
   /usr/bin/neue-shell`; the `neue-comp` split PKGBUILD builds a `neue-shell` package, which
   `neue-session` depends on, so the unit never names a missing binary. 3.27 still does the
   other shell-side packages.

## Alternatives considered
- **The bar from existing widgets** (a pull-down button per title in a stack): pull-down
  buttons draw button chrome and have no menu-bar highlight; the bar needs its own metrics
  and material.
- **`chrono` or `time` for the clock:** `time` can't read the local offset safely in a
  multi-threaded process; `chrono` needs `iana-time-zone` and more. jiff reads tzdb itself.
- **Log Out through logind `Session.Terminate`:** kills every process of the session
  without letting the compositor release the seat, and skips ADR 0027's stop path.
- **Queueing handle edits for windows not yet created (in the kit):** the right general
  fix, but a toolkit-core change outside this task; the shell's one case is handled
  locally. Worth revisiting if more apps hit it.

## Consequences
- `neue_platform::Options` has a new field (`extra_fonts`); every struct literal passes it.
- Every neue-paint user embeds the bold face too (about 700 KB more).
- The fake host in neue-kit's tests lays layer surfaces out at the window's width, as the
  drivers do.
- Restart and Shut Down may prompt through polkit once the agent exists (3.20).
