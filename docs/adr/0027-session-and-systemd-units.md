# 0027: `neue-session` and the systemd user units (task 1.19)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
greetd must start a neueOS session and see it end at logout. The compositor picks its own
Wayland socket (`wayland-N`) and X display (`:N`, ADR 0026), so nothing else knows them in
advance, and the systemd user manager needs them for the units that come later (shell,
services, portals).

## Decision
1. **`neue-session`** (`session/neue-session`, std only) is what greetd runs. It refuses to
   start if `neue-session.target` is active, clears stale `WAYLAND_DISPLAY`, `DISPLAY` and
   `NEUE_SOCKET` in the user manager, sets the session variables (`XDG_SESSION_TYPE=wayland`,
   `XDG_CURRENT_DESKTOP` and `XDG_SESSION_DESKTOP` = `neueOS`) and an allow-list of login
   variables (`XDG_SEAT`, `XDG_SESSION_ID`, `XDG_SESSION_CLASS`, `XDG_VTNR`, `LANG`,
   `LANGUAGE`, `LC_*`) with `systemctl --user set-environment` (explicit values: no
   `set_var`), then runs `systemctl --user start --wait neue-session.target`. It unsets what it set on
   the way out. `neue-session stop` stops the target.
2. **Units** (`session/systemd/user/`). `neue-session.target` `BindsTo=` `neue-comp.service`
   and `graphical-session.target`, so the session ends when the compositor does and
   `--wait` returns. `neue-comp.service` is `Type=notify`, `PartOf=` the target, no restart.
3. **Compositor handshake.** `neue-comp --notify`: after the sockets and X display exist, run
   `systemctl --user set-environment WAYLAND_DISPLAY=... DISPLAY=...`, then send `READY=1`
   to `$NOTIFY_SOCKET` (one datagram, `std`). Units ordered after the compositor see the
   environment. Failures are warnings.
4. **Stopping.** The compositor has no signal handler: calloop's `signals` source needs the
   `nix` crate, a new dependency. The unit instead uses `ExecStop=neuectl quit` (the
   existing IPC) and `KillSignal=SIGCONT`, so systemd does not SIGTERM the compositor while it
   tears down; SIGKILL follows after 10 s.
5. **greetd.** `session/wayland-sessions/neueos.desktop` (`Exec=neue-session`) and
   `session/greetd/config.toml` (`agreety --cmd neue-session`) for the VM; the graphical
   greeter replaces the latter in Phase 3.

## Consequences
- No new dependency and no `unsafe`. `neue-session` builds and tests on macOS (a fake
  manager records the `systemctl` calls); the units and greetd need the VM.
- A compositor crash ends the session: `neue-session` fails and greetd shows the login.
- `SIGTERM` or `kill` on the compositor still skips teardown; `neuectl quit` is the clean path.
- Paths in the units assume `/usr/bin` (the PKGBUILD of task 1.20).

## Alternatives considered
- **A `Request::Environment` in `neue-ipc`** that `neue-session` polls: more protocol surface
  for no gain over the compositor pushing the values once.
- **A fixed socket name**: `DISPLAY` stays unknown.
- **calloop `signals` or `signal-hook`** for SIGTERM: cleaner, needs a dependency.

## Revisit when
We want `SIGTERM` to be a clean stop (then add the dependency and drop `ExecStop`), or for
the shell to start as its own unit (Phase 3).
