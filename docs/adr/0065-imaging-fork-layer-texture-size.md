# 0065: Patch `imaging` from our fork so layer textures can grow past 4096 px

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.37, runbook step 16.2. On macOS, zooming the gallery on a 5K display stretched the
last frame instead of laying the window out at its new size. winit delivered every
`Resized`, and the driver relaid and repainted at 5120×2672 px, but every frame failed:

```
rendering with Vello Hybrid failed: IntermediateTexture(TooLarge { width: 5120,
height: 100, max_width: 4096, max_height: 4096 })
```

vello_gpu renders layers (clips, opacity, blends) into intermediate textures whose size is
capped by `LayersConfig::max_texture_size`, 4096 px by default. A layer as wide as the
window failed the whole frame, nothing was presented, and Core Animation scaled the last
drawable to the new bounds. Resizing by hand worked because it never got past 2048 pt
(4096 px). The same failure hits any output wider or taller than 4096 px, on Linux too.

`imaging_vello_hybrid` (ADR 0030) builds the vello_gpu renderer with
`vello_gpu::Renderer::new`, which uses the default settings, and exposes no way to pass
others. Upstream `forest-rs/imaging` has no newer rev that changes this.

## Decision
1. **Fork `imaging`** as `github.com/sophie-maney/imaging`, branch `neueos`, commit
   `b995a49`: upstream rev `f3f932a` (the rev ADR 0030 pins) plus one patch.
   `VelloHybridRendererState::new` calls `Renderer::new_with` with
   `layers_config.max_texture_size = u16::MAX`, which vello_gpu clamps to the device's
   `max_texture_dimension_2d` (16384 on Apple GPUs). GitHub's fork endpoint failed with
   HTTP 500 on the day, so the repo is a plain copy, not a GitHub fork.
2. **Point the build at it with `[patch."https://github.com/forest-rs/imaging"]`** in the
   root `Cargo.toml`. That replaces the source for our own `imaging` pin and for the Masonry
   fork's alike, so the Masonry fork needs no new rev. Every package of that source in
   the tree is patched (`imaging`, `imaging_wgpu`, `imaging_vello_cpu`,
   `imaging_vello_hybrid`); otherwise two copies of `imaging` would be built.
3. ADR 0030 §4's rules apply to this fork too: patches on top of an upstream rev, the
   `neueos` branch never rewritten, bumps in their own PR, nothing upstreamed for now
   (owner's call).

## Consequences
- Windows and outputs up to the device's texture limit render. Beyond that (above 16384 px
  on Apple GPUs, 8192 px on some GL drivers) a full-size layer still fails; no real
  display gets there today.
- Intermediate textures only grow to the largest layer drawn, so the memory cost only
  shows up for large windows. A 5120 px wide layer texture costs about 2 MB per 100 rows.
- Bumping `imaging` now means rebasing the patch onto the new rev in the fork, then moving
  both the pin and the `[patch]` rev.
- If upstream exposes renderer settings, drop the fork and set the size from
  neue-platform instead.
