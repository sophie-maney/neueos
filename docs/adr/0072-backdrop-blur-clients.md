# 0072: Backdrop blur in clients: windows ask, materials switch to the translucent tint

- **Status:** Accepted
- **Date:** 2026-11-05
- **Builds on:** 0039 (translucent materials), 0071 (compositor blur)

## Context
Task 3.5. The compositor blurs behind any surface with a blur region (ADR 0071). The
materials already have a translucent tint and an opaque fallback (ADR 0039); nothing
connected the two. Only the menu bar, dock shelf and HUD panel have a translucent
variant today. Menus, popovers and sheets are opaque materials.

## Decision
1. **The Wayland driver binds `ext_background_effect_manager_v1`** and reads its
   `capabilities` event into a flag (`Ctx::has_blur`). Apps ask in `on_start`, so the
   driver does one roundtrip after binding when the global exists. Without the global, or
   without the blur capability, nothing changes. The winit driver never offers blur.
2. **A window asks with `WindowSpec::blur: Option<Blur>`**, ignored without
   `has_blur`. The driver creates the effect object for the window's surface and sets
   the region to the whole window, resized on every configure. `Blur::corner_radius`
   cuts the corners: the region is a body plus one-pixel-high strips along the rounded
   edges, because the protocol takes rectangles only (ADR 0071 §Consequences).
3. **neue-kit decides the look.** `Window::blur_behind()` says a window's root material is
   translucent; a HUD says so by itself. `Cx::open_window` asks for the blur and paints
   `Backdrop::Translucent` only when `Cx::has_blur()` and the theme isn't increased
   contrast (ADR 0039 §2); otherwise the request is not made and the opaque tint stays.
   `Cx::backdrop()` gives the shell the same answer for the menu bar widget, so the
   request and the paint never disagree.
4. **Popups stay as they are.** Menus, popovers and sheets are opaque materials with no
   translucent variant, so they do not ask for blur. That needs new palettes and goldens
   and is a separate task (3.5b).
5. **No new goldens.** The existing goldens render with `Backdrop::Opaque`, unchanged;
   the translucent variants of the menu bar and HUD already have goldens in `neue-paint`
   (ADR 0039 §5). The kit tests compare what a window paints with and without blur.

## Consequences
- The shell's menu bar and kit HUD windows are translucent over a blurred backdrop on
  neueOS, and keep the opaque fallback on macOS and under other compositors.
- The theme can change at run time: a window opened with blur keeps its blur request when
  contrast becomes increased, and paints opaque (the palette falls back by itself); the
  compositor blurs under an opaque surface for nothing until the window is reopened.
- A compositor that drops the blur capability later stops blurring (protocol); windows
  keep painting the translucent tint until restarted. Rare; accepted.

## Alternatives considered
- **Blur regions from the toolkit's widgets** (menus, popovers): needs translucent menu
  and popover materials first, and a region per popup surface. Deferred to 3.5b.
- **Assuming blur when the global exists:** skips the roundtrip, but capabilities can be
  empty (a compositor without a blur-capable renderer).
