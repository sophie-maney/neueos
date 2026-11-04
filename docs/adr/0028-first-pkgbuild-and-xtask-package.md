# 0028: The first PKGBUILD and `cargo xtask package` (task 1.20)

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
The session stack (`neue-comp`, `neuectl`, `neue-session` and its units, ADR 0027) is
installed by hand with `install`. Phase 1 needs a real pacman package that builds on both
architectures and installs in the VM; later PKGBUILDs (shell, apps, the ISO) copy the pattern.

## Decision
1. **One split PKGBUILD** at `distro/pkgbuilds/neue-comp/PKGBUILD`, pkgbase `neue-comp`,
   packages `neue-comp`, `neuectl` and `neue-session` (units, `wayland-sessions` entry, an
   example greetd config in `/usr/share/neueos/greetd/`, not `/etc`). `neue-session` depends on
   the other two. One `cargo build` serves all three.
2. **Build flags:** `--release --frozen --no-default-features --features neue-comp/tty`,
   so the shipped compositor has no winit and no X11 crates (ADR 0001). `options=(!debug !lto)`.
3. **Source** is a tarball of the working tree (`git ls-files` tracked and untracked-not-ignored
   files, so uncommitted edits are packaged), made by xtask. No version tags exist yet; `pkgver`
   stays `0.0.0` until releases do.
4. **Dependencies are fetched, not vendored.** `prepare()` runs `cargo fetch --locked`, which
   also fetches the pinned Smithay git rev from `Cargo.lock`. `cargo vendor` would put every
   workspace dependency (wgpu, vello, ...) in the tarball for three small binaries. The plan
   said "vendored git deps"; the lock file gives the same pin. Revisit for an offline or
   reproducible build (ISO, an AUR/own repo).
5. **`cargo xtask package <crate> [--arch aarch64|x86_64|all]`** maps a crate to its PKGBUILD
   directory (`PACKAGES` in xtask), stages it under `target/package/<dir>/`, and runs
   `makepkg --syncdeps --noconfirm --force --cleanbuild` in the OrbStack machine for each
   architecture (default aarch64). Packages land in `target/package/<arch>/`. Cargo's target
   directory and `BUILDDIR` live in `target/<machine>-package` and `target/<machine>-pkgbuild`,
   outside `$srcdir`, so rebuilds are incremental. Any of the three crate names builds all three
   packages.

## Consequences
- No new dependency. The Arch `rust` or `rustup` package satisfies `makedepends=(cargo)`;
  with rustup, `rust-toolchain.toml` picks the toolchain.
- `cargo fetch` needs network inside the build; fine for dev machines, not for a clean-chroot
  or offline build.
- The Rosetta amd64 machine is slow; use `--arch aarch64` day to day.

## Alternatives considered
- **Three PKGBUILDs:** triples the build, since the workspace compiles together.
- **`cargo vendor` in the tarball:** see decision 4.
- **`git archive HEAD`:** misses uncommitted edits, which defeats the dev loop.
