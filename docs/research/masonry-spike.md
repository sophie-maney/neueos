# Masonry spike A: versions and paint (task 2.1, 2026-11-04)

## Result: the versions reconcile, and paint works. The gate's first half passes.
`masonry_core`, `masonry_testing` and `masonry_winit` build together with our pins, using one
copy each of `vello_cpu`/`vello_gpu` 0.3, wgpu 30, parley 0.12, accesskit 0.25, kurbo 0.13
and peniko 0.6. That takes a small fork (below) and a git pin of `imaging`. A Masonry widget paints
the neue-paint Aqua button through a new `imaging::Painter` replay target. Its
`masonry_testing` render matches neue-paint's own goldens on macOS aarch64, Linux aarch64
and Linux x86_64. The second half of the gate (the sctk driver, xdg-popup and
text-input-v3) is task 2.2.

Effort: about half a day, well under the ADR 0002 §4 budget of two days.

## Upstream state (2026-11-04)
- Masonry `main` (`dc49687`, 2026-11-03) pins parley 0.8, accesskit 0.24, wgpu 28 and
  crates.io `imaging` 0.0.1. That release's `imaging_vello_cpu` 0.0.1 renders through
  `vello_cpu` 0.0.7, and its brushes are `peniko::Brush`.
- `imaging` `main` (`f3f932a`, 2026-10-17) already targets `vello_cpu`/`vello_gpu` 0.3 and
  wgpu 30, which match our pins. It has not been released, and its crate versions are still 0.0.1/0.0.2.
  Since 0.0.1 it gained its own `Brush`/`ImageBrush` (with external images), filter
  transforms and validation. `masonry_core` compiles against it unchanged, because
  `From<peniko::Brush>` impls cover the old call sites.
- linebender/xilem#1864 (open, review requested) moves Masonry to parley 0.12 and
  accesskit 0.25. parley 0.12 dropped its `accesskit` feature, so the PR adds
  `masonry_core/src/core/text_accessibility.rs` (+495 lines, a port of parley's old
  AccessKit glue). It merges cleanly on `main`.

## Fork cost
Fork `sophie-maney/xilem`, branch `neueos`, rev `499f423` (ADR 0030). On top of upstream
`dc49687`:

| Change | Size | Upstreamable? |
|---|---|---|
| Merge of #1864: parley 0.12, accesskit 0.25 | +495 new file, ~60 lines elsewhere, 160 screenshots | it's upstream's own PR |
| `imaging*` → git `f3f932a`, wgpu 28 → 30 (workspace `Cargo.toml`) | 8 lines | yes, once `imaging` releases |
| wgpu 30 API in `masonry_imaging` (1 line) and `masonry_winit` (~30 lines: instance descriptor, `CurrentSurfaceTexture`, `queue.present`, surface colour space, objc2 Metal layer call) | ~30 lines | yes |
| No C dependencies: parley without `system` (no fontconfig); `oxipng` made an optional feature of `masonry_testing` (no libdeflate) | ~15 lines | `oxipng` feature: maybe; parley: no, it's our policy |

Our own patches total about 55 lines over 7 files. Rebasing onto a newer upstream should
take under an hour while #1864 is open. Once it's merged, it's mostly the wgpu and `imaging`
pins. The biggest ongoing cost is `imaging`'s churn: it is pre-alpha and unreleased, and we
pin a commit.

## Dependency footprint
- No duplicate rendering, text or a11y crates. The only duplicates are small macOS-only
  ones (objc2 0.5 via `copypasta` in `masonry_winit`) and `pollster` 0.4/1.0.
- Vello's compute renderer (`vello`) and Skia are not in the lockfile. `masonry_winit` is
  built with `imaging_vello_hybrid` (vello_gpu) only.
- New licences: Apache-2.0, MIT, BSD-2/3, BSL-1.0, BlueOak-1.0 (anymap3, dual-licensed),
  Zlib. All are compatible with GPL-3.0-or-later.
- No C libraries on Linux. `masonry_winit` pulls in winit's default X11 features, so it is
  a macOS-only dev-dependency of `neue-paint`, and task 2.4 replaces it.

## Paint
- `neue_paint::imaging::ImagingReplayer` (feature `imaging`) replays a `Scene` into any
  `imaging::Painter`. Commands map one to one: fill, stroke, blurred rounded rect, layer to
  group (clip and opacity), glyph run. Coordinates stay logical, because Masonry applies the
  window scale at render time. Procedural textures are generated at `PaintCtx::scale_factor()`
  and passed as premultiplied `ImageData` image brushes, cached per (texture, scale).
- Brush transforms have the same convention as vello (relative to the geometry
  transform), so the texture maths is shared with the vello replay (`replay.rs`).
- `tests/masonry.rs`: a `masonry_core` widget paints the button tile (linen and Aqua button)
  through the replayer. `masonry_testing` renders it, and the output is compared with
  `tests/golden/button-*.png` (rendered by neue-paint's `CpuRenderer`), using the existing
  tolerance (channel ≤ 2, ≤ 0.1 % of pixels over). Measured on macOS: 80–90 % of pixels
  identical, the worst channel difference is 2, and no pixel is over tolerance. The
  remaining difference is the pipeline: `imaging_vello_cpu` uses the u8 pipeline, while
  our reference goldens use f32.
- `examples/masonry_button.rs` (macOS): the same widget in a `masonry_winit` window on
  vello_gpu. It shows the pressed state while the pointer is held.

## Open items for later tasks
- **System fonts.** With parley's `system` feature off, `RenderRootOptions::use_system_fonts`
  has no effect, and apps get only the fonts we register. Apps will need fontconfig
  discovery on Linux, which is a C dependency, so it needs an ADR when 2.7 or 2.10
  needs it.
- **Golden harness (2.9).** `masonry_testing` can't choose the f32 pipeline. Either keep
  comparing within tolerance, or render goldens with our own harness around `RenderRoot`
  and `CpuRenderer`.
- **`imaging` image brushes** now support `ExternalImage`, which may help with
  `VisualLayerKind::External` and backdrop content (Phase 3).
- Don't upstream anything for now (owner's call). Track #1864 and an `imaging` release, and
  move back to upstream pins once both have landed.
