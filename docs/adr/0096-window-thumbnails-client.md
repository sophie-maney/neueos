# 0096: Window thumbnails: a toplevel-capture client in neue-platform

- **Status:** Accepted
- **Date:** 2026-11-06

## Context
Task 3.17d. A minimised window's dock tile (ADR 0095) should show a picture of the window.
The compositor already captures single windows through `ext-image-copy-capture-v1` with
`ext-foreign-toplevel-image-capture-source-v1` sources (ADR 0023), but the shell had no
client for it. The shell's window list is `neue_shell_v1` (ADR 0066); a toplevel source
is made from an `ext_foreign_toplevel_handle_v1`, which only `ext-foreign-toplevel-list-v1`
hands out.

## Decision
1. **Where.** The client lives in neue-platform's Wayland driver
   (`wayland/capture.rs`), next to `neue_shell_v1`, and is bound only when the compositor
   offers `neue_shell_v1` (the shell): ordinary apps don't bind the toplevel list or the
   capture globals. No new dependency: `wayland-protocols` (staging) has the client side,
   and sctk's `RawPool` gives the shm memory without our own `unsafe`.
2. **API.** `ShellRequest::Thumbnail { toplevel, max_side: Option<u32> }` starts (or
   resizes) and stops a window's thumbnails; `ShellEvent::Thumbnail(ToplevelId,
   ToplevelImage)` brings each one. `ToplevelImage` is premultiplied RGBA8 (`Arc<[u8]>`),
   already scaled down to fit `max_side` square, so the app converts it to a `Picture`
   without knowing shm formats. Without the globals, no thumbnails come and the tile keeps
   its drawing of a window.
3. **Matching windows.** The driver binds `ext_foreign_toplevel_list_v1` and keeps each
   handle by its identifier once its first `done` arrives. A `neue_shell_v1` window is
   matched by its identical identifier (`window-N`). A request that comes before the
   handle waits for it.
4. **Frame loop.** One session per watched window and one shm buffer per session, made
   again when the session's constraints (size, formats) change. Formats read, best first:
   ARGB, XRGB, ABGR, XBGR 8888. After a frame is ready the next is asked for after
   `INTERVAL` (500 ms); the compositor holds it until the window changes (ADR 0023 §5), so
   a window that doesn't draw costs nothing. A failed frame is retried after the same
   interval; `stopped` ends the watch.
5. **Scaling** is a box filter on the CPU in the driver, from the window's size to fit
   `max_side` (the shell asks for its largest tile size at scale 2). Only the scaled
   picture crosses into the app.
6. **Ending.** The watch ends when the app sends `max_side: None` (the shell does when the
   window's minimised tile goes: restored or closed), when the window leaves the shell's
   list, or when the compositor stops the session.
7. **Painting.** `dock_shelf::minimised_window` takes `Option<&Picture>`: the thumbnail is
   fitted in the tile (keeping its proportions, whole logical pixels), clipped to small
   rounded corners, over a soft shadow, with a rim; the app's icon badge stays in the
   tile's bottom right corner. No title bar is drawn on it (the capture has none, ADR 0023
   §3). Tokens `THUMBNAIL_RADIUS`, `THUMBNAIL_SHADOW_BLUR`, `THUMBNAIL_SHADOW_OFFSET` and
   the palette's `thumbnail_shadow` (light and dark).
8. **Texture cache.** Each thumbnail is a new `Picture`, so the dock's `ImagingReplayer`
   would keep every one. `ImagingReplayer::evict_unused` drops textures not drawn since
   the last call; the dock calls it after each paint (this also bounds the icons cached at
   every magnified size).

## Alternatives considered
- **Bind the list lazily on the first request.** The handles would arrive late and the
  first thumbnail would wait a roundtrip; the shell always has minimised windows sooner or
  later.
- **A `neue_shell_v1` request that makes a capture source.** A private protocol change for
  what the standard protocols already do.
- **Hand the app the raw shm pixels and scale in the shell.** It would move multi-MB
  copies across the app boundary every frame and make every app deal with shm formats.
- **dmabuf capture.** Faster for large windows, but needs GBM allocation in the shell; shm
  is enough at two frames a second.

## Consequences
- The shell binds `ext-foreign-toplevel-list-v1`, the capture globals and a session per
  minimised window; each frame is drawn offscreen by the compositor (ADR 0023) and copied
  into shm.
- Most clients don't draw while minimised (no frame callbacks), so a thumbnail usually
  shows the window as it was when it was minimised; clients that commit anyway update it.
- neue-comp gains `State::capture_session_count` for tests.

## Not done
- dmabuf buffers; capturing title bars (ADR 0023's bar-inclusive option); thumbnails
  elsewhere (window switcher, Overview), which can use the same request.
