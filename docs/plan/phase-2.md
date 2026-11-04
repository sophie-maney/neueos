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
      `sctk_spike` (replaced by `demo` in 2.5) and `ime_probe` examples, checked in the VM (`docs/checks/phase-2.md` §1).
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
      renderer and `Options`; `App`/`Ctx` API with run-time windows; `winit_demo` example (renamed `demo` in 2.5).
- [x] **2.5 `neue-platform` Wayland driver: core** · Opus. Harden the 2.2 driver:
      xdg-toplevel, layer-shell, fractional scale + viewporter, frame callbacks,
      cursor-shape, text-input-v3, keyboard repeat.
      Align the driver with the winit driver's `App`/`Ctx` API (ADR 0032), so the 2.4 demo
      runs on both.
      *Accept:* the 2.4 demo runs under neue-comp in the VM; a headless neue-comp test maps
      a window from it.
      *Done:* ADR 0033. `App`/`Ctx`/`WindowSpec` (with `WindowKind::Layer`) at the crate
      root, `neue_platform::run` per host; `wayland::run_with_connection`; per-surface
      frame-callback pacing; cursor shapes with xcursor fallback; key repeat and winit's
      key tables; click counting. `examples/demo.rs` replaces `winit_demo` and
      `sctk_spike`; `neue-comp/tests/platform.rs` maps and draws a driver window. VM check
      `docs/checks/phase-2.md` §3 passed: steps 1–4 run by the agent, 5–6 (cursor shapes,
      scale 1.5) by the owner.
- [x] **2.6 Wayland driver: popups, clipboard, a11y** · Opus. xdg-popup positioning for
      all Masonry layers, clipboard and primary selection, xdg-activation, xdg-dialog,
      AccessKit on Linux (`accesskit_unix`).
      *Accept:* in the VM, a popup flips at screen edges, copy and paste works with foot,
      and Orca or `accerciser` sees the tree.
      *Done:* ADR 0034. Per-kind placement (menus anchor to the widget they open under,
      tooltips don't grab), nested menus, `xdg_popup.reposition`; clipboard both ways;
      primary selection **paste only** (Masonry reports no selections: 2.14);
      `TakeFocus` and the start-up token through xdg-activation; `WindowSpec::parent` and
      `modal`; one `accesskit_unix` adapter per window. Also fixes neue-comp, which never
      unconstrained layer-shell popups. VM check `docs/checks/phase-2.md` §4, run by the
      agent: an AT-SPI client from Python stands in for Orca and accerciser (neither is
      installed).
- [x] **2.7 `neue-kit` app model** · Opus. Our public API over Masonry: widget facade,
      Elm-style app model, background tasks returning messages. ADR.
      *Accept:* counter and async examples on macOS; app crates don't depend on
      `masonry_*` (checked by xtask).
      *Done:* ADR 0035. New crate `neue-kit`: `Application` (model, `Message`, `update`)
      over retained widgets changed through `Handle`s; builders `Label`, `Button`,
      `Stack` (interim looks; 2.9 makes the real push button); `Cx::spawn`,
      `spawn_future`, `sender` and cancellable `Task`s. neue-platform gains `Ctx::waker`
      and `App::on_wake`. Examples `counter` and `async_tasks` checked on macOS; headless
      app-model tests on a fake host. `cargo xtask check` fails if a crate under `apps/`
      or `shell/` depends on `masonry*`. No goldens yet: 2.9 builds the golden helper.
- [x] **2.8 Springs and animation** · Opus. Spring animation on `on_anim_frame` with the
      `neue-theme` presets, honouring reduced motion.
      *Accept:* unit tests step deterministically; reduced motion snaps to the end value.
- [x] **2.9 Gallery, golden harness, push button (the pattern)** · Opus. `apps/gallery`
      with page list and light/dark, scale and accent switches; a golden helper in
      neue-kit (states × light/dark × scale 1 and 2), gallery added to `GOLDEN_CRATES`;
      the Aqua push button with default-pulse, focus ring, AccessKit and keyboard.
      *Accept:* the gallery runs on macOS and in the VM; button goldens pass on macOS and
      Linux.
      *Done:* ADR 0037. `apps/gallery` (`neue-gallery`, `cargo xtask gallery`): page list,
      switches for appearance, accent, contrast, reduced motion and zoom (zoom multiplies
      the display scale; a real scale override would need a platform API). Golden
      comparison moved to `neue_paint::golden`; `neue_kit::testing` (feature) renders a
      `Fixture` in states and `check_states` covers states × light/dark × 1x/2x; 52
      push-button goldens. Push button: neutral gel, accent when default or held, pulse,
      focus ring, disabled; Space presses the focused button, Return the default button
      via a window root that is the focus fallback (new `WindowSpec::focus_fallback`).
      Also `Cx::set_theme` and `Zoom`. Goldens pass on macOS, Linux aarch64 and x86_64;
      VM check `docs/checks/phase-2.md` §5 run by the agent.
- [x] **2.10 UI font decision (Q2)** · Sonnet. Gallery page comparing the PLAN.md §4 candidates
      side by side; **the owner picks**; bundle the font (OFL) and write an ADR.
      *Accept:* the theme's type scale uses it; goldens re-blessed after diff review.
      *Done:* ADR 0038. The owner compared the four §4 candidates plus DejaVu Sans on a
      temporary Fonts page and picked **DejaVu Sans** (Bitstream Vera licence, not OFL).
      `UI_FAMILY` and the bundled font in `neue-paint` changed; the 88 text-bearing goldens
      (gallery buttons, title bars, button materials) were re-blessed after spot-checking.
      The comparison page and its multi-font plumbing were removed again (commits
      b57c90c, edf9bc5 in the branch history); `NEUE_GALLERY_PAGE=<n>` stays for screenshots.
      Regular weight only, so no bold yet.
- [x] **2.11 Materials for Phases 2–3** · Sonnet. Dark HUD, translucent menu bar, glass
      dock shelf, in light and dark, plus a Materials gallery page.
      *Accept:* goldens per material; CPU and GPU renders match within tolerance.
      *Done:* ADR 0039. `neue_theme::materials::{menu_bar, dock_shelf, hud}` palettes with
      alpha and `Backdrop::{Opaque, Translucent}` (opaque is the derived Phase 2 fallback;
      increased contrast forces it); drawing in `neue_paint::materials` (dock shelf in
      perspective and flat). 48 neue-paint goldens plus 16 gallery goldens of the new
      `neue_kit::Specimen`; `neue-paint/tests/gpu.rs` compares CPU and GPU renders (max
      channel difference 2 on Apple GPUs; skips without an adapter). Gallery page
      "Materials" (window now 860×560). Goldens pass on macOS, Linux aarch64 and x86_64.
- [x] **2.12 Buttons batch** · Sonnet. Bevel and recessed buttons, checkbox, radio,
      segmented control.
      *Done:* ADR 0040. `neue_paint::materials::{bevel_button, recessed_button, toggle,
      segmented}` (gel reused from the push button), neue-kit `BevelButton` (bevel and
      recessed, momentary or toggle), `Checkbox` (with mixed), `Radio`, `Segmented`; the
      first springs in use (check mark fade, sliding highlight). Gallery page "Buttons";
      168 goldens (42 states × light/dark × 1x/2x). Radio groups are the app's job for now
      (no arrow keys between radio buttons); segments are not separate AccessKit nodes.
- [x] **2.13 Value controls** · Sonnet. Slider, stepper, progress bar with barber pole,
      spinner.
      *Done:* ADR 0041. `neue_paint::materials::{slider, stepper, progress_bar, spinner}`
      over a shared groove, with palettes in `neue_theme::materials`; neue-kit `Slider`,
      `Stepper`, `ProgressBar` (determinate on a spring, indeterminate barber pole) and
      `Spinner`; `Ticker` for ambient animation that stops under reduced motion. Gallery
      page "Values" (the slider drives a bar and the stepper a label); 88 goldens, passing on
      macOS and Linux aarch64 (x86_64 not run). New
      widget types must be added to `theme::default_properties` (ADR 0041 §8). Not yet:
      slider ticks, vertical slider, stepper hold-to-repeat.
- [x] **2.14 Text fields** · Opus. Text, search and secure fields.
      *Accept also:* IME composition works on macOS and in the VM.
      *Done:* ADR 0042. `neue_kit::TextField` (`new`, `search`, `secure`) on parley's
      `PlainEditor`: one line that scrolls to the caret, `on_change` and `on_submit`
      (without a submit handler Return reaches the default button), Tab selects all,
      Escape and the cancel button clear a search field, blinking caret (steady under
      reduced motion). Secure fields hold bullets in the editor, so the real text never
      reaches layout or AccessKit; no copy, no IME. Well in
      `neue_paint::materials::text_field`; new tokens `text_selection`,
      `TEXT_FIELD_WIDTH`, caret blink. `Fixture::{type_text, select_text, compose}`;
      gallery page "Fields"; 68 goldens, passing on macOS and Linux aarch64 (x86_64 not
      run). IME: VM check `docs/checks/phase-2.md` §6 (input-method probe); macOS IME
      step for the owner in the same section. Not yet: undo, primary selection (2.28),
      context menu (2.17).
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
      reload to 2; in the VM, `docs/checks/phase-2.md` §3 step 6 shows the demo's
      layer-shell strip sharp. Order: any time, but before 2.5's VM check.
- [ ] **2.28 Primary selection from text fields** · Sonnet. Found in 2.14: the Wayland
      driver pastes the primary selection but never owns it. Add a platform call to set
      the primary selection (`Ctx`, no-op on winit), a runner hook, and have
      `TextField` report its selected text when a pointer or Shift selection ends (not in
      secure fields). ADR amendment to 0034.
      *Accept:* in the VM, selecting text in the gallery's field and middle-clicking in
      foot pastes it.
- **Exit:** the gallery shows the whole widget set, in light and dark, identically on
  macOS and in the VM.
