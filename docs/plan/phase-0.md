# Phase 0: Foundations

Task list for Phase 0 of the roadmap in `PLAN.md` §8. Architecture and decisions
live in `PLAN.md` and `docs/adr/`.

One task = one thread = one PR. Do them roughly in this order; 0.1 → 0.2 → 0.3 are
sequential, 0.4 can start once R2 is in, and 0.6 / 0.7 can run in parallel.

- [x] **0.0 Research spikes (R1–R4)** · Opus thread launching Sonnet subagents.
      Desk research only, results in `docs/research/`:
      R1 global menus on Wayland per toolkit, R2 Arch Linux ARM on Parallels,
      R3 Masonry evaluation, R4 vello_cpu maturity.
      *Accept:* four notes, each with a recommendation, open questions and sources.
- [x] **0.1 Workspace skeleton** · Sonnet. `LICENSE` (GPL-3.0-or-later), root
      `Cargo.toml` (edition 2024, `[workspace.dependencies]`, `[workspace.lints]`),
      `rust-toolchain.toml`, `rustfmt.toml`, `.gitignore`, and one stub portable crate
      (`crates/neue-paint`) plus one stub Linux-only crate (`compositor/neue-comp`, gated
      with `cfg(target_os = "linux")`).
      *Accept:* `cargo fmt --check`, `cargo clippy --workspace -- -D warnings` and
      `cargo test --workspace` pass on macOS.
- [x] **0.2 `cargo xtask` skeleton** · Sonnet. `xtask/` crate with `check` (fmt + clippy
      + tests, skipping Linux-only crates on macOS through a crate list, not comments),
      and `linux` / `linux-amd64` that run a command in the OrbStack machines. Other
      subcommands print "not implemented yet". Update "Commands" in `AGENTS.md`.
      *Accept:* `cargo xtask check` passes on macOS; `cargo xtask linux cargo check`
      works once 0.4 is done.
- [x] **0.3 CI** · Sonnet. GitHub Actions running `cargo xtask check` on macOS (aarch64),
      Linux x86_64 and Linux arm64, with a pinned toolchain and caching.
      *Accept:* all three jobs green on the PR; required checks enabled on `main`.
      (Jobs green in PR #6. Enabling required checks on `main` is an owner step in GitHub settings.)
- [x] **0.4 Dev machines + `docs/dev-setup.md`** · Sonnet writes, **owner runs**.
      Exact steps for the OrbStack Arch machines (arm64 and amd64) and the Parallels
      Arch Linux ARM VM (route chosen from R2), file sync (Parallels Tools or
      virtiofs/SSH + rsync), and the Rust toolchain in each.
      *Accept:* owner has all three machines; the doc matches what was actually done.
      (Owner ran `docs/dev-setup.md`; results summarised in its §3. GRUB_UEFI used, since
      archboot offers no systemd-boot.)
- [x] **0.5 VM graphics checks** · Opus, owner at the VM. Record `eglinfo` (virgl vs.
      llvmpipe), run Smithay's `anvil` on the VM's DRM device and nested under a stock
      Wayland session, and run a vello demo in the VM. Results in
      `docs/research/vm-graphics.md`.
      *Accept:* anvil shows a window on DRM in the VM (or a documented blocker and
      fallback).
      (virgl GLES 3.0, Vulkan only llvmpipe. anvil works on DRM and nested in weston once
      archboot's kmscon is replaced by getty. `vello_gpu` renders on virgl GL. Run over SSH and
      checked with `prlctl capture`; a few checks at the VM window are listed as open questions.)
- [x] **0.6 `neue-paint` proof of concept** · Opus. Procedural linen and an Aqua push
      button (normal + pressed, light + dark) as `neue-paint` code; an example that
      shows them in a winit window via vello on macOS; a test that renders them with
      `vello_cpu` and compares against a committed golden PNG at scale 1 and 2.
      *Accept:* example runs on the Mac; golden test passes in `cargo xtask check`.
      (Recorded `Scene` replayed into `vello_cpu` and `vello_gpu`; the example uses
      `vello_gpu` on wgpu behind the `gpu` feature. Notes for ADR 0002 in
      `docs/research/paint-poc.md`.)
- [x] **0.7 Global-menu verification** · Sonnet writes the checklist from R1, **owner
      runs it** in the VM under a stock Plasma Wayland session (Qt6, GTK3, Chromium/
      Electron, Firefox). Results added to `docs/research/global-menu.md`.
      *Accept:* a coverage table with tested/observed status per toolkit.
      (Run by Claude over SSH in the VM's Plasma session on 2026-11-03, with no input injection;
      results in `global-menu-checklist.md`, summary in `global-menu.md`. Open items listed there.)
- [x] **0.8 ADRs 0001–0005** · Opus. 0001 Smithay, 0002 toolkit stack and Masonry (from
      R3, R4, 0.6), 0003 IPC, 0004 config format, 0005 global-menu strategy (from R1,
      0.7). Then split Phase 1 into tasks.
      *Accept:* five ADRs merged; Phase 1 has a task list.
      Plan corrections from R1–R4 to settle in these ADRs (then fix PLAN.md §3.2, §6.3):
      - GTK3/GTK4 on Wayland export menus via `gtk_shell1` + `org.gtk.Menus`, not
        `appmenu-gtk-module` (X11-only), so neue-comp also needs `gtk_shell1`.
      - Linebender's vello compute crate is now experimental; `vello_cpu` +
        `vello_gpu` (ex-`vello_hybrid`) are the maintained pair. `neue-paint` should
        own a small command list and replay it per backend.
      - Masonry has its own layout (not taffy), no springs, and a paint API moving to
        the `imaging` crate; crates.io release lags `main` by a year.
      (ADRs in `docs/adr/`; PLAN.md §3.1, §3.2, §3.3 and §6.3 updated, Phase 1 split in `phase-1.md`.)
- **Exit:** CI is green on 3 platforms. Smithay runs in the VM. Proof-of-concept renders
  on the Mac.
