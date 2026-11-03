# 0013: Server-side title bars and text in `neue-paint` (task 1.10)

- **Status:** Accepted
- **Date:** 2026-11-03

## Context
PLAN §3.1 has the compositor draw Mountain Lion title bars for clients that accept
server-side decorations, with `neue-paint` on `vello_cpu`, cached and cheap on resize.
`neue-paint` had no text yet, and the compositor drew its windows through Smithay's
`space_render_elements`, which has no place for per-window chrome.

## Decision
1. **Negotiation.** `xdg-decoration` (unstable v1) and KDE's `org_kde_kwin_server_decoration`
   (GTK3 and older Qt use it; its default mode is announced as *server*), both from
   Smithay. We prefer server-side: a new `xdg-decoration` object and `unset_mode` get
   server-side. A client that explicitly asks for client-side gets it, with either
   protocol. Clients that bind neither (GTK4/libadwaita, Firefox by default) draw their
   own. A mode change re-fits a zoomed window.
2. **Geometry.** The space still places a window's *client* geometry; the bar is
   [`TITLE_BAR_HEIGHT`] (22) logical pixels above it, as wide as the window. Placement,
   zoom, move constraints and output rescue use the work area minus the bar
   (`State::content_area`), so the bar is always on screen and a zoomed window's client
   area is the work area less 22 px. `neuectl windows` keeps reporting client geometry
   (no IPC change).
3. **Rendering.** `neue-paint` gains `materials::title_bar`: background (vertical
   gradient, top highlight, bottom separator, 5 px rounded top corners), three traffic
   lights and an embossed, centred title, in light/dark × focused/unfocused. The bar is
   cut into three slices from one minimum-width bar: left (68 px, corner and buttons),
   middle (1 px, stretched) and right (8 px). Slices are rendered once per
   (style, scale) with `vello_cpu` (`Quality::Fast`) and shared; the title is rendered
   per window and only again when the text, style or scale changes, or when it is
   truncated and the width changes. Each window wraps the shared pixmaps in its own
   `MemoryRenderBuffer`s (buffer scale 1, logical destination size, unrounded physical
   position), because Smithay's damage tracker keys elements by buffer id. Resizing a
   window re-renders nothing. Render times are logged at `debug` ("rendered title bar
   slices", "rendered window title", in µs).
4. **Stacking.** A new `render::output_elements` replaces `space_render_elements` in both
   backends: windows top to bottom, each followed by its bar, with layer-shell surfaces
   above and below in Smithay's order. Popups stay above their window's bar.
5. **Text.** `neue-paint::text::TextEngine` wraps parley 0.12 (Apache-2.0 OR MIT) without
   its `system` feature: no system font discovery and no fontconfig (no C dependency),
   so output is identical on every host. Single lines only, with an ellipsis when
   truncated. The `Scene` records glyph runs (`Command::Glyphs`) and both backends replay
   them through vello's `glyph_run` (hinting on). parley shares `skrifa` and the
   `FontData` type with vello 0.3, so no conversion is needed.
6. **Placeholder font: Fira Sans Regular** (from the PLAN §4 candidates), bundled in
   `crates/neue-paint/fonts/` under the SIL Open Font Licence 1.1 (`OFL.txt` next to
   it). The OFL allows bundling and embedding fonts with any software; the font stays
   under the OFL inside our GPL-3.0-or-later binaries (aggregation, as other GPL desktops
   ship OFL fonts). The final UI font is still open question 2.
7. **Appearance** is light for every bar until an appearance setting exists (Phase 3);
   `Chrome::appearance` is the switch. Focus follows the toplevel's `activated` state.

## Consequences
- foot, Qt and GTK3 windows without header bars get our chrome; GTK4/libadwaita and
  header-bar GTK3 apps keep theirs.
- The bar does not react to the pointer yet: clicks on it fall through to whatever is
  below, and there are no hover states or drag-to-move (task 1.11). Hit-testing can use
  `TrafficLight::rect` and `render::frame_geometry`.
- Fractional scales are drawn from pixmaps rendered at that exact scale; slice seams land
  on the same rounded physical pixel, so there are no gaps.
- Golden tests: `titlebar-{focused,unfocused}-{light,dark}@{1x,2x}` in `neue-paint`, plus
  a unit test that the slices match the columns of the full bar.
- Every `neue-paint` user embeds the 450 KB font.

## Alternatives considered
- **Wrap `Window` in our own `SpaceElement`** whose geometry includes the bar (anvil's
  approach): cleaner for hit-testing later, but touches every `Space<Window>` user in the
  WM, IPC and tests. Revisit with Overview (Phase 4) if more per-window chrome appears.
- **One texture per window, re-rendered on resize:** simple but re-rasterises on every
  resize step; the slices avoid that.
- **System fonts through fontique's fontconfig backend:** a C dependency and
  host-dependent goldens.
- **Force server-side even when a client asks for client-side:** double title bars for
  clients that ignore the answer.

## Revisit when
- The UI font is chosen (Q2), or neue-theme tokens replace the hard-coded metrics and
  colours (Phase 2).
- Chrome interaction (1.11) needs the bar in hit-testing.
