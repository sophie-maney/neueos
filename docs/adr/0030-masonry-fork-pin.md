# 0030: Pin Masonry to a fork and `imaging` to a git rev

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
ADR 0002 §4 puts `masonry_core` under neue-kit from a pinned git rev, if Masonry's
dependencies can be reconciled with ours. Task 2.1 (`docs/research/masonry-spike.md`) found:
- Masonry `main` pins parley 0.8, accesskit 0.24, wgpu 28 and crates.io `imaging` 0.0.1,
  whose CPU backend renders through `vello_cpu` 0.0.7;
- `imaging` `main` already matches our `vello_cpu`/`vello_gpu` 0.3 and wgpu 30 pins, but is
  unreleased;
- linebender/xilem#1864 (open) moves Masonry to parley 0.12 and accesskit 0.25;
- Masonry's defaults pull in two C libraries: fontconfig (parley's `system` feature) and
  libdeflate (`oxipng` in `masonry_testing`).

## Decision
1. Pin `masonry_core`, `masonry_testing` and `masonry_winit` to
   `github.com/sophie-maney/xilem` rev `499f423` (branch `neueos`): upstream `dc49687`, plus
   #1864, plus our own patches:
   - `imaging` at the git rev below and wgpu 30 (with the API port);
   - parley without `system`;
   - `oxipng` as an optional, default-on feature of `masonry_testing` (we turn it off).
2. Pin `imaging` to `github.com/forest-rs/imaging` rev `f3f932a`, the same rev the fork
   uses.
3. `neue-paint` gets an `imaging` feature with `ImagingReplayer`, the third replay target
   (ADR 0002 §4). Masonry widgets paint neue-paint scenes through it.
4. Same rules as the Smithay fork (ADR 0029):
   - patches sit on top of an upstream rev;
   - the `neueos` branch is never rewritten;
   - bumps happen in their own PR and re-run the goldens;
   - nothing is upstreamed for now (owner's call).
   Move back to upstream pins once #1864 is merged and `imaging` is released with
   vello 0.3 support.
5. `masonry_winit` is a macOS-only dev-dependency of `neue-paint`, for the spike example
   only. Its winit pulls in X11 crates on Linux, and task 2.4 replaces it with our own
   driver.

## Consequences
- One copy each of vello_cpu/vello_gpu, wgpu, parley, accesskit, kurbo and peniko in the
  tree. No new C libraries.
- Apps get no system fonts until a later ADR adds fontconfig discovery (needed by 2.7
  and 2.10 at the latest).
- `masonry_testing` renders with vello_cpu's u8 pipeline. Our goldens use f32, so the two
  agree within tolerance, not bit for bit.
- Every fork or `imaging` rev must stay reachable on GitHub; `cargo fetch --locked` uses
  them like any git dependency.
