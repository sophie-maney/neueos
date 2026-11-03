# neue-paint proof of concept (task 0.6, 2026-11-03)

Input for ADR 0002 (toolkit stack). Code: `crates/neue-paint`.

## What was built
- `Scene`: a small recorded command list in logical pixels (fill, stroke, blurred rounded
  rect, clip/opacity layer). Each command carries its own transform.
- Replay into `vello_cpu::RenderContext` and `vello_gpu::Scene` through one macro
  (`src/replay.rs`). The two APIs match method for method, so there is no backend trait.
- `cpu::CpuRenderer` (always built) and `gpu::GpuRenderer` (`gpu` feature, wgpu).
- `ProceduralTexture`: tileable textures generated **per device scale** and cached per
  `(cache_key, scale)`. The CPU backend samples them as `ImageSource::Pixmap`. The GPU
  backend uploads them once to vello_gpu's atlas (`ImageSource::Pixmap` is
  `unimplemented!` on the GPU).
- Materials: procedural linen (hash and value noise, plain weave, only `+ * floor`, so
  deterministic) and the Aqua gel push button (rim, four-stop gel with a hard midline,
  gloss, shadow and etch). Both have light and dark variants, and the button has normal
  and pressed states. No label yet: text comes with neue-kit.
- `examples/poc.rs`: winit 0.30 window, wgpu 30, vello_gpu. Runs on macOS (Metal),
  output matches the CPU goldens by eye.
- Golden test `tests/golden.rs`: four PNGs (light/dark × 1x/2x) in `tests/golden/`.
  `cargo xtask golden [--bless]`.

## Decisions
| Decision | Why |
|---|---|
| Own command list, replayed per backend | No shared scene type in vello 0.3 (R4). The macro keeps it at about 100 lines. |
| Pin `vello_cpu`/`vello_gpu`/`vello_common` to `=0.3.0` | Breaking releases every 1–2 months. |
| GPU backend behind the `gpu` feature, off by default | Compositor chrome and tests shouldn't pull in wgpu. `xtask check` lints with `--all-features`. |
| Goldens use `Quality::Reference`: f32 pipeline, `Level::baseline()`, one thread | As reproducible as vello allows. `Quality::Fast` (u8, detected SIMD) is for screens. |
| Tolerance: ≤2 per channel, ≤0.1% of pixels over that | R4 numbers, plus 1 extra for the shared PNG round trip. Tightened if CI shows exact matches. |
| Material colours live in `neue-paint` for now | `neue-theme` doesn't exist yet. Marked `TODO(phase 2)`. |
| Surface must use a non-sRGB format | vello writes sRGB-encoded values. An `*Srgb` surface would encode twice. |

## Findings
- wgpu 30 API changes against older examples: `queue.present(frame)` (not
  `frame.present()`), `SurfaceConfiguration::color_space` is required,
  `get_current_texture()` returns the `CurrentSurfaceTexture` enum, and
  `InstanceDescriptor::new_with_display_handle_from_env` takes the winit display handle,
  which is needed for GL on Wayland (vm-graphics.md).
- `vello_gpu::Renderer::render` needs a depth view of the target size. `GpuRenderer`
  caches one per size.
- Cross-platform golden results: see the table below.

| Host | Result |
|---|---|
| macOS aarch64 (blessed here) | exact |
| Linux x86_64 (OrbStack, Rosetta, SSE baseline) | passes within tolerance (max diff not logged) |

## Not done (from R4's list; carried to ADR 0002 or Phase 2)
- CPU-vs-GPU pixel comparison within tolerance (needs a GPU adapter in tests; only a visual
  check was done).
- Sweep gradient and drop-shadow filter layer checks.
- Timing a 3000×60 title bar with text, and the Smithay `MemoryRenderBuffer` upload.
- Running the example in the Parallels VM on virgl GL.
