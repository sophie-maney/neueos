# 0012: Cursors: XCursor themes, cursor-shape-v1, resize cursors (task 1.9)

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
Until now the tty backend drew one procedural arrow for every pointer state (ADR 0001,
amendment 2) and the nested backend left the host's cursor alone. Task 1.9 makes the
compositor draw the cursor a client or an interaction asks for, in the user's theme, at the
right size for each output.

## Decision
1. **Dependency: `xcursor` 0.3** (pure Rust, MIT, no C). It only finds and parses theme
   files (`CursorTheme::load`, `parse_xcursor`). It is already in the lockfile through
   winit, and is optional in `neue-comp`, enabled by the `tty` feature. Rendering,
   caching and animation are ours (`src/cursor.rs`).
2. **Theme and size** come from `XCURSOR_THEME` (default `default`) and `XCURSOR_SIZE`
   (default 24, clamped to 8..=256), the variables clients already read, so the compositor
   and its apps agree. A `[pointer]` setting in `compositor.toml` (with hot reload) is a
   follow-up; the settings pane (Phase 5) will own it.
3. **Per-scale sizes.** The image is chosen at `XCURSOR_SIZE × output scale` (rounded) by
   nearest nominal size in the theme file, and drawn 1:1 in physical pixels (a buffer of
   scale 1 with a logical size of `pixels / scale`). A scale-2 output uses 48 px images
   and stays sharp; fractional scales get the nearest size, not a resampled one.
4. **Lookup and fallback.** A CSS name (`CursorIcon::name`) is tried first, then the
   legacy aliases (`alt_names`, e.g. `left_ptr`, `xterm`). If the theme has none, the
   built-in arrow is drawn at the needed size (the arrow is now rasterised at any size
   instead of being fixed at 24 px). Animated cursors step by session time; the tty
   render loop already polls once per refresh, so no timer is needed.
5. **`cursor-shape-v1`** (version 2) from Smithay. Smithay's cursor-shape dispatch needs
   the `TabletSeatHandler` trait on the state; we implement it with defaults and enable
   no tablet protocol.
6. **Compositor-chosen cursors win.** During an interactive resize the cursor is the
   edge's resize shape (`n/e/s/w/ne/nw/se/sw-resize`). Smithay lets the grab's client
   keep setting its cursor during the grab, so `State::cursor_override` makes
   `SeatHandler::cursor_image` ignore client requests (and Smithay's own reset to the
   arrow on focus loss) until the grab ends. Move grabs keep the arrow.
7. **Nested (winit) backend** passes the shape to the host window (`Window::set_cursor`,
   `set_cursor_visible`) so it uses the host's theme. Client-drawn cursor surfaces are not
   rendered when nested and show the host's arrow, as before.
8. **Hardware cursor plane** was in place since 1.4 (`Kind::Cursor` elements with
   `ALLOW_CURSOR_PLANE_SCANOUT`). Themed images keep using it as long as they fit the
   plane (commonly 64×64); a bigger `XCURSOR_SIZE × scale` is composited instead, which
   works but costs a full-frame update on each pointer move.

## Consequences
- Text fields, links and resize edges show the right cursor for clients that use
  cursor-shape-v1 (GTK 4, Qt 6, Chromium, recent toolkits). Others still send their own
  surface, which is drawn as before.
- Cursor artwork depends on an installed XCursor theme; the distro must ship one
  (original artwork, no Apple assets) with the Phase 3 theme work. Until then the VM uses
  whatever theme is installed, or the built-in arrow.
- Server-side title bars and edges (1.10, 1.11) can pick resize shapes through
  `set_cursor_override`.

## Alternatives considered
- **Load themes with `wayland-cursor`:** a C library binding with no use on the DRM path.
- **Resample one size for every scale:** blurry; themes ship several sizes for this.
- **Let the client's cursor win during a resize:** the cursor would flicker back to the
  client's idea of the edge, and server-side edges have no client at all.
