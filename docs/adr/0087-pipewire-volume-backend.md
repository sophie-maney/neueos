# 0087: The PipeWire volume backend: pipewire-rs in a new `neue-audio` crate

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.14b. The volume keys (ADR 0086) moved an in-memory level; they must change the
default sink's volume and mute, and Mic Mute the default source's. 3.14c (the volume
extra) also needs the list of sinks and a way to set the default one, and 3.22 (the
ScreenCast portal) needs PipeWire for video streams. PLAN.md fixes PipeWire with
WirePlumber as the audio stack and allows C system libraries only through safe Rust
bindings, with an ADR for each new one.

Options:
- **pipewire-rs** (`pipewire` 0.10, the PipeWire project's own bindings): the whole client
  API, including streams for 3.22. Links libpipewire (MIT) through bindgen, so building
  needs clang and the headers.
- **Talking to `pipewire-pulse` over the Pulse protocol** (`libpulse-binding`, also C, or a
  pure-Rust Pulse client): the Pulse model loses PipeWire's metadata and routes, and a
  second protocol wouldn't serve 3.22.
- **Running `wpctl`**: a process per key press, no change notifications, nothing for 3.22.
- **WirePlumber's D-Bus or Lua API**: WirePlumber has no stable D-Bus API for this.

## Decision
1. **`pipewire` 0.10 (pipewire-rs)**, MIT, with its `libspa`, `pipewire-sys`, `libspa-sys`
   (MIT) and the build-time `bindgen` (BSD-3-Clause), `clang-sys` (Apache-2.0) and
   `system-deps`, `nom`, `cookie-factory` and friends (MIT or Apache-2.0): all compatible
   with GPL-3.0-or-later. Built against the system libpipewire (Arch `libpipewire`; Ubuntu
   24.04's 1.0 in CI). No version feature is enabled, so it builds against PipeWire 0.3.x
   and later. `unsafe` stays inside the bindings.
2. **A new Linux-only crate, `neue-audio`** (in xtask's `LINUX_ONLY_CRATES`; the
   `pipewire` dependency is gated on `target_os = "linux"`). The model (`State`,
   `Endpoint`, `Kind`, the cubic scale, the metadata's JSON) has no PipeWire in it and is
   unit-tested; the backend is `Audio`.
3. **The PipeWire main loop runs on its own thread** (`neue-audio`), because pipewire-rs
   objects are not `Send` and the shell's loop is neue-kit's. `Audio::start(on_change)`
   spawns it; `on_change` gets the whole `State` (sinks, sources, the default ids,
   whether connected) on every change, deduplicated. `Audio::{set_volume, set_muted,
   set_default}` queue commands over `pipewire::channel`. When PipeWire goes away (core
   error on `PW_ID_CORE`) the thread reports an empty state and reconnects every 2 s,
   keeping the command channel.
4. **What it follows:** nodes with `media.class` `Audio/Sink`, `Audio/Source` or
   `Audio/Source/Virtual` (listed once their `Props` arrive), `Audio/Device` cards and
   their `Route` params, and the `default` metadata's `default.audio.sink` and
   `default.audio.source` (JSON `{"name": "<node.name>"}`).
5. **Volume is on the cubic scale `wpctl` shows** (`linear = cubic³`), read as the loudest
   channel; setting it keeps the balance between channels.
6. **Writes go where `pipewire-pulse` and `wpctl` put them:** for a node that belongs to a
   card (`device.id` and `card.profile.device`) with a known route, a `Route` param on the
   device with `save: true` (the hardware mixer moves and WirePlumber stores it per
   port); otherwise the node's `Props`. Reads come from the node's `Props`, which mirror
   the route.
7. **Setting the default** writes `default.configured.audio.sink`/`.source` (type
   `Spa:String:JSON`) on the `default` metadata, as `wpctl set-default` does; WirePlumber
   then updates `default.audio.*` and moves streams.
8. **The shell** (`osd.rs`) starts `Audio` with the kit's `Sender`. Volume Up and Down step
   the default sink on the OSD's grid (repeats step from the volume last asked for, as
   brightness does; Up leaves a volume above 1 alone) and unmute it; Mute toggles it;
   Mic Mute toggles the default source's mute and shows the new
   `OsdGlyph::{Microphone, MicrophoneMuted}` (struck through, with a gap cut around the
   stroke; goldens `shell-osd-microphone{,-muted}`). No PipeWire or no default device:
   the key does nothing.

## Consequences
- Building `neue-shell` on Linux needs clang and libpipewire's headers: added to the
  PKGBUILD's `makedepends`, CI's apt line (`libpipewire-0.3-dev libclang-dev`) and the
  OrbStack and VM setup (`docs/dev-setup.md`). `neue-shell` depends on `libpipewire`;
  `pipewire` and `wireplumber` are optional dependencies.
- 3.14c reads `State::sinks` and calls `set_default` and `set_volume`; 3.22 adds streams
  to the same crate or uses `pipewire` directly.
- No integration test runs against a PipeWire daemon in CI; the VM check (null sinks,
  `docs/dev-setup.md` §1.6e) covers the backend, and a card with routes needs real
  hardware (owner check).
