# Phase 2: Toolkit and design system v1

Task list for Phase 2 of the roadmap in `PLAN.md` §8. Architecture and decisions
live in `PLAN.md` and `docs/adr/`.

One task = one thread = one PR, as before. Order: the gate 2.1 → 2.2 first (ADR 0002 §4);
2.3 can run alongside it. Then 2.4–2.9 (the core), after which the widget tasks 2.12–2.24
mostly run in parallel. 2.26 closes the phase. Every widget task delivers a gallery page,
goldens (states × light/dark × scale 1 and 2), AccessKit roles and labels, and keyboard
operability, following the 2.9 pattern. If the gate fails (2.2), stop: 2.4 onward is
re-planned around our own tree. Deferred: the backdrop-blur protocol moves to Phase 3
(the menu bar is its first user; Phase 2 materials draw an opaque tinted fallback), and
the paper and leather materials move to Phase 6/7 (Widgets, Write).

- [x] **2.1 Masonry spike A: versions and paint** · Opus. Pin `masonry_core` and
      `masonry_testing`; reconcile `imaging` with our `vello_cpu`/`vello_gpu` 0.3 and
      parley 0.12 pins; add an `imaging::Painter` replay target to neue-paint; a Masonry
      widget paints the Aqua button on macOS (via `masonry_winit`, allowed here only).
      *Accept:* builds on macOS and Linux; the `masonry_testing` golden of the button
      matches the neue-paint golden; patch and fork cost in
      `docs/research/masonry-spike.md`.
      *Done:* fork `sophie-maney/xilem` (ADR 0030), `neue_paint::imaging`, the button goldens
      match on macOS, Linux aarch64 and Linux x86_64.
- [x] **2.2 Masonry spike B: sctk driver** · Opus. Minimal sctk driver for `RenderRoot`:
      one xdg-toplevel and one layer-shell surface, a wgpu surface, fractional scale,
      text-input-v3 into a `TextInput`, and one Masonry layer realised as an xdg-popup.
      *Accept:* works under neue-comp in the VM (checked with `neuectl screenshot`);
      go/no-go recorded as an ADR 0002 amendment.
      *Done:* **go**. New crate `neue-platform` with `wayland` (Linux-only module) and the
      `sctk_spike` and `ime_probe` examples, checked in the VM (`docs/checks/phase-2.md` §1).
      Findings for 2.5/2.6 and Phase 3 are in `docs/research/masonry-spike.md`: neue-comp
      sends no scale to layer surfaces, and it has no virtual-keyboard, so fcitx5 can't bind.
- [x] **2.3 `neue-theme` crate** · Opus. New portable crate: semantic colours, metrics,
      radii, type scale and durations; light and dark; accent colour (blue, graphite);
      increased contrast; spring presets and the reduced-motion flag. neue-paint materials
      take their values from it.
      *Accept:* no colour or metric literals left in materials; existing goldens unchanged
      bit-for-bit; neue-comp still builds.
      *Done:* crate `neue-theme` (ADR 0031): `Variant` (appearance × accent × contrast)
      selects colours; metrics, radii, type scale and motion are constants; a materials
      table for the Aqua button, linen and title bar. Existing goldens re-blessed with no
      byte change; 24 new goldens for graphite and increased contrast.
- [ ] **2.4 `neue-platform` winit driver** · Opus. Our own `RenderRoot` driver replacing
      `masonry_winit`: wgpu surface, scale, pointer and keyboard, IME, cursor, clipboard,
      animation frames, multiple windows, AccessKit (`accesskit_winit`).
      *Accept:* a text-field demo runs on macOS, IME works, VoiceOver reads the tree.
      *Status:* implemented. Owner checks (`docs/checks/phase-2.md` §2): steps 1, 2 and 5
      (demo, typing/clipboard/layers, several windows) passed; 3 (IME) and 4 (VoiceOver)
      pending. `neue_platform::winit` (ADR 0032) is **macOS only**:
      on Linux, `ui-events-winit` would pull in X11 crates. Shared `neue_platform::gpu`
      renderer and `Options`; `App`/`Ctx` API with run-time windows; `winit_demo` example.
- [ ] **2.5 `neue-platform` Wayland driver: core** · Opus. Harden the 2.2 driver:
      xdg-toplevel, layer-shell, fractional scale + viewporter, frame callbacks,
      cursor-shape, text-input-v3, keyboard repeat.
      Align the driver with the winit driver's `App`/`Ctx` API (ADR 0032), so the 2.4 demo
      runs on both.
      *Accept:* the 2.4 demo runs under neue-comp in the VM; a headless neue-comp test maps
      a window from it.
- [ ] **2.6 Wayland driver: popups, clipboard, a11y** · Opus. xdg-popup positioning for
      all Masonry layers, clipboard and primary selection, xdg-activation, xdg-dialog,
      AccessKit on Linux (`accesskit_unix`).
      *Accept:* in the VM, a popup flips at screen edges, copy and paste works with foot,
      and Orca or `accerciser` sees the tree.
- [ ] **2.7 `neue-kit` app model** · Opus. Our public API over Masonry: widget facade,
      Elm-style app model, background tasks returning messages. ADR.
      *Accept:* counter and async examples on macOS; app crates don't depend on
      `masonry_*` (checked by xtask).
- [ ] **2.8 Springs and animation** · Opus. Spring animation on `on_anim_frame` with the
      `neue-theme` presets, honouring reduced motion.
      *Accept:* unit tests step deterministically; reduced motion snaps to the end value.
- [ ] **2.9 Gallery, golden harness, push button (the pattern)** · Opus. `apps/gallery`
      with page list and light/dark, scale and accent switches; a golden helper in
      neue-kit (states × light/dark × scale 1 and 2), gallery added to `GOLDEN_CRATES`;
      the Aqua push button with default-pulse, focus ring, AccessKit and keyboard.
      *Accept:* the gallery runs on macOS and in the VM; button goldens pass on macOS and
      Linux.
- [ ] **2.10 UI font decision (Q2)** · Sonnet. Gallery page comparing the PLAN.md §4 candidates
      side by side; **the owner picks**; bundle the font (OFL) and write an ADR.
      *Accept:* the theme's type scale uses it; goldens re-blessed after diff review.
- [ ] **2.11 Materials for Phases 2–3** · Sonnet. Dark HUD, translucent menu bar, glass
      dock shelf, in light and dark, plus a Materials gallery page.
      *Accept:* goldens per material; CPU and GPU renders match within tolerance.
- [ ] **2.12 Buttons batch** · Sonnet. Bevel and recessed buttons, checkbox, radio,
      segmented control.
- [ ] **2.13 Value controls** · Sonnet. Slider, stepper, progress bar with barber pole,
      spinner.
- [ ] **2.14 Text fields** · Opus. Text, search and secure fields.
      *Accept also:* IME composition works on macOS and in the VM.
- [ ] **2.15 Scroll view** · Opus. Overlay scrollbars and virtual scrolling (the base for
      tables and collection views).
- [ ] **2.16 Text view** · Sonnet. Multi-line text in the scroll view.
- [ ] **2.17 Menus** · Opus. Menu model, context menus, pop-up and pull-down buttons,
      combo box; keyboard navigation and type-select.
- [ ] **2.18 DBusMenu export** · Opus. New `neue-dbus` crate (Linux-only): export the 2.17
      model as `com.canonical.dbusmenu` and set the KDE appmenu address (ADR 0005).
      *Accept:* a zbus test client reads the layout and fires an action; `gdbus
      introspect` works in the VM.
- [ ] **2.19 Table, outline view, source-list sidebar** · Opus.
- [ ] **2.20 Icon and collection view** · Sonnet.
- [ ] **2.21 Split view and tab view** · Sonnet.
- [ ] **2.22 Window chrome and unified toolbar** · Opus. ADR: client-side title bars
      (reusing neue-paint's) vs. extending the server-side ones (ADR 0013).
- [ ] **2.23 Sheet, popover, HUD panel** · Opus. Sheets map to xdg-dialog; popovers are
      xdg-popups with an arrow.
- [ ] **2.24 Colour well, date picker, path control** · Sonnet.
- [ ] **2.25 `docs/design/` specs** · Sonnet. Token reference, materials spec, motion
      spec.
- [ ] **2.26 Exit check** · Sonnet, plus owner VM steps. Screenshot every gallery page
      with `neuectl` in the VM, compare with macOS and the goldens, fix drift, tick
      Phase 2.
- [ ] **2.27 neue-comp: scale for layer surfaces** · Sonnet. Found in 2.2:
      `State::refresh_surface_scales` (`compositor/neue-comp/src/outputs.rs`) sends
      `wp_fractional_scale_v1.preferred_scale` and `wl_surface.preferred_buffer_scale`
      to windows only, so layer surfaces stay at scale 1 and are upscaled (blurry) on a
      1.5× output. Also walk every output's `layer_map_for_output(output).layers()` and
      call `send_scale`'s body for each `LayerSurface` (with `with_surfaces`, so its
      popups and subsurfaces get it too) using that output's scale. Keep "only changes are
      sent". Add a layer-surface variant of the test client's preferred-scale tracking
      (`tests/harness/client.rs`, now toplevel-only).
      *Accept:* a headless test in `tests/outputs.rs` or `tests/shell_protocols.rs`: a
      layer surface on a 1.5 output gets `preferred_scale` 180, and 240 after a config
      reload to 2; in the VM, `docs/checks/phase-2.md` §1 step 7 shows the layer-shell strip
      sharp. Order: any time, but before 2.5's VM check.
- **Exit:** the gallery shows the whole widget set, in light and dark, identically on
  macOS and in the VM.
