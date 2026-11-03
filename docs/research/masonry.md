# Research: Masonry as the base of neue-kit (2026-11-03)

## Recommendation: adopt `masonry_core` with a fork-ready posture

Depend on `masonry_core` (plus `masonry_testing`) from a pinned git rev. Write our own
widgets, theme and drivers on top. Do not use `masonry_winit`, Xilem, or Masonry's default
look. If the 0.6 proof of concept fails, fall back to an own thin tree, reusing the same
crates (parley, accesskit, kurbo, peniko). Write that outcome up as an ADR. Note that this
changes PLAN.md §3.2: Masonry has its own layout and not taffy.

## Facts (verified 2026-11-03)
- crates.io `masonry` is 0.4.0 (2025-10-29). Releases: 0.2 May 2024, 0.3 May 2025, 0.4 Oct
  2025. There has been no release in 12 months, but `main` is active: 302 commits since
  v0.4.0, last push 2026-10-29, 161 open issues. Every release is 0.x with breaking changes.
- Licence is Apache-2.0, which is compatible with GPL-3.0-or-later. `imaging` is
  Apache-2.0 OR MIT.
- Crate split: `masonry_core` (widget tree, passes, `RenderRoot`; no windowing),
  `masonry` (stock widgets and theme), `masonry_winit` (driver), `masonry_testing`,
  `masonry_imaging`. Xilem sits on top and is optional. Masonry does not depend on Xilem.
- `main` has moved off direct vello `Scene` painting onto the new `imaging` crate
  (forest-rs, 0.0.1, May 2026). Pinned deps on `main` lag crates.io: vello 0.8 (latest
  0.11), parley 0.8 (latest 0.12), accesskit 0.24 (latest 0.25). Expect upgrade friction.

## Against the criteria
**(a) Paint control: good.** Widgets get `pre_paint`, `paint` and `post_paint` with an
`imaging::Painter`. The IR supports clips, isolated groups (filters, mask, blend, opacity),
gradients and image brushes via peniko, glyph runs and `BlurredRoundedRect`. Group filters
are Blur, DropShadow, Offset and Flood. Procedural materials (linen, brushed metal, gel) are
plain paint code. Two gaps:
- There is no *backdrop* blur. `Filter::Blur` blurs the group's own content only. Real
  backdrop blur needs compositor cooperation (`ext-background-effect-v1`, or neue-comp's own
  blur), or a snapshot of lower layers. `VisualLayerKind::External` is a placeholder for
  externally realised content, which fits this.
- Custom wgpu textures in widgets (#1319) are not supported. Everything goes through the IR.

**(b) Own Wayland backend: feasible.** `RenderRoot` exposes `handle_window_event`,
`handle_pointer_event`, `handle_text_event`, `handle_access_event` and `redraw()`. `redraw()`
returns a `VisualLayerPlan` plus an AccessKit `TreeUpdate`. It emits driver signals:
`StartIme`, `ImeMoved`, `NewLayer`, `RepositionLayer`, `SetCursor`, `ClipboardStore`,
`RequestAnimFrame`, `DragWindow` and more. An Android driver already exists outside the
repo, which shows that alternative drivers are an intended use. We would write the sctk
driver: layer-shell, xdg-dialog, text-input-v3 mapped to `Ime::Preedit` and `Ime::Commit`,
fractional scale (`RenderRoot::scale_factor` exists), and a wgpu surface per window. Popups
and menus are Masonry "layers" inside one window. Mapping them to xdg-popup surfaces needs
driver work, because `NewLayer` currently carries only coarse types (tooltip, selector,
other). Multi-window is supported, with one `RenderRoot` per window. On Wayland, upstream
`masonry_winit` has no layer-shell, so shell surfaces need our driver anyway.

**(c) Churn: the main risk.** The paint backend, property system, layout API
(`measure` and `layout` with `Length`) and `VisualLayerPlan` are all in flux. Mitigations:
pin a rev, vendor if needed, and keep our API behind neue-kit traits.

**(d) Work saved: large.** Masonry already provides:
- a widget arena, with pass scheduling, hit testing, focus and tab order, and pointer capture;
- parley-based `TextArea` / `TextInput` / `Prose` with selection, IME and clipboard hooks;
- AccessKit tree generation wired through every widget (`accessibility_role` and
  `accessibility`);
- a test harness with `vello_cpu` screenshots (`masonry_testing`, via `imaging_vello_cpu`);
- scrolling (Portal), virtual scroll, flex, grid and split widgets.

**Layout.** It is Masonry's own CSS-ish measure/layout protocol, not taffy. Custom layout
is easy (implement `measure` and `layout`). To use taffy we would wrap it inside a widget.

**Animation.** Only `on_anim_frame` and `RequestAnimFrame`. There are no springs or
transitions, and no reduced-motion setting. `neue-theme` springs have to be built by us
either way.

## Known pain points
- High CPU while a text box is focused on Linux (#1562).
- Wayland clipboard without XWayland (#1419), scale-factor handling (#429), and blur and
  transparency only in prototype stage (#1835).
- Text widgets under-tested (#1341). Scroll/Portal and Properties are still tracking issues
  (#1344, #1342). "Missing widgets" (#1109): no menus, tables or outline view, so most of
  our v1 widget set is ours to write.

## Own thin tree: rough cost
About 8-12k lines and 6-10 weeks of work:
- arena and passes (about 3k);
- focus, hit testing and pointer capture;
- text editing with IME, selection and undo on parley (the biggest item, 2-3 weeks);
- AccessKit tree plumbing;
- test harness;
- scroll and virtualisation.

A bug-for-bug-compatible IME is the real cost. This is the "same crates" fallback.

## What the 0.6 neue-paint PoC must test
1. A widget painting a linen texture, an Aqua gel button (gradients, inner shadow, specular
   group with a mask) and brushed metal. Check that output is sharp at scale 1 and 2.
2. Golden tests via `masonry_testing` with `imaging_vello_cpu`: determinism, and how closely
   the CPU and GPU renders match.
3. A minimal sctk driver driving `RenderRoot`: one layer-shell surface and one xdg-toplevel,
   with a wgpu surface, fractional scale and text-input-v3 into a `TextInput`.
4. Popup realisation as xdg-popup, plus a backdrop-blur strategy through `External` layers
   or `ext-background-effect-v1`.
5. Cost of bumping to vello 0.11 and parley 0.12, to gauge how far we are from upstream.
6. Idle CPU and frame time of a menu bar plus dock on aarch64.

## Open questions
- Does Linebender plan a 0.5 release soon, and what is the `imaging` API stability policy
  (Zulip #masonry)?
- Will upstream accept a generic Wayland/sctk driver, or `NewLayer` extensions for
  popups?
- Is a `masonry_core`-only dependency supported, with the default theme and properties
  skipped?
- Plan §3.2 names taffy: drop it, or keep it as an optional wrapper widget?

## Sources
- https://crates.io/crates/masonry and https://github.com/linebender/xilem (ARCHITECTURE.md,
  `masonry_core/src/core/widget.rs`, `masonry_core/src/app/render_root.rs`,
  `masonry_core/src/app/visual_layers.rs`, `masonry_testing`)
- https://github.com/forest-rs/imaging (README, `filter.rs`, `paint.rs`)
- Issues: https://github.com/linebender/xilem/issues/1835, /1319, /1562, /1419, /429, /1341
- https://linebender.org/blog/ (no 2026 posts on Masonry found. "Linebender in 2026 Q1"
  was not read.)
- https://xi.zulipchat.com/#narrow/stream/317477-masonry (not read, no access)
