# vello_cpu research (2026-11-03)

## Facts
- `vello_cpu` 0.3.0, `vello_gpu` 0.3.0 (formerly `vello_hybrid`), `vello` 0.11.0, all released 2026-10-02. Licence Apache-2.0 OR MIT (GPL-3.0-or-later compatible). MSRV 1.89.
- `vello_cpu` ships every 1-2 months, usually with breaking changes. **No API stability guarantee.**
- **PLAN.md drift:** the `vello` compute renderer moved to `research/` and is called experimental. Linebender says `vello_gpu` will be the primary GPU renderer and `vello_cpu` is currently more mature. `vello_gpu` needs no compute shaders (good for the VM) but panics on mask layers, complex filter graphs and some blend modes. Needs an ADR: `vello_gpu` or `vello`.
- **One scene description?** Not natively. `vello_cpu::RenderContext` and `vello_gpu::Scene` have near-identical imperative APIs over shared kurbo/peniko types. `vello::Scene` differs. The forest-rs `imaging` crates (0.0.x) replay one recorded scene into vello, vello_hybrid, vello_cpu and Skia. Masonry uses them, but they are pre-alpha. `neue-paint` should own a small recorded command list and replay it per backend.

## Feature coverage
| Feature | vello_cpu | GPU difference |
|---|---|---|
| Linear/radial/sweep gradients | yes (check sweep in PoC) | none |
| Images, extend modes, bicubic, tint | yes | none |
| Clip, opacity, blend layers | yes | some blends panic |
| Mask layers | yes | panics |
| Blurred rounded rect, inset | yes, solid colour only | none |
| Filters | blur, drop shadow, offset, flood; graphs panic; none when multithreaded | similar |
| Turbulence/noise, lighting | **missing** | missing |
| Text | `glyph_run` via `glifo` 0.4 and skrifa; parley supplies runs | none |
| AA | analytic coverage; u8 (fast) or f32 (accurate) pipeline | |

Generate noise and linen procedurally into a seeded `Pixmap` and use it as a tiled image paint, which works on both backends.

## Determinism
Not bit-exact across platforms. Vello's suite shares one reference PNG across scalar, NEON, SSE, AVX2, AVX-512 and wasm. Per-channel tolerance is 0 (f32 scalar), 1 (f32 SIMD), 2 (u8), 1 (GPU), plus per-test `diff_pixels` allowances. Masonry compares exactly on a Linux/macOS/Windows CI matrix, but RGB only. A `SKIP_RENDER_TESTS` flag there may be stale; I didn't check.

**Strategy:** `f32_pipeline`, `RenderMode::OptimizeQuality`, single thread, pinned `RenderSettings.level`, bundled fonts only. Tolerance ≤1 per channel and ≤0.1% of pixels. Use our own ~40-line diff (`kompari` is git-only, not on crates.io; Masonry copied its diff function). Bless on macOS arm64; Linux x86_64 CI must pass. Add a non-blocking pinned-scalar job to see if exact match works.

## Compositor chrome
- **Performance:** no published numbers. A 2025 Linebender post has `vello_cpu` second to Blend2D on an M1 Pro, ahead of Skia and tiny-skia. A 3000×60 px bar should take about 1 ms or less on one thread (**unmeasured estimate**). Leave `multithreading` off.
- **Design:** render on change, not per frame. One cached texture per window; re-render on focus, title, hover, resize or scale. Nine-slice title bar (fixed caps, stretchable 1 px column) avoids re-render on resize. Cache shadows per (radius, scale, focus) and share across windows.
- **Upload:** `Pixmap` is premultiplied RGBA8 (bytes R,G,B,A), which is DRM `Fourcc::Abgr8888`. In Smithay 0.7 (MIT), use `MemoryRenderBuffer::new(Abgr8888, size, scale, ..)` with `MemoryRenderBufferRenderElement`; it tracks damage and uploads. Alternative: `GlesRenderer::import_memory(data, Abgr8888, size, false)`. Smithay blends as premultiplied.

## Recommendation
- **Golden tests:** adopt `vello_cpu`.
- **Chrome:** adopt it behind a `neue-paint` trait, with caching, pinned to `=0.3.x`.

## Plan B
- **tiny-skia** 0.12.0 (BSD-3-Clause, linebender org): stable and mature, CPU only. No layers, blur, shadow or text; own path types. Fallback for chrome, with a hand-written blur.
- **wgpu offscreen in tests:** driver-dependent (lavapipe vs Metal). Use only as a CPU-vs-GPU cross-check.
- **Skia/Cairo bindings:** C dependency, needs an ADR, conflicts with Rust-only. Reject.
- **Direct pixel code:** procedural materials can write into a `Pixmap` without the renderer.

## Risks
API churn (trait and pin mitigate). Filters limited. `Resources` lifecycle and glyph cache are rough or experimental. Cross-arch exactness unproven. GPU backend choice changed since PLAN.md.

## 0.6 PoC should test
1. Linen and Aqua button replayed into `vello_cpu` and the GPU backend; compare within tolerance.
2. Goldens at scale 1 and 2 on macOS arm64 and Linux x86_64; record max per-channel diff for u8/f32 and scalar/SIMD.
3. Sweep gradient, blurred rounded rect, drop-shadow layer.
4. Time a 3000×60 title bar with text, single thread, cold and cached.
5. Upload to a Smithay `MemoryRenderBuffer`; check for alpha halos.

## Sources
- https://github.com/linebender/vello (README, ARCHITECTURE.md, `vello_cpu/` README and CHANGELOG)
- https://github.com/linebender/vello/releases/tag/sparse-strips-v0.3.0
- https://github.com/linebender/vello/blob/main/vello_tests/vello_dev_macros/src/lib.rs
- https://github.com/linebender/xilem/blob/main/masonry_testing/src/screenshots.rs
- https://crates.io/crates/vello_cpu, /imaging, /tiny-skia
- https://linebender.org/blog/tmil-19/
- https://github.com/Smithay/smithay/blob/master/src/backend/renderer/element/memory.rs
