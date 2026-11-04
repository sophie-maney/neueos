# Masonry spike A: versions and paint (task 2.1, 2026-11-04)

## Result: the versions reconcile, and paint works. The gate's first half passes.
`masonry_core`, `masonry_testing` and `masonry_winit` build together with our pins, using one
copy each of `vello_cpu`/`vello_gpu` 0.3, wgpu 30, parley 0.12, accesskit 0.25, kurbo 0.13
and peniko 0.6. That takes a small fork (below) and a git pin of `imaging`. A Masonry widget paints
the neue-paint Aqua button through a new `imaging::Painter` replay target. Its
`masonry_testing` render matches neue-paint's own goldens on macOS aarch64, Linux aarch64
and Linux x86_64. The second half of the gate (the sctk driver, xdg-popup and
text-input-v3) is task 2.2.

Effort: about half a day, well under the ADR 0002 §4 budget of two days.

## Upstream state (2026-11-04)
- Masonry `main` (`dc49687`, 2026-11-03) pins parley 0.8, accesskit 0.24, wgpu 28 and
  crates.io `imaging` 0.0.1. That release's `imaging_vello_cpu` 0.0.1 renders through
  `vello_cpu` 0.0.7, and its brushes are `peniko::Brush`.
- `imaging` `main` (`f3f932a`, 2026-10-17) already targets `vello_cpu`/`vello_gpu` 0.3 and
  wgpu 30, which match our pins. It has not been released, and its crate versions are still 0.0.1/0.0.2.
  Since 0.0.1 it gained its own `Brush`/`ImageBrush` (with external images), filter
  transforms and validation. `masonry_core` compiles against it unchanged, because
  `From<peniko::Brush>` impls cover the old call sites.
- linebender/xilem#1864 (open, review requested) moves Masonry to parley 0.12 and
  accesskit 0.25. parley 0.12 dropped its `accesskit` feature, so the PR adds
  `masonry_core/src/core/text_accessibility.rs` (+495 lines, a port of parley's old
  AccessKit glue). It merges cleanly on `main`.

## Fork cost
Fork `sophie-maney/xilem`, branch `neueos`, rev `499f423` (ADR 0030). On top of upstream
`dc49687`:

| Change | Size | Upstreamable? |
|---|---|---|
| Merge of #1864: parley 0.12, accesskit 0.25 | +495 new file, ~60 lines elsewhere, 160 screenshots | it's upstream's own PR |
| `imaging*` → git `f3f932a`, wgpu 28 → 30 (workspace `Cargo.toml`) | 8 lines | yes, once `imaging` releases |
| wgpu 30 API in `masonry_imaging` (1 line) and `masonry_winit` (~30 lines: instance descriptor, `CurrentSurfaceTexture`, `queue.present`, surface colour space, objc2 Metal layer call) | ~30 lines | yes |
| No C dependencies: parley without `system` (no fontconfig); `oxipng` made an optional feature of `masonry_testing` (no libdeflate) | ~15 lines | `oxipng` feature: maybe; parley: no, it's our policy |

Our own patches total about 55 lines over 7 files. Rebasing onto a newer upstream should
take under an hour while #1864 is open. Once it's merged, it's mostly the wgpu and `imaging`
pins. The biggest ongoing cost is `imaging`'s churn: it is pre-alpha and unreleased, and we
pin a commit.

## Dependency footprint
- No duplicate rendering, text or a11y crates. The only duplicates are small macOS-only
  ones (objc2 0.5 via `copypasta` in `masonry_winit`) and `pollster` 0.4/1.0.
- Vello's compute renderer (`vello`) and Skia are not in the lockfile. `masonry_winit` is
  built with `imaging_vello_hybrid` (vello_gpu) only.
- New licences: Apache-2.0, MIT, BSD-2/3, BSL-1.0, BlueOak-1.0 (anymap3, dual-licensed),
  Zlib. All are compatible with GPL-3.0-or-later.
- No C libraries on Linux. `masonry_winit` pulls in winit's default X11 features, so it is
  a macOS-only dev-dependency of `neue-paint`, and task 2.4 replaces it.

## Paint
- `neue_paint::imaging::ImagingReplayer` (feature `imaging`) replays a `Scene` into any
  `imaging::Painter`. Commands map one to one: fill, stroke, blurred rounded rect, layer to
  group (clip and opacity), glyph run. Coordinates stay logical, because Masonry applies the
  window scale at render time. Procedural textures are generated at `PaintCtx::scale_factor()`
  and passed as premultiplied `ImageData` image brushes, cached per (texture, scale).
- Brush transforms have the same convention as vello (relative to the geometry
  transform), so the texture maths is shared with the vello replay (`replay.rs`).
- `tests/masonry.rs`: a `masonry_core` widget paints the button tile (linen and Aqua button)
  through the replayer. `masonry_testing` renders it, and the output is compared with
  `tests/golden/button-*.png` (rendered by neue-paint's `CpuRenderer`), using the existing
  tolerance (channel ≤ 2, ≤ 0.1 % of pixels over). Measured on macOS: 80–90 % of pixels
  identical, the worst channel difference is 2, and no pixel is over tolerance. The
  remaining difference is the pipeline: `imaging_vello_cpu` uses the u8 pipeline, while
  our reference goldens use f32.
- `examples/masonry_button.rs` (macOS): the same widget in a `masonry_winit` window on
  vello_gpu. It shows the pressed state while the pointer is held.

## Open items for later tasks
- **System fonts.** With parley's `system` feature off, `RenderRootOptions::use_system_fonts`
  has no effect, and apps get only the fonts we register. Apps will need fontconfig
  discovery on Linux, which is a C dependency, so it needs an ADR when 2.7 or 2.10
  needs it.
- **Golden harness (2.9).** `masonry_testing` can't choose the f32 pipeline. Either keep
  comparing within tolerance, or render goldens with our own harness around `RenderRoot`
  and `CpuRenderer`.
- **`imaging` image brushes** now support `ExternalImage`, which may help with
  `VisualLayerKind::External` and backdrop content (Phase 3).
- Don't upstream anything for now (owner's call). Track #1864 and an `imaging` release, and
  move back to upstream pins once both have landed.

---

# Masonry spike B: sctk driver (task 2.2, 2026-11-04)

## Result: the driver works under neue-comp. The gate passes (ADR 0002 amendment).
`crates/neue-platform/src/wayland/` (about 1,700 lines with rustfmt's layout) drives one
`RenderRoot` per surface on smithay-client-toolkit 0.21.1, calloop and wgpu.
`examples/sctk_spike.rs` shows the neue-paint Aqua button, a Masonry `TextInput` and a
`Selector` in an xdg-toplevel, plus a label in a layer-shell surface. Checked in the VM under
the installed neue-comp session (`docs/checks/phase-2.md` §1):

| Piece | Result |
|---|---|
| xdg-toplevel (server-side title bar) | renders; resize via configure |
| layer-shell (top, anchored top) | renders at its configured size |
| wgpu | virgl GLES with `WGPU_BACKEND=gl` (llvmpipe Vulkan otherwise) |
| fractional scale + viewporter | output scale 1.5: buffer 1.5×, `wp_viewport` destination = logical size, text sharp |
| text-input-v3 | preedit (underlined) then commit into `TextInput`, via `examples/ime_probe.rs` |
| Masonry layer as xdg-popup | `Selector` menu: popup with grab, item click → `SelectionChanged`, outside click → `popup_done` → menu closes |

Effort: about one day, inside the ADR 0002 budget.

## How it works
- **Rendering:** `masonry_imaging`'s vello_gpu (hybrid) renderer draws a `PreparedFrame`
  into an offscreen `Rgba8Unorm` texture, which is blitted to the surface (as
  `masonry_winit` does). The offscreen texture has no `STORAGE_BINDING`, which GLES 3.0
  lacks. Surface: non-sRGB format, `Fifo`, adapter limits.
- **Frames:** a window is drawn when Masonry asks for a redraw and no frame callback is
  pending. `AnimFrame` gets the elapsed time since the last frame.
- **Layers → popups:** each `RenderRootSignal::NewLayer` is added to the `RenderRoot`, then
  shown as an xdg-popup sized to the layer root's bounding box (positioner anchored at its
  origin, flip-y and slide). `redraw()`'s plan is split at layer roots. The base layer
  goes to the window, and each popup's run goes to its popup with `translate(-origin)`.
  Pointer events on a popup get `origin` added. A configure at a different origin (flip or slide)
  calls `reposition_layer`. `popup_done` sends Masonry a pointer `Cancel`, and Masonry's
  menu layers close on that.
- **text-input-v3:** `StartIme`/`EndIme`/`ImeMoved` map to enable/disable/cursor rectangle;
  `preedit_string`/`commit_string` are applied on `done` as `Ime::Preedit`/`Ime::Commit`.
- **Scale:** `wp_fractional_scale_v1` when the compositor has it (with viewporter);
  otherwise `preferred_buffer_scale` and `set_buffer_scale`. Popups use their parent's scale.

## Gotchas (fixed in the spike)
- sctk binds the seats that exist at start-up without calling `SeatHandler::new_seat`, so the
  driver walks `SeatState::seats()` after start-up.
- neue-comp gives the popup keyboard focus with its grab. Masonry's `SelectorMenu` closes on
  `WindowFocusChange(false)`, so a keyboard `leave` is deferred to the end of the event batch
  and dropped if focus lands on another surface of the same window.
- `xdg_popup.grab` must come before the popup's first commit: the driver uses
  `Popup::from_surface` and commits itself (sctk's `Popup::new` commits immediately).
- Masonry's `Selector` opens on button *release*. neue-comp accepts that serial, because the
  pointer's implicit grab has ended by then.
- Smithay sends text-input `enter` only on a keyboard focus change while an input method is
  bound: start the input method before the app.

## Found in neue-comp (not fixed here)
- **Layer surfaces get no scale.** `refresh_surface_scales`
  (`compositor/neue-comp/src/outputs.rs`) walks windows only, so at scale 1.5 the
  layer-shell surface stays at 1 and is upscaled (blurry). The menu bar and dock (Phase 3)
  need this. It's a few lines over `layer_map_for_output(..).layers()`.
- **No `zwp_virtual_keyboard_manager_v1`** (ADR 0016 "Not done"). fcitx5 5.1.23 binds
  `zwp_input_method_manager_v2` but never creates an input method without it, so no fcitx5
  engine works under neue-comp today. The Phase 1 check §12 needs a re-run once it lands.
- **Pointer focus after a client-destroyed popup.** When the client destroys a grabbed popup
  itself (a menu item was picked), the next absolute ydotool motion over the window brings
  no `wl_pointer.enter`, and a click then lands on nothing and deactivates the window. A
  relative motion first restores focus. Seen with ydotool only; check it with a real mouse
  in 2.6.

## Open items for 2.5/2.6
- `RepositionLayer` from Masonry only moves the layer; the popup keeps its first placement
  (needs `xdg_popup.reposition`, 2.6). `LayerType` (tooltip, selector) can choose the
  positioner per kind.
- Key repeat, a full keysym/evdev table (`Code` is `Unidentified`), cursor shape, AccessKit,
  clipboard: not in the spike.
- Fonts: only Fira Sans is registered, so CJK text has no glyphs and its line collapses (the
  commit still reaches the text, as `Changed("日本")` shows). This is the system-fonts open
  item above.
- Surrounding text and `delete_surrounding_text`: Masonry has no API for them yet.
