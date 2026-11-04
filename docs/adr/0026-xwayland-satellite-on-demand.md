# 0026: xwayland-satellite on demand (task 1.18)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
ADR 0001 decided that X11 apps run through `xwayland-satellite` and that `neue-comp` only
reserves the X display sockets and spawns satellite on the first connection. Task 1.18
implements it. Open points: how the sockets are reserved, how they reach satellite, what
happens when it exits, and how `DISPLAY` is exported.

## Decision
1. **Reservation** (`xwayland.rs`), the way Xorg does it: the lowest free display `:0` to
   `:32`, a lock file `/tmp/.X<n>-lock` (pid right-aligned in ten columns, mode 0444, a lock
   whose pid is gone in `/proc` is stale and replaced), the filesystem socket
   `/tmp/.X11-unix/X<n>` and the abstract socket `@/tmp/.X11-unix/X<n>` (libxcb tries the
   abstract one first). `/tmp/.X11-unix` is created 1777 if missing. All with `std`
   (`SocketAddrExt`): no new dependency, no `unsafe`. Lock and socket file are removed on
   drop; a crash leaves them, and the next start treats them as stale.
2. **On demand.** Both sockets are watched on the event loop. The first connection spawns
   `xwayland-satellite :<n> -listenfd <fd> -listenfd <fd>` with `WAYLAND_DISPLAY` set to our
   socket and `DISPLAY` removed. The fds are `dup`s (`rustix::io::dup` makes them without
   close-on-exec), so the child inherits them under the same numbers and the originals
   stay ours. Satellite starts Xwayland on them. Needs satellite 0.7 or later. We do not
   probe with `--test-listenfd-support`, so that no X-related process runs before the first
   connection; an older satellite fails on first use and the log says so.
3. **Not found, not fatal.** If the command is missing from `PATH`, the compositor logs a
   warning, reserves nothing and exports no `DISPLAY`. A failed reservation is the same.
4. **Lifecycle.** A thread waits on the child and reports its exit through a calloop
   channel. The sockets are watched again then: at once if satellite ran 5 s or more,
   otherwise after 1 s, so a satellite that crashes on start cannot spin the loop. A client
   that connected meanwhile stays queued in the socket and starts it again. Satellite is a
   Wayland client and exits when the compositor goes away.
5. **`DISPLAY`.** Children started with `--command` get `DISPLAY=:<n>`; otherwise it is
   removed, so nothing falls back to a foreign X server. The compositor does not change its
   own environment (`set_var` is `unsafe` in edition 2024): the session (task 1.19) must
   import `DISPLAY` for the shell and the launcher. `State::xwayland_display()` returns it.
6. **Options.** `Options::xwayland` (`Some` by default; ignored without a Wayland socket,
   so tests that connect clients directly are unaffected), command line `--no-xwayland` and
   `--xwayland-satellite <path>`. Not in `compositor.toml` yet: nothing needs to change it
   at run time. Add it when the settings app has a use for it.

## Consequences
- X11 clients are plain xdg toplevels to us. Selection bridging is satellite's job (ADR
  0011). Whether global menus work for X11 apps is open (ADR 0005); not part of this task.
- Real X11 behaviour (xeyes, a GTK3 app, cursors, scaling) is checked in the VM
  (`docs/dev-setup.md` §19). The headless tests use a shell script as a stand-in.
- Multi-seat or multiple compositors on one machine pick different display numbers
  through the lock files, as with Xorg.

## Alternatives considered
- **Probe `--test-listenfd-support` at start-up** (what niri does): rejects old satellites
  early, but runs an X-related process before any X client exists.
- **Satellite creates and owns the sockets** (`xwayland-satellite :0` without `-listenfd`):
  satellite would run from the start, which the plan rules out.
- **`pre_exec` to clear close-on-exec:** needs `unsafe`; `dup` does the same safely.
