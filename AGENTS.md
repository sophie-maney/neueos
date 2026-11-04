# AGENTS.md

Instructions for coding agents working in this repository. Read `PLAN.md` first, but only
the sections relevant to your task. It holds the architecture, the decisions already made,
and a roadmap summary; each phase's task list lives in `docs/plan/phase-N.md`. Do not create `CLAUDE.md`; this file is the single source of agent
instructions. **Before any work, run the checks in "How to work" below.**

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
  `xwayland-satellite` process. One exception: `neue-comp`'s dev-only `winit` feature pulls
  in winit's X11 crates transitively (unused at runtime, never in release builds; ADR 0001).
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

## How to work: scope, model and token budget
The owner is on a usage-limited plan. Results matter, but tokens are scarce: get things
working first and polish later. Follow these rules in every thread.

### 1. Check before you start (stop and ask if any check fails)
State the result of these checks in one or two lines, then continue or ask.
- **Scope:** one thread is one task, and one task is one PR (about a day of work, a few
  files, one crate or a closely related pair). If the request is a whole phase, several
  tasks, or vaguely bounded ("continue Phase 1"), **do not start coding.** Propose a split
  into PR-sized tasks with acceptance criteria. Once the owner agrees, write the split
  to `docs/plan/phase-N.md` (with a short summary in `PLAN.md` §8), then work on the first
  task only.
- **Model:** check that the model fits the task (table below). If it doesn't, say so and
  ask whether to continue or switch. Don't ask for trivial tasks (under about 10 minutes of
  work).
- **Clarity:** if acceptance criteria or a design decision are missing and can't be
  derived from `PLAN.md`, the ADRs or the code, ask. Don't guess on architecture.

| Task type | Right model |
|---|---|
| Splitting a phase into tasks, ADRs, architecture, compositor core (rendering, input, protocols, window management), toolkit core (widget tree, text, input/IME, animation, accessibility), Overview/Spaces, hard debugging | **Opus** |
| CI, `xtask`, PKGBUILDs, archiso, settings panes, more widgets or apps that follow an existing pattern, tests, docs, research subagents | **Sonnet** |
| Renames, formatting, typo and one-line fixes | **Haiku** (or Sonnet) |

Rule of thumb: Opus sets a pattern, and Sonnet repeats it.

### 2. Stop and ask during work when…
- the task turns out bigger than estimated (it needs a new crate, a cross-crate interface
  change, or more than about 15 files): stop, report what you found, and propose a split;
- a fix needs a new dependency (especially C), a new protocol or IPC, or a change to a
  decision in `PLAN.md` or an ADR;
- you've made **two failed attempts** at the same bug or build error: summarise the
  hypotheses and evidence, and ask before trying more;
- the next step needs **manual or hardware work**: installing a VM, BIOS, partitioning,
  testing on real PCs, anything that needs clicking through a GUI. Write exact steps as
  ssh commands run from the Mac, following `docs/dev-setup.md` §2, into
  `docs/checks/phase-N.md` (reusable commands go into `docs/dev-setup.md` §1), and let
  the owner run them. Don't
  burn tokens on trial and error;
- the context is getting long and the task isn't close to done: suggest `/compact` with a
  focus (for example "keep failing test, hypothesis, files touched"), or wrap up with a
  handoff (step 4) so a fresh thread can continue.

### 3. Save tokens while working
- Read only what you need: the relevant `PLAN.md` section, the task in
  `docs/plan/phase-N.md`, and the files involved. Don't
  re-read whole crates.
- Keep tool output short: `cargo check --message-format short`, `| tail -n 50`, and single
  tests (`cargo test -p <crate> <name>`). Never dump full build logs into context.
- Run the full `cargo xtask check` once before handing off, not after every edit.
- **Subagents:** only for independent research or exploration that would otherwise flood
  the context. Use Sonnet by default. Use Opus only when the outcome is an architecture
  judgement and a Sonnet report was inconclusive. Give each subagent:
  - specific questions,
  - judgement criteria,
  - an output limit (for example "under 300 words, recommendation plus source links").

  Don't spawn subagents for routine coding, and don't let them re-do work you already have.
- **Never run multi-agent Workflows** unless the owner explicitly asks for one.
- Prefer separate threads, run by the owner, over subagents for parallel independent tasks.

### 4. Hand off at the end of every task
The repo is the memory between threads, not chat history. Before finishing:
- tick the boxes in `docs/plan/phase-N.md` (and update the plan if the implementation
  diverged);
- write an ADR for any decision you made;
- commit on a branch, open or update the PR, and fill in the testing notes (macOS, VM, real
  hardware: which of these were done and which were not);
- end with a short **Next** line: the next task, its suggested model, and anything the
  owner must do by hand first.

## Platform matrix
| Crate group | macOS (dev host, aarch64) | Linux aarch64 | Linux x86_64 |
|---|---|---|---|
| `neue-paint`, `neue-theme`, `neue-kit`, `neue-platform` (winit), `neue-config`, `neue-ipc`, `neuectl`, `neue-desktop` (pure parts), apps' UI, widget gallery | ✅ portable | ✅ | ✅ |
| `neue-comp`, `neue-shell`, services, `neue-platform` (Wayland), anything using D-Bus, DRM, udev or PipeWire | ❌ | ✅ | ✅ |

- Gate Linux-only code with `#[cfg(target_os = "linux")]` at module or crate level.
  Linux-only crates must still be members of the workspace; exclude them from macOS CI with
  `cargo xtask` filters, not by commenting them out.
- Apps must be able to start on macOS through the winit backend, using mocked services, so
  their UI can be iterated on natively.

## Commands
> `cargo xtask` lives in `xtask/` (alias in `.cargo/config.toml`). `check`, `golden`,
> `linux` and `linux-amd64` work; the other subcommands print "not implemented yet" until
> their tasks land. `check` lints with `--all-features`. Golden crates are listed in
> `GOLDEN_CRATES`. Linux-only crates are listed in `LINUX_ONLY_CRATES`
> (`xtask/src/main.rs`) and skipped by `check` on other hosts. Machine names default to
> `neueos-arm64` / `neueos-amd64`; override with `NEUE_ORB_ARM64` / `NEUE_ORB_AMD64`.

```sh
cargo xtask check          # fmt --check + clippy -D warnings + tests for the current host
cargo xtask gallery        # run the widget gallery (works on macOS)
cargo run -p neue-paint --example poc --features gpu  # paint PoC window (GPU, macOS)
cargo xtask golden --bless # regenerate golden images (review the diffs!)
cargo xtask linux <cmd>    # run <cmd> inside the OrbStack Arch machine (arm64)
cargo xtask linux-amd64 <cmd>  # same, in the amd64 (Rosetta) machine
cargo xtask vm sync|run    # sync to the Parallels VM and start a session there
cargo xtask package <crate>    # build a pacman package via the PKGBUILD in distro/pkgbuilds
cargo xtask iso [--arch x86_64|aarch64]
```

### Driving the running desktop
`neuectl` (the compositor's IPC) is the interface for automation and verification.
- `neuectl windows`, `neuectl outputs`, `neuectl focused-window` list state (`--json` for
  scripts). `neuectl spaces` arrives with Phase 4.
- `neuectl actions` lists actions; `neuectl action <name> [--window ID]` triggers one.
- `neuectl screenshot <path>` captures the screen (task 1.16).
- `neuectl events` (alias of `subscribe`) streams window events as JSON lines.

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
- Update the checkboxes in `docs/plan/phase-N.md` when you finish roadmap items. If an implementation diverges
  from the plan, update the plan in the same PR.
- Private Wayland protocols live in `crates/neue-protocols/` as XML, with documentation in
  `docs/protocols/`.
