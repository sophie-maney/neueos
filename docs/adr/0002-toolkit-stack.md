# 0002: Toolkit stack: neue-paint on vello_cpu/vello_gpu, Masonry for the widget tree

- **Status:** Accepted. The Masonry gate (Decision 4) passed on 2026-11-04: see the amendment.
- **Date:** 2026-11-03

## Context
PLAN.md §3.2 assumed vello's compute renderer for the GPU, taffy for layout, and left
"Masonry or our own tree" open. Research and the 0.6 proof of concept changed the picture:
- R4 (`docs/research/vello-cpu.md`): vello's compute renderer moved to `research/` and is
  experimental. `vello_cpu` + `vello_gpu` (ex-`vello_hybrid`) are the maintained pair. Their
  APIs match closely, but there's no shared scene type. Both ship breaking releases every
  1-2 months.
- 0.5 (`docs/research/vm-graphics.md`): the VM has GLES 3.0 (no compute) and llvmpipe
  Vulkan. `vello_gpu` on wgpu GL is hardware-accelerated there; compute vello is not.
- 0.6 (`docs/research/paint-poc.md`): `neue-paint` records its own `Scene` and replays it
  into `vello_cpu` and `vello_gpu` with one macro. Linen and the Aqua button render the
  same on both; goldens pass on macOS aarch64 (exact) and Linux x86_64 (within tolerance).
- R3 (`docs/research/masonry.md`): `masonry_core` gives full paint control, a driver API
  we can implement for Wayland, AccessKit, parley text editing with IME, and a
  `vello_cpu` test harness. But it has its own layout (not taffy), no springs, paints
  through the pre-alpha `imaging` crate, and crates.io lags `main` by a year. crates.io
  `imaging_vello_cpu` 0.0.2 still depends on `vello_cpu` 0.0.9 while we pin 0.3.0.

## Decision
1. **Rendering backends:** `vello_cpu` and `vello_gpu` only. Not vello's compute renderer,
   not tiny-skia, not Skia/Cairo. Pinned exactly (`=0.3.x`) in the workspace; bumps are
   their own PRs that re-run the goldens.
2. **`neue-paint` owns the scene.** Materials and widgets emit a small recorded command
   list (`neue_paint::Scene`, kurbo/peniko types, logical pixels) that is replayed per
   backend. This is the only drawing API our code writes against. Procedural textures are
   generated per device scale into pixmaps and used as image paints (no turbulence filter
   exists in vello).
   - CPU: compositor chrome, golden tests, any software fallback.
   - GPU: apps and the shell, on wgpu. wgpu setup must pass the display handle to the
     `Instance`, request the adapter's own limits, accept `Fifo` presentation, and use a
     non-sRGB surface format (0.5/0.6 findings).
   - Avoid features `vello_gpu` panics on (mask layers, filter graphs, some blend modes)
     or gate them to the CPU path. Golden tests are the authority on looks.
3. **Text and accessibility:** parley (with fontique) for layout and editing, AccessKit for
   accessibility. No other text or a11y stack.
4. **Widget tree: build on `masonry_core`, behind neue-kit's own API.**
   - Depend on `masonry_core` and `masonry_testing` only, from a pinned git rev. Not
     `masonry` (stock widgets and theme), `masonry_winit`, or Xilem.
   - neue-kit exposes its own widget, app-model and theme API; Masonry types don't leak
     into app crates. That keeps a later swap to our own tree possible.
   - Masonry widgets paint through `imaging::Painter`. neue-paint gets a third replay
     target for it, so one `Scene` still feeds every backend.
   - `neue-platform` writes our own drivers for `RenderRoot`: smithay-client-toolkit on
     Wayland (xdg-shell, layer-shell, xdg-popup for Masonry layers, text-input-v3,
     fractional scale) and winit on macOS. `masonry_winit` may be used as a shortcut in the
     first Phase 2 task only.
   - **Gate:** the first Phase 2 task is a spike that makes `masonry_core` (at a pinned
     rev), `imaging` and our `vello_cpu`/`vello_gpu` pins build together, renders a
     neue-paint button inside a Masonry widget on both backends, and drives it from the sctk
     driver with text-input-v3. If the versions can't be reconciled in about two days, or
     the driver can't map popups to xdg-popup, fall back to **our own thin tree** on the
     same crates (R3 estimates 8-12k lines, 6-10 weeks) and supersede this point with a new
     ADR.
5. **Layout:** Masonry's measure/layout protocol. **taffy is dropped** from §3.2; it may come
   back as a wrapper widget if a grid layout needs it.
6. **Animation:** neue-kit builds spring animation on Masonry's `on_anim_frame`, with the
   presets and the reduced-motion flag from `neue-theme`.
7. **Backdrop blur** (menus, sheets, HUDs, the menu bar) is the compositor's job, not the
   toolkit's: clients mark regions, `neue-comp` blurs behind them. The protocol
   (`ext-background-effect-v1` or a private one) is decided in Phase 3 (amended 2026-11-04: deferred from Phase 2; the menu bar is the first user). Decided in ADR 0071: `ext-background-effect-v1`.

## Amendment 2026-11-04: gate result (tasks 2.1 and 2.2): **go**
Both halves of the Decision 4 gate pass, so Masonry stays and 2.4 onward proceed as planned.
- **Versions and paint (2.1):** reconciled through our fork (ADR 0030); the neue-paint
  button paints inside a Masonry widget and matches its golden.
- **sctk driver (2.2):** `neue_platform::wayland` drives `RenderRoot` on
  smithay-client-toolkit 0.21. Checked under neue-comp in the VM with `neuectl screenshot`
  (`docs/checks/phase-2.md` §1): an xdg-toplevel and a layer-shell surface render with
  vello_gpu on wgpu (virgl GL); fractional scale 1.5 with viewporter re-renders sharp;
  text-input-v3 preedit and commit reach a Masonry `TextInput`; the `Selector` menu, a
  Masonry layer, is an xdg-popup with a grab, its items take clicks, and an outside click
  dismisses it.
- **How layers map to popups:** Masonry keeps laying out and hit-testing every layer in
  window coordinates. The driver splits `RenderRoot::redraw`'s `VisualLayerPlan` at layer
  roots, paints each layer's run into its popup's surface (translated by the popup's
  configured origin), and adds that origin back to pointer input on the popup. A compositor
  flip or slide is fed back with `reposition_layer`. Nothing in Masonry had to change.
- **Dependencies this adds** (all MIT/Apache-2.0): `smithay-client-toolkit` with its
  `system` feature (libwayland-client, dlopen'd as winit already does, because wgpu needs a
  `wl_display*`) and `xkbcommon` (the libxkbcommon ADR 0001 accepts, now on the client side
  as well), `raw-window-handle`, and the fork's `masonry_imaging` (vello_gpu renderer). The
  fork's `masonry` crate (stock widgets) is allowed in spike examples only, like
  `masonry_winit` in 2.1.
- **Found on the way** (for 2.5/2.6 and Phase 3, details in `docs/research/masonry-spike.md`):
  neue-comp sends no preferred scale to layer surfaces; neue-comp has no
  `virtual-keyboard`, so fcitx5 5.1 can't bind as an input method (the spike uses a scripted
  input-method-v2 probe); Masonry's menus close on window focus loss, so focus moving into
  a window's own popup must not be reported as one.

## Consequences
- Every renderer bump touches one crate (`neue-paint`) plus the goldens.
- We carry version-alignment work between Masonry's pins and ours until upstream releases
  catch up; the spike measures how much.
- Most of the v1 widget set (menus, tables, outline view, all Aqua controls) is ours to
  write either way; Masonry saves the tree, focus, text editing, a11y plumbing and tests.
- PLAN.md §3.2 is updated: GPU backend is `vello_gpu`, layout is Masonry's, taffy removed.

## Alternatives considered
- **vello compute renderer for the GPU:** experimental upstream, and no compute on the
  VM's GLES 3.0.
- **`imaging` as our scene type instead of our own command list:** pre-alpha (0.0.x) and its
  crates.io release pins an old `vello_cpu`. Our list is about 100 lines; revisit if
  `imaging` stabilises.
- **Own widget tree from day one:** 6-10 weeks before the first app, mostly re-doing IME and
  a11y plumbing Masonry already has. Kept as the fallback.
- **Masonry with its stock widgets/theme or Xilem:** we replace every widget's look, and an
  Elm-style app model is ours to design; extra layers would only add churn.

## Revisit when
The Phase 2 spike fails its gate; `imaging` reaches a stable release; vello_cpu/vello_gpu
commit to API stability.
