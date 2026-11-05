# 0067: Shell surfaces in neue-platform and neue-kit

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 3.2a. The shell (3.2b) puts a menu bar and a desktop background on every output and
follows the compositor's window list over `neue_shell_v1` (ADR 0066). neue-platform could
open layer surfaces but always let the compositor pick the output, told the app nothing
about outputs, and ended its loop when the last window closed. The compositor offers
`neue_shell_v1` only on the one connection it hands the shell (`WAYLAND_SOCKET`), so the
shell can't open a second connection for it: the toolkit's own driver has to bind it.

## Decision
1. **A portable `neue_platform::desktop` module** holds what only the shell uses:
   `OutputId`/`OutputInfo`/`OutputEvent`, `ToplevelId`/`Toplevel`/`ShellEvent`,
   `ShellRequest` and `ChordModifiers`. neue-kit re-exports it, with `LayerSpec`,
   `Anchor`, `Margin`, `ShellLayer` and `LayerKeyboard`, as `neue_kit::desktop`.
2. **Outputs.** The Wayland driver reports each output through `App::on_output` once
   sctk has its full description (after xdg-output), then changes and removal.
   `OutputId` is the driver's own counter, never reused. `OutputInfo` carries the
   connector name, the description, the logical size (xdg-output, else the current mode
   over the scale) and the integer `wl_output` scale. `Ctx::outputs` lists them.
   `LayerSpec::output` puts a layer surface on one; an output that's gone counts as
   none. The compositor closes layer surfaces on an unplugged output, which the driver
   already turned into a closed window.
3. **`neue_shell_v1` in the driver.** The driver binds `neue_shell_manager_v1` when it's
   offered (`Ctx::has_shell`). Each toplevel gets a `ToplevelId` (the driver's counter;
   the `window-N` string is `Toplevel::identifier`); its state events are buffered and
   reported as one `ShellEvent::Toplevel` with the whole state on `done`, and only when
   something changed. `closed` becomes `ShellEvent::Closed` and the object is
   destroyed. `active` becomes `ShellEvent::Active`. `Ctx::shell(ShellRequest)` queues
   activate, minimise, unminimise, close, `run_action` and `send_shortcut`; the driver
   sends them after the callback. A failed action comes back as
   `ShellEvent::ActionFailed`. Without the protocol requests are dropped with a debug log.
4. **Left for the tasks that need them:** icons (3.16), menu addresses (3.7), system keys
   (3.14) and minimise rectangles (3.16). The driver ignores those events for now.
5. **`Ctx::keep_alive`** (and `Cx::keep_alive` in the kit) keeps the loop running with no
   window open, until `exit`: the shell starts with no surface and opens them as outputs
   arrive, and has none while every output is unplugged.
6. **neue-kit**: `Window::layer(LayerSpec)` opens a layer surface with no chrome and a
   transparent background (the root paints everything); `Application::output` and
   `Application::shell` get the events; `Cx::outputs`, `Cx::has_shell` and `Cx::shell`
   mirror the platform's.
7. **The winit driver** reports no outputs and no shell protocol. The shell's widgets are
   portable; the shell process is Linux-only.

## Alternatives considered
- **A generic hook for extra protocols in the driver** (the app dispatching its own
  objects on the driver's queue): the driver's `State` would become public API, and the
  shell would still need a way to name its own surfaces. One private protocol for one
  client doesn't warrant it.
- **The shell binding `neue_shell_v1` on a second connection**: the compositor offers it
  on the shell's socket only (ADR 0066), and `set_minimize_rectangle` needs the shell's
  own surfaces.
- **Reporting raw protocol events** (identifier, title… then done): every consumer would
  re-implement the batching. The driver applies `done` once.
- **Keeping the shell alive with a hidden window**: a surface for nothing, and a toplevel
  the compositor would list.

## Consequences
- neue-platform's `App` gains `on_output` and `on_shell` (default no-ops) and `Ctx` gains
  `outputs`, `has_shell`, `shell` and `keep_alive`; `LayerSpec` gains `output`.
- neue-kit's `Application` gains `output` and `shell` (default no-ops).
- neue-comp's `State::layer_surfaces` lists layer surfaces by namespace and output, for
  tests; its harness gains `connect_shell_raw`.
