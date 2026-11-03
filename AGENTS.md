# AGENTS.md

Instructions for coding agents working in this repository. Read `PLAN.md` first. It holds
the architecture, the decisions already made, and the roadmap. Do not create `CLAUDE.md`;
this file is the single source of agent instructions.

## Project in one paragraph
neueOS is a personal Linux desktop environment and Arch-based distribution that
reinterprets OS X Mountain Lion's skeuomorphic design with modern technology. It consists
of:
- a Wayland compositor (`neue-comp`, built on Smithay),
- a custom toolkit (`neue-kit`, built on wgpu, vello, parley and AccessKit),
- a shell (`neue-shell`: menu bar, dock, notifications, launcher, search, widgets),
- services, first-party apps, and a distribution (PKGBUILDs, archiso, an installer).

All first-party code is Rust.

## Hard rules
- **Rust only** for first-party code. Shaders (WGSL/GLSL), Wayland protocol XML, and themes
  for third-party toolkits (GTK CSS, Kvantum SVG) are the only exceptions. C libraries are
  used only through safe Rust bindings. Any new C dependency needs an ADR in `docs/adr/`.
- **Wayland only.** Never add X11 or Xorg code, Xlib/XCB crates, or an X11 window manager
  to the compositor. Legacy X11 apps are handled exclusively by the external
  `xwayland-satellite` process.
- **Audio:** PipeWire only. **App distribution:** Flatpak only (never Snap). **Base:** Arch
  Linux (x86_64) and Arch Linux ARM (aarch64).
- **No Apple assets.** No Apple icons, textures, fonts (Lucida Grande, Helvetica Neue),
  sounds, screenshots used as assets, or trademarked names (Finder, Spotlight, Mission
  Control…) in user-facing strings. All artwork is original. Using these names in code
  comments to explain intent is fine.
- **Licence:** GPL-3.0-or-later. Every dependency must be licence-compatible; check
  before adding one.
- **Don't break the portable build.** Crates marked portable (see below) must compile and
  pass tests on macOS.

## Platform matrix
| Crate group | macOS (dev host, aarch64) | Linux aarch64 | Linux x86_64 |
|---|---|---|---|
| `neue-paint`, `neue-theme`, `neue-kit`, `neue-platform` (winit), `neue-config`, `neue-desktop` (pure parts), apps' UI, widget gallery | ✅ portable | ✅ | ✅ |
| `neue-comp`, `neue-shell`, services, `neue-platform` (Wayland), anything using D-Bus, DRM, udev or PipeWire | ❌ | ✅ | ✅ |

- Gate Linux-only code with `#[cfg(target_os = "linux")]` at module or crate level.
  Linux-only crates must still be members of the workspace; exclude them from macOS CI with
  `cargo xtask` filters, not by commenting them out.
- Apps must be able to start on macOS through the winit backend, using mocked services, so
  their UI can be iterated on natively.

## Commands
> These are planned and are created in Phase 0. Update this section when they land.

```sh
cargo xtask check          # fmt --check + clippy -D warnings + tests for the current host
cargo xtask gallery        # run the widget gallery (works on macOS)
cargo xtask golden --bless # regenerate golden images (review the diffs!)
cargo xtask linux <cmd>    # run <cmd> inside the OrbStack Arch machine (arm64)
cargo xtask linux-amd64 <cmd>  # same, in the amd64 (Rosetta) machine
cargo xtask vm sync|run    # sync to the Parallels VM and start a session there
cargo xtask package <crate>    # build a pacman package via the PKGBUILD in distro/pkgbuilds
cargo xtask iso [--arch x86_64|aarch64]
```

### Driving the running desktop
`neuectl` (the compositor's IPC) is the interface for automation and verification.
- `neuectl windows`, `neuectl spaces`, `neuectl outputs` list state.
- `neuectl action <name>` triggers actions.
- `neuectl screenshot <path>` captures the screen.
- `neuectl events` streams events.

Use these to verify visual and behavioural changes in the VM instead of guessing.

## Code conventions
- Rust edition 2024, latest stable toolchain. Shared dependency versions live in the root
  `[workspace.dependencies]`, and lints in `[workspace.lints]`.
- `rustfmt` defaults. `clippy` with `-D warnings`. No `#[allow]` without a reason comment.
- **Errors:** `thiserror` in libraries, `anyhow` in binaries. No `unwrap()` or `expect()`
  in non-test code unless an invariant makes failure impossible; document that invariant.
- **`unsafe`:** only when unavoidable, with a `// SAFETY:` comment. Keep it out of UI and
  app code.
- **Logging:** `tracing`. No `println!` in libraries or daemons.
- **Async and event loops:**
  - The compositor is single-threaded on `calloop`. Do not block in it.
  - Services use `tokio` together with `zbus`.
  - Toolkit apps use neue-kit's loop. Run blocking work on background tasks.
- **D-Bus:** use `zbus`. Our interface names live under `org.neueos.*`.
- **Config:** TOML, schema in `neue-config`, with every field documented. Add new settings
  with defaults so old configs keep working.
- Name crates `neue-<thing>`. Keep crates focused; prefer a new small crate over a
  catch-all.

## Design and UI work
- Theme values come from `neue-theme` tokens. Never hard-code colours, metrics or durations
  in widgets.
- Materials must be procedural or vector so they stay sharp at any scale. Every material
  needs a light **and** a dark variant.
- Every new or changed widget needs:
  - an entry in the widget gallery,
  - golden-image tests (states × light/dark × scale 1 and 2),
  - AccessKit roles and labels,
  - keyboard operability.
- Motion: use the spring presets in `neue-theme`, and respect the reduced-motion setting.

## Testing expectations
- Unit tests next to the code.
- Golden tests: rendered with `vello_cpu`, so they are deterministic. Commit updated
  goldens only after inspecting the diff.
- Compositor: integration tests on the headless backend with scripted Wayland clients.
- Behaviour that only shows up in a real session: verify in the Parallels VM via
  `neuectl`, and say so in the PR.
- Performance-sensitive changes (render paths, Overview, animations) must also be tested on
  real x86_64 hardware. Flag them in the PR if that hasn't happened yet.

## Workflow
- Work on a branch. Never commit directly to `main`.
- Commit messages: imperative and scoped, e.g. `comp: add xdg-dialog sheet animation`.
- Record architecture decisions as ADRs in `docs/adr/NNNN-title.md`. Use them for new
  dependencies, protocols, IPC changes and cross-crate interfaces.
- Update `PLAN.md` checkboxes when you finish roadmap items. If an implementation diverges
  from the plan, update the plan in the same PR.
- Private Wayland protocols live in `crates/neue-protocols/` as XML, with documentation in
  `docs/protocols/`.
