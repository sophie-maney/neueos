# 0069: The desktop background

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
Task 3.2c. Each output needs a background layer surface under the windows. It shows a
default picture, or a photo the user chose in the config. The toolkit could draw vectors
but no pixels, and there was no image decoder in our own code. `png` is already in the tree
(vello's glifo uses it) and 3.3 will load PNG icons too; wallpapers are mostly JPEG.

## Decision
1. **One background per output**, a `neue-desktop` layer surface on `ShellLayer::Background`
   anchored on all four edges with exclusive zone -1, `LayerKeyboard::None`. It is opened
   and closed with the output, like the bar. Widget: `neue_kit::desktop::Desktop`
   (`Handle<Desktop>::set_photo` / `set_default`).
2. **Default picture.** Vector art in `neue_paint::materials::desktop::default_picture`: a
   sky gradient, a glow (sun in light, moon in dark) and three ranges of hills. The colours
   and the ridge heights are `neue-theme` tokens (`materials::desktop`), with a light and a
   dark variant. It follows the theme's appearance and is sharp at any size and scale.
3. **Photos.** `neue_paint::materials::desktop::Picture` (premultiplied RGBA8 behind an
   `Arc`) is drawn through the existing `ProceduralTexture` cache: a texture the size of the
   output that resamples the photo at the device scale (bilinear, box-averaged when it
   shrinks a lot). So neither the scene nor the backends changed, and the scaling happens
   once per size and scale, not per frame. `Fit` is `Fill` (cover, the default), `Fit`
   (letterboxed in black), `Stretch` or `Center` (one photo pixel per logical pixel).
4. **Config.** `shell.toml` (`neue_config::ShellConfig`): `[desktop] picture = "<path>"`
   (a leading `~/` is the home directory) and `fit = "fill" | "fit" | "stretch" |
   "center"`. A missing, unreadable or undecodable file logs a warning and keeps the built-in
   picture. The config is read when the shell starts; reloading it live waits for the
   Preferences app (3.24), which will also write it.
5. **Decoding.** `neue-shell`'s portable `picture` module, on a background task so a large
   JPEG never stalls the bar: `png` 0.18 (MIT OR Apache-2.0) and `zune-jpeg` 0.5 (MIT OR
   Apache-2.0 OR Zlib), both pure Rust and GPL-3.0-or-later compatible. The format comes
   from the first bytes, not the extension. Limits: 128 MiB per file, 100 million pixels.
   Alpha is composited over black; 16-bit and palette PNGs are reduced to 8-bit RGBA;
   progressive and CMYK JPEGs are whatever `zune-jpeg` supports. No colour management: the
   pixels are taken as sRGB.

## Alternatives considered
- **The `image` crate:** pulls in many codecs we don't want (and their licences to check).
- **A new image brush in `Scene`:** the right general API once something else needs
  pictures (the dock's icons, 3.3), but it touches all three backends; the texture cache
  already does what a wallpaper needs.
- **Decoding in the compositor and drawing the wallpaper there:** would make the picture
  work without the shell, but the compositor is single-threaded (no blocking in it) and
  shouldn't hold a decoder. The shell restarts within a second (ADR 0066).

## Consequences
- Until the photo is decoded (a moment after start) the built-in picture shows, and so
  it does after a shell restart.
- Pictures far larger than the output cost memory in the shell (width × height × 4); the
  100-megapixel limit caps that at 400 MB. Downscaling while decoding can come later.
- Dependencies added to the workspace: `png`, `zune-jpeg`.
