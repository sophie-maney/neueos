# 0092: The dock: shelf reflections, a `Dock` widget with focus-only tile children, a model of pinned and running apps, minimise targets

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.16 (Dock A): the glass shelf with reflections, the dock as a layer surface at the
bottom, pinned apps from config, launching, running indicators, click to activate (and
cycle windows), launch and attention bounces (springs, reduced motion), and minimise
targets for the compositor. The shelf material existed since Phase 2 without reflections.
`neue_shell_v1` already had `set_minimize_rectangle` (ADR 0066 §10), the compositor
already turned `xdg-system-bell` into attention, and the shell already got its activation
tokens granted (ADR 0066 addendum).

## Decision
1. **Reflections in `neue-paint`.** `dock_shelf_with(…, reflections)` draws what the closure
   draws between the glass fill and the sheen, clipped to the shelf and at the palette's
   `reflection` opacity (light 0.30, dark 0.40, none under increased contrast).
   `reflected(scene, base, f)` mirrors about the icons' base line, squashed to
   `REFLECTION_SCALE` (0.5), so a bouncing icon's reflection moves away from it. Also
   `running_indicator` (a core and a blurred glow, light and dark) and `placeholder_icon`
   for apps without one. Tokens in `neue_theme::materials::{dock_shelf, dock}`.
2. **Raster icons follow the scene transform.** Texture brushes are placed in scene space,
   so a mirrored raster icon sampled its texture unmirrored (a flat block).
   `icon::draw_raster` now places the texture with `scene.transform()`. Nothing else
   draws raster icons under a transform; no other golden changed.
3. **`neue_kit::desktop::Dock`.** One widget paints the shelf, reflections, indicators and
   icons in one scene and handles the pointer (press on a tile, release on the same tile is
   a click). Each tile is also a child `DockTileWidget` that takes no pointer and only
   holds keyboard focus (with the focus ring) and the tile's AccessKit node (Button, the
   app's name, "Running"/"Opening"/"Wants attention", busy while launching). Key presses and
   AccessKit clicks bubble from the child to the dock, which knows the tile by
   `ctx.target()`. Clicks reach the app as the tile's key (`Dock::on_click`); tiles change
   with `Handle::set_tiles`, matched by key so a bounce and focus survive updates.
4. **The layout is a pure function of the tile count** (`dock_size`, `dock_icon_rect`):
   48 px icons 4 px apart, 22 px padding at the ends (more than the shelf's 18 px
   perspective inset), the icons' bottoms 6 px into the 24 px shelf, and 24 px of bounce
   room above them. The shell computes minimise rectangles from it without asking the
   widget.
5. **Bounce.** A rise on `RISE` (response 0.5, critically damped) that turns at 90 % of
   22 px, and a fall on `Spring::BOUNCY` that ends when the icon lands, like a ball on a
   floor: about 0.5 s a bounce. Launch bounces back to back; attention rests 500 ms
   between bounces. A bounce under way finishes when it is stopped. Under reduced motion
   nothing moves: a launching icon stays put and an icon asking for attention stands 8 px
   raised until it is answered.
6. **The surface.** A top-layer surface anchored to the bottom edge only (centred),
   `fit_content` so it follows the tile count, an exclusive zone of the icons and shelf
   (maximised windows stop above the shelf), no keyboard. The new
   `LayerSpec::pass_through_top` leaves the bounce room out of the input region, so the
   pointer reaches windows under it. On the first output; it moves to another when that
   one goes away.
7. **The model (`neue_shell::dock`, portable).** Tiles are the pinned apps
   (`shell.toml` `[dock] pinned`: desktop file IDs or app ids, default `["foot"]`;
   uninstalled ones left out), then other apps with windows in the order their first
   window opened. A window belongs to the entry `find_by_app_id` finds, else to its app id,
   else it is a tile of its own. A click on an app without windows launches it; on a
   running app it activates the window of it that was active last (else the newest); if
   one of its windows is active already, the next one, so clicks cycle.
8. **Launching** (`dock_host`): the tile bounces at once, the shell asks for an
   activation token on the dock's surface and launches the entry with it
   (`XDG_ACTIVATION_TOKEN`), so the first window comes up focused. The bounce stops when
   the app has a window or after `LAUNCH_TIMEOUT` (10 s), or at once if the launch fails.
9. **`ShellRequest::SetMinimizeRectangle { toplevel, surface, rect }`** in
   `neue-platform`: the Wayland driver resolves the app's `WindowId` to its `wl_surface`.
   The shell sends each window's icon rect when it changes (tiles come and go).
10. **Icons** come from the desktop entry's `Icon` through the theme (`IconLoader`, 48 px
    at scale 2) on a spawned thread, once per name; apps without an entry or icon get the
    placeholder.

## Alternatives considered
- **Tiles as full child widgets that paint their icons:** the reflections and indicators
  must be drawn inside the shelf, under its sheen, which the parent paints before the
  children; one scene in the parent keeps the layering right.
- **Clicks as child actions:** tiles created later would need their handlers registered
  through the window (as notification panels do); bubbling to the parent needs none.
- **Asking the widget where the tiles are** for minimise targets: an extra round trip
  through actions; the fixed layout makes it unnecessary.
- **A surface as tall as the bounce with the whole rect as input:** it would swallow clicks
  on windows under the bounce room.
- **Blur under the shelf:** the blur region covers the whole surface, bounce room
  included; the shelf draws the opaque tint for now.

## Consequences
- `neue_platform::LayerSpec` gains `pass_through_top`; `ShellRequest` gains a variant.
- The compositor logs each minimise rectangle at debug level (to check it in the VM).
- Phase 4's genie has targets to animate to.

## Not done
- Magnification, the left and right edges, rearranging, removing, Keep in Dock, context
  menus, the Trash tile and minimised-window tiles: task 3.17.
- The app's own window icon (`xdg-toplevel-icon`) for apps without an entry: the platform
  doesn't pass icons on yet.
- Focusing the dock from the keyboard (Ctrl+F3): the surface takes no keyboard; the tiles
  are Tab-operable once it does.
- Name labels over hovered icons.
- Following `[dock] pinned` while the shell runs (3.17 edits it).
