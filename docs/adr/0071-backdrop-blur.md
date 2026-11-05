# 0071: Backdrop blur in the compositor: `ext-background-effect-v1` and dual Kawase

- **Status:** Accepted
- **Date:** 2026-11-05
- **Settles:** 0002 §7 (the blur protocol, deferred to Phase 3)

## Context
Task 3.4. Menus, popovers, HUDs, sheets and the menu bar are translucent over a blurred
backdrop (PLAN.md §4). ADR 0002 §7 makes that the compositor's job: clients mark regions,
`neue-comp` blurs behind them. The protocol was left open between the staging
`ext-background-effect-v1` and a private one.

## Decision
1. **Protocol: `ext-background-effect-v1`** (wayland-protocols staging). Our pinned
   `wayland-protocols` 0.32.13 has it, and the Smithay fork has the server side
   (`smithay::wayland::background_effect`: the global, the double-buffered blur region in
   the surface's cached state). No private protocol. The global announces `blur` to every
   client; a blur region is a `wl_region` in surface coordinates, applied on commit.
2. **A framebuffer-effect render element.** `blur::BlurElement` goes right below the
   render element of every surface with a blur region (toplevels, subsurfaces, popups,
   layer surfaces), in `render::output_elements`, so the tty, winit, headless and
   screencopy paths all get it. It uses the Smithay fork's framebuffer-effect support in
   `OutputDamageTracker`: when damage behind the element overlaps it, the tracker damages
   the whole element (lifting opaque regions above it), draws what is below, and calls
   `capture_framebuffer`, which copies that area out of the framebuffer
   (`glBlitFramebuffer`) and blurs it into a texture kept in the tracker's per-element
   cache. `draw` paints that texture inside the region only, with blending off.
3. **Damage widened by the reach.** The element covers the region's bounds grown by the
   blur's reach and clipped to the output, so any change within reach of the region
   re-blurs it, and the copy never contains stale pixels. Cheap over-approximation: one
   change behind re-blurs the whole region.
4. **Cached while nothing behind changes.** Damage that only touches the surface itself
   or anything above it (the client's own redraws, the cursor) reuses the cached texture.
   An idle desktop with blurred surfaces draws nothing (ADR 0019); measured in the VM: no
   CPU time over 10 s.
5. **Dual Kawase** (Bjørge, SIGGRAPH 2015): 3 down and 3 up passes at scale ≤ 1.5, 4 at
   higher scales, sample offset 2; the blur reaches 32 logical pixels either way. The
   passes are Smithay custom texture shaders (`compile_custom_texture_shader`) rendered
   into `GlesTexture` levels through the frame's renderer guard (`FrameContext`), so the
   only raw GL is the framebuffer copy and setting the levels to clamp at their edges.
   Blur is done in framebuffer orientation and drawn back with the output's transform, so
   rotated outputs need nothing special. Strength is a constant in `blur.rs`, not a
   setting yet.
6. **Off where it cannot help.** No blur for a full-screen window's surfaces (nothing
   shows through; the primary plane stays eligible for direct scanout, ADR 0020; Smithay
   never scans out an effect element anyway). No blur on a locked session. A renderer
   without GLES 3 blits (`Capability::Blit`) skips blur with one warning.
7. **`blur-demo` example** (`cargo run -p neue-comp --example blur-demo`): a see-through
   window asking for blur, for checking the compositor before first-party clients ask for
   blur (task 3.5) and for the owner's checks on the PC.

## Consequences
- Clients get blur by setting a region. Task 3.5 makes neue-platform do that for
  translucent materials and switches them from the opaque fallback (ADR 0039).
- A region is rectangles: rounded menus approximate their corners with a few rectangles
  or keep the corners opaque. Corner masks would need an extension or a private request.
- Every change behind a blurred surface costs one redraw of the grown area plus about 2×
  its pixels in blur passes. Dragging a window under the menu bar re-blurs the bar's strip
  every frame; checked for smoothness on real hardware by the owner (task 3.4 *Owner:*).
- Pixels under an opaque window that sits above a blurred surface are not redrawn, so
  they read as that window near its edge (a halo of its colour, at most the reach wide).
  Smithay's tracker lifts opaque regions only above the effect element, which covers the
  common cases (the blurred surface itself, windows overlapping it).
- `capture_framebuffer` creates a temporary FBO per capture and `pass` binds each level
  as a target (Smithay creates and drops an FBO each time). Fine at menu sizes; reuse
  them if profiling asks.

## Alternatives considered
- **A private protocol** (ADR 0002 §7's fallback): unnecessary once the staging protocol
  and its Smithay support were available; other toolkits (Qt, GTK) are adopting the
  staging one.
- **Our own change detection** (fingerprints of the elements behind, an epoch counter):
  considered before finding the tracker's framebuffer-effect support; it duplicates what
  the tracker knows exactly and is easy to get wrong with screencopy's separate trackers.
- **Gaussian blur:** more passes and taps for the same width; dual Kawase is the usual
  choice in compositors (KWin, Hyprland, picom) for its quality per sample.

## Revisit when
- The protocol leaves staging or gains a version with corner radii or blur strength.
- Profiling on the PC shows the re-blur cost while dragging (then: per-region blur
  caching by element damage, or fewer passes at a lower resolution).
