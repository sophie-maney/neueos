# 0064: `neue_window_v1`, a private protocol for where a window's own chrome ends

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 2.35, runbook step 11.7. neue-comp hangs a sheet from the top of its parent's
window geometry (ADR 0058) when the parent draws its own chrome, because it can't know
where a client-side title bar and toolbar end. Kit windows draw their chrome themselves
(ADR 0055), so on Wayland every sheet covered its parent's toolbar. The height changes at
run time (toolbar display modes, zoom), so it can't be a fixed number. The owner decided
on 2026-11-05 to solve it with a small private protocol.

## Decision
1. **A new private protocol, `neue_window_v1`** (`crates/neue-protocols/protocols/
   neue-window-v1.xml`, documented in `docs/protocols/neue-window-v1.md`):
   `neue_window_manager_v1.get_window(new_id, wl_surface)` and
   `neue_window_v1.set_chrome_height(uint)`, in surface-local logical pixels,
   double-buffered on `wl_surface.commit`. It takes a `wl_surface` rather than an
   `xdg_toplevel`, so neue-protocols needs no xdg-shell bindings, and the height counts
   only while the surface is a toplevel. The name is general on purpose: later per-window
   facts (Phase 4) become new requests in a new version instead of new protocols.
2. **Unprivileged.** It only says where a window's own content is; sandboxed apps get it.
3. **Compositor.** The committed height lives in the surface's cached state
   (`WindowCachedState`). `placement::sheet` takes how far below the top of the parent's
   window geometry the chrome ends (the announced height minus the geometry's offset in
   the surface, clamped to the parent's height); without one, 0 as before.
   `State::attach_sheets` already runs after every toplevel commit, so a change of height
   moves the sheet with the frame that draws it.
4. **Client.** `neue_platform::ChromeHeight` is a shared cell (`Arc<AtomicU32>`) that
   `WindowSpec::chrome_height` hands to the driver. neue-kit's window root writes its
   chrome's laid-out height into it in `layout`, rounded up. The Wayland driver reads it
   after `RenderRoot::redraw`, before the frame is committed, and sends it only when it
   changed, so it lands in the same commit as the frame. The winit driver ignores it
   (macOS sheets are drawn inside the parent, already under the chrome).

## Alternatives considered
- **A callback from the platform into the kit** (`Fn(&RenderRoot) -> Option<f64>`,
  downcasting the root widget): no shared state, but neue-platform would call into a
  widget type it doesn't know and needs a downcast on every frame. The cell is simpler.
- **Guess from the window geometry** (a fixed 22 px, or the title-bar height): wrong for
  every window with a toolbar, the case that matters.
- **`gtk_shell1` or another existing protocol:** none carries this. KDE's and GNOME's
  shells place modal dialogs centred, not hung from a toolbar.
- **A not double-buffered request:** simpler on the server, but the sheet could jump a
  frame before or after the toolbar it hangs from changes height.

## Consequences
- `neuectl windows` shows sheets under the toolbar of kit windows; GTK and Qt windows,
  which don't speak the protocol, keep ADR 0058's behaviour.
- A new public field and type in neue-platform (`WindowSpec::chrome_height`,
  `ChromeHeight`), additive: windows without it are unchanged.
- neue-platform's Wayland driver now depends on neue-protocols (`client` feature; no new
  third-party crate).
- The Phase 4 sheet slide can start the sheet at the same line.
