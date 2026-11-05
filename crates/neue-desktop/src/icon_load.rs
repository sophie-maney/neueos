//! Loading icon files into [`neue_paint::icon::Icon`]s: PNG pixels as they are, SVG
//! rasterised (with `resvg`) at the size it will be drawn.

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use neue_paint::icon::Icon;
use neue_paint::materials::desktop::Picture;
use png::{ColorType, Decoder, Transformations};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
use thiserror::Error;

use crate::icon_theme::{IconFile, IconFormat, IconLookup};

/// An icon file larger than this is refused.
const MAX_FILE_SIZE: u64 = 8 << 20;
/// A PNG with more pixels than this is refused (icons are up to 512 by 512).
const MAX_PIXELS: u64 = 4096 * 4096;
/// An SVG is rasterised at most this many pixels on a side.
const MAX_SVG_SIDE: u32 = 2048;

/// Why an icon file could not be loaded.
#[derive(Debug, Error)]
pub enum IconLoadError {
    /// The file could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        source: std::io::Error,
    },
    /// The file is over 8 MiB.
    #[error("{0} is too large for an icon")]
    TooLarge(PathBuf),
    /// The PNG is damaged, too large, or in a format that is not handled.
    #[error("bad PNG: {0}")]
    Png(String),
    /// The SVG does not parse or is empty.
    #[error("bad SVG: {0}")]
    Svg(String),
}

/// Loads the file `file` as an icon. A PNG keeps its own size; an SVG is rasterised so its
/// longer side is `px` device pixels (at most 2048).
pub fn load_file(file: &IconFile, px: u32) -> Result<Icon, IconLoadError> {
    let io = |source| IconLoadError::Io {
        path: file.path.clone(),
        source,
    };
    if fs::metadata(&file.path).map_err(io)?.len() > MAX_FILE_SIZE {
        return Err(IconLoadError::TooLarge(file.path.clone()));
    }
    let bytes = fs::read(&file.path).map_err(io)?;
    let picture = match file.format {
        IconFormat::Png => decode_png(&bytes)?,
        IconFormat::Svg => rasterize_svg(&bytes, px)?,
    };
    Ok(Icon::raster(picture))
}

fn decode_png(bytes: &[u8]) -> Result<Picture, IconLoadError> {
    let bad = |e: png::DecodingError| IconLoadError::Png(e.to_string());
    let mut decoder = Decoder::new(Cursor::new(bytes));
    // Palettes, low bit depths and transparency to plain 8-bit channels.
    decoder.set_transformations(Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(bad)?;
    let info = reader.info();
    if u64::from(info.width) * u64::from(info.height) > MAX_PIXELS {
        return Err(IconLoadError::Png("too many pixels".into()));
    }
    let len = reader
        .output_buffer_size()
        .ok_or_else(|| IconLoadError::Png("too large".into()))?;
    let mut buf = vec![0; len];
    let frame = reader.next_frame(&mut buf).map_err(bad)?;
    let data = &buf[..frame.buffer_size()];
    let rgba: Vec<u8> = match frame.color_type {
        ColorType::Rgba => data.to_vec(),
        ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        // `normalize_to_color8` turns palettes into RGB(A).
        ColorType::Indexed => return Err(IconLoadError::Png("unexpanded palette".into())),
    };
    Picture::from_rgba8(frame.width, frame.height, &rgba)
        .ok_or_else(|| IconLoadError::Png("bad size".into()))
}

fn rasterize_svg(bytes: &[u8], px: u32) -> Result<Picture, IconLoadError> {
    let bad = |what: &str| IconLoadError::Svg(what.to_owned());
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())
        .map_err(|e| IconLoadError::Svg(e.to_string()))?;
    let size = tree.size();
    let longer = size.width().max(size.height());
    if !(longer.is_finite() && longer > 0.0) {
        return Err(bad("empty"));
    }
    let k = px.clamp(1, MAX_SVG_SIDE) as f32 / longer;
    let side = |s: f32| ((s * k).round() as u32).clamp(1, MAX_SVG_SIDE);
    let (w, h) = (side(size.width()), side(size.height()));
    let mut pixmap = Pixmap::new(w, h).ok_or_else(|| bad("bad size"))?;
    resvg::render(&tree, Transform::from_scale(k, k), &mut pixmap.as_mut());
    Picture::from_premultiplied_rgba8(w, h, pixmap.data()).ok_or_else(|| bad("bad size"))
}

/// Looks icons up and loads them, remembering what it loaded: ask for the same icon at the
/// same size again and the answer is a clone.
#[derive(Debug)]
pub struct IconLoader {
    lookup: IconLookup,
    cache: HashMap<(String, u32, u32), Option<Icon>>,
}

impl IconLoader {
    /// A loader over `lookup`.
    pub fn new(lookup: IconLookup) -> Self {
        Self {
            lookup,
            cache: HashMap::new(),
        }
    }

    /// The icon named `name` (or at the absolute path `name`) for `size` logical pixels at
    /// `scale`. `None` when no theme has it or the file is unusable (that is logged once);
    /// callers show a generic icon then.
    pub fn load(&mut self, name: &str, size: u32, scale: f64) -> Option<Icon> {
        let scale = if scale.is_finite() {
            scale.max(1.0)
        } else {
            1.0
        };
        // Scales are small positive numbers; 1/100 steps keep the key exact.
        let key = (name.to_owned(), size, (scale * 100.0).round() as u32);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let whole = scale.ceil() as u32;
        let px = (f64::from(size) * scale).ceil() as u32;
        let icon = self.lookup.find(name, size, whole).and_then(|file| {
            load_file(&file, px)
                .inspect_err(|error| tracing::warn!(%error, icon = name, "cannot load icon"))
                .ok()
        });
        self.cache.insert(key, icon.clone());
        icon
    }

    /// Forgets what was loaded (after the theme's files changed).
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// The lookup, for finding files without loading them.
    pub fn lookup(&self) -> &IconLookup {
        &self.lookup
    }
}
