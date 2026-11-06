# 0066: `neue_shell_v1`, the private protocol between neue-comp and neue-shell

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 3.1. The shell (menu bar, dock, OSDs; Phase 3) needs the window list, the active
window and each window's app id, title, minimised and attention state, icon and
global-menu address (ADR 0005, 0025). It must activate, minimise, restore and close
windows, give the compositor a minimise target per window for Phase 4's genie, run
compositor actions, type key chords into the active app for the fallback Edit menu
(ADR 0005 §4), and get the volume, brightness and media keys. `ext-foreign-toplevel-list`
carries only titles and app ids, and `wlr-foreign-toplevel-management` has no icons,
menus, key routing or privilege model. ADR 0027 left open how the shell is started.
PLAN.md §3.3 and §9 say the compositor restarts the shell when it crashes.

## Decision
1. **A private protocol, `neue_shell_v1`** (`crates/neue-protocols/protocols/
   neue-shell-v1.xml`, documented in `docs/protocols/neue-shell-v1.md`):
   - `neue_shell_manager_v1` (global): events `toplevel` (new object per window),
     `active`, `system_key`; requests `run_action`, `send_shortcut`.
   - `neue_shell_toplevel_v1`, one per window: events `identifier`, `title`, `app_id`,
     `state` (minimised, attention), `icon` + `icon_buffer`, `menu_dbusmenu` /
     `menu_gmenu` / `menu_none`, `done`, `closed`; requests `activate`, `minimize`,
     `unminimize`, `close`, `set_minimize_rectangle`.
   - `neue_shell_action_v1`: the outcome of `run_action`, `done` or `failed(message)`.
   The windows are the ones `ext-foreign-toplevel-list` announces, and `identifier` is
   the same `window-N` string, N being the `neuectl` id.
2. **Only the shell may bind it, and the compositor starts the shell.** `neue-comp
   --shell <command>` runs the command through `sh -c` with one end of a socket pair in
   `WAYLAND_SOCKET` and `WAYLAND_DISPLAY` set for the shell's own children. That
   connection is marked as the shell (`ClientState::shell`), and the global's `can_view`
   is true only for it: other clients, sandboxed ones included, don't see the global, and
   binding its name anyway is a protocol error from libwayland's registry. wayland-client
   removes `WAYLAND_SOCKET` from the environment on connect, so apps the shell launches
   don't inherit it.
3. **Restarts.** A waiter thread blocks on the child and reports its exit through a
   calloop channel, like `xwayland-satellite` (ADR 0026). The compositor then drops the
   old client (a leftover child might still hold the socket) and starts the command
   again: at once if it ran for 10 s or more, otherwise after 250 ms, doubling with each
   early exit in a row up to 30 s. A shell crash never takes down the session.
4. **The active window**, not the keyboard focus, is what the shell gets: while a layer
   surface (a shell menu), a popup or the lock screen has the keyboard, the window that
   had it stays active, as an app keeps the menu bar while its menus are open on the
   original. Clicking the desktop, or the last window closing or minimising, makes it
   null.
5. **State is diffed.** The compositor keeps the snapshot the shell was last sent per
   window and sends only what changed, followed by `done`, wherever `neuectl` subscribers
   are told about a window, and on the commit that applies a new `xdg-toplevel-icon`.
   Nothing is tracked while no shell is bound; a newly bound manager gets everything.
6. **Icons** are passed as sealed memfds of tightly packed ARGB8888 (copied out of the
   app's shm buffers once per icon, at most 512 px a side), so the shell needs no access
   to the app's buffers and the same fd serves later binds. The theme name, if any, comes
   too; with neither the shell uses its desktop-entry icon for the app id.
7. **`send_shortcut(modifiers, keysym)`** resolves the keysym (and `Shift_L`,
   `Control_L`, `Alt_L`, `Super_L`) to keycodes in the current layout with Smithay's
   `keycode_for_keysym`, gives the active window the keyboard if something else has it,
   and presses then releases the keys through the seat with a filter that forwards
   everything, so our own bindings never see them. Keysyms, not keycodes, so Cmd-C-style
   chords work on any layout. Ignored while locked.
8. **System keys** (volume, mute, mic mute, screen and keyboard brightness, play, pause,
   next, previous, stop, eject) go to the shell as `system_key(key, state, modifiers)`
   while a manager is bound, also on the lock screen and past a shortcut inhibitor; a
   compositor key binding for the same key wins. Repeat is the shell's job. With no shell bound the
   keys go to the focused client as before.
9. **`run_action`** runs a `neuectl` action; a null window means the active window (not
   the keyboard focus, which may be the shell's menu). Its outcome is sent after the
   dispatch that ran it: wayland-backend cannot send a destructor event for an object
   during the request that created it.
10. **`set_minimize_rectangle(surface, x, y, w, h)`** is stored on the window and
    resolved to global coordinates when read (`State::minimize_target`), so it follows
    the dock surface. Only the shell's own surfaces count, and only layer surfaces
    resolve.
11. **`neuectl` is unchanged.**
12. **Bitfield enums** (`modifiers`, `state`) make the generated code use `bitflags`,
    which neue-protocols now depends on (already in the tree through Smithay, MIT or
    Apache-2.0).

## Alternatives considered
- **A systemd user unit for the shell** (`Restart=always`): systemd's journal and
  restart policy for free, but the compositor could not tell the shell from any other
  client; the options are "first unsandboxed binder wins" (racy, any client can win) or
  checking `/proc/<pid>/exe` (fragile). The compositor spawning it on a private socket
  is how it knows. The shell's stdout and stderr go where the compositor's go (the
  journal, under `neue-comp.service`).
- **`wlr-foreign-toplevel-management` plus extras**: covers activate, minimise and close,
  but icons, menus, attention, key routing and the shell-only rule would still need a
  private protocol next to it. One protocol is simpler for the one client that uses it.
- **Icons as the app's `wl_buffer`s or file paths**: buffers belong to another client;
  apps set pixel icons precisely because there's no file.
- **Keycodes in `send_shortcut`**: the shell would need the layout to type Ctrl+C on
  Dvorak; the compositor already has it.
- **`neuectl` (JSON IPC) for the shell**: no fd passing for icons, no object lifetimes,
  and a second connection to keep in step with the Wayland surfaces the shell draws.

## Consequences
- `neue-comp` gains `--shell <command>` and `Options::shell`. The session unit does not
  pass it yet: task 3.2 adds `--shell /usr/bin/neue-shell` to `neue-comp.service` with the
  binary, so until then nothing restarts in a loop.
- In the VM, `pkill neue-shell` brings the shell back at once; a shell that can't start
  is retried every 30 s at most, with a warning each time.
- Phase 4's genie reads `State::minimize_target`.
- ADR 0027's "revisit when the shell starts as its own unit": decided against; the shell
  is the compositor's child.

## Not done
- The parent of a dialog (the dock's window list will want to skip sheets): a later
  version can add a `parent` event.
- Workspaces and outputs per window: `ext-workspace` and Phase 4.
- A real shell client: task 3.2 (the headless tests script one).

## Addendum (2026-11-06, task 3.15c): the shell's activation tokens
- **The compositor grants every `xdg_activation_v1` token the shell's client asks for**
  (the client inserted with `insert_shell_client`, `ClientState::shell`), whatever has
  keyboard focus and whatever serial it passes. The shell hands tokens to the apps it
  launches (`XDG_ACTIVATION_TOKEN`) and to the sender of a clicked notification (the
  `ActivationToken` signal, ADR 0089); a banner or the dock never has keyboard focus, so
  the focus rule for ordinary clients would refuse them all.
- **Ordinary clients are unchanged:** a token takes focus only if its client had keyboard
  focus and any serial is no older than that focus; otherwise activating with it asks for
  attention. Tokens stay single-use and live 10 s.
- **Why not a protocol request instead:** xdg-activation is what apps already speak, and
  the token can pass through D-Bus and the environment; the trust is the one the shell
  already has (it alone binds `neue_shell_v1`).
- **Consequence:** a compromised shell can move focus at will, which it could already do
  with `neue_shell_toplevel_v1.activate`.
