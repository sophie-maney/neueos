//! The desktop picture (task 3.2c, ADR 0069): which picture `shell.toml` asks for, and PNG
//! and JPEG decoding into a [`Picture`]. Portable: the shell decodes on a background task,
//! the tests on any host.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use neue_config::{DesktopConfig, PictureFit};
use neue_kit::desktop::{Fit, Picture};

/// The largest file read (a wallpaper is a few MiB; this stops a wrong path from eating
/// memory).
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
/// The largest picture decoded, in pixels (8K is 33 million).
const MAX_PIXELS: u64 = 100_000_000;

/// Why a picture could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum PictureError {
    #[error("reading {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("the file is {0} bytes, over the {MAX_FILE_BYTES} byte limit")]
    TooLarge(u64),
    #[error("not a PNG or JPEG file")]
    UnknownFormat,
    #[error("the picture is {0} × {1} pixels, over the {MAX_PIXELS} pixel limit")]
    TooManyPixels(u64, u64),
    #[error("decoding the PNG: {0}")]
    Png(#[from] png::DecodingError),
    #[error("decoding the JPEG: {0}")]
    Jpeg(String),
}

/// The configured picture and its placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub path: PathBuf,
    pub fit: Fit,
}

/// The picture `config` asks for, with `~/` resolved against `home`; `None` for the
/// built-in picture.
pub fn wanted(config: &DesktopConfig, home: Option<&Path>) -> Option<Wanted> {
    let path = config.picture.as_ref()?;
    let path = match (path.strip_prefix("~"), home) {
        (Ok(rest), Some(home)) => home.join(rest),
        _ => path.clone(),
    };
    let fit = match config.fit {
        PictureFit::Fill => Fit::Fill,
        PictureFit::Fit => Fit::Fit,
        PictureFit::Stretch => Fit::Stretch,
        PictureFit::Center => Fit::Center,
    };
    Some(Wanted { path, fit })
}

/// Reads and decodes the file at `path`.
pub fn load(path: &Path) -> Result<Picture, PictureError> {
    let read = |source| PictureError::Read {
        path: path.to_owned(),
        source,
    };
    let len = std::fs::metadata(path).map_err(read)?.len();
    if len > MAX_FILE_BYTES {
        return Err(PictureError::TooLarge(len));
    }
    decode(&std::fs::read(path).map_err(read)?)
}

/// Decodes a PNG or JPEG (found by its first bytes, not the file name).
pub fn decode(bytes: &[u8]) -> Result<Picture, PictureError> {
    let (width, height, rgba) = if bytes.starts_with(b"\x89PNG") {
        decode_png(bytes)?
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        decode_jpeg(bytes)?
    } else {
        return Err(PictureError::UnknownFormat);
    };
    Picture::from_rgba8(width, height, &rgba).ok_or(PictureError::UnknownFormat)
}

fn check_size(width: u64, height: u64) -> Result<(), PictureError> {
    if width * height > MAX_PIXELS {
        return Err(PictureError::TooManyPixels(width, height));
    }
    Ok(())
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), PictureError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // Palettes, low bit depths and transparency to plain 8-bit channels.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    check_size(u64::from(info.width), u64::from(info.height))?;
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or(PictureError::UnknownFormat)?
    ];
    let frame = reader.next_frame(&mut buf)?;
    buf.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        // `normalize_to_color8` expands palettes.
        png::ColorType::Indexed => return Err(PictureError::UnknownFormat),
    };
    Ok((frame.width, frame.height, rgba))
}

fn decode_jpeg(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), PictureError> {
    let jpeg = |e: zune_jpeg::errors::DecodeErrors| PictureError::Jpeg(format!("{e:?}"));
    let mut decoder = zune_jpeg::JpegDecoder::new(Cursor::new(bytes));
    decoder.decode_headers().map_err(jpeg)?;
    let info = decoder.info().ok_or(PictureError::UnknownFormat)?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    check_size(u64::from(width), u64::from(height))?;
    // RGB, three bytes a pixel (zune-jpeg's default output).
    let rgb = decoder.decode().map_err(jpeg)?;
    let rgba = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|&[r, g, b]| [r, g, b, 255])
        .collect();
    Ok((width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 × 1 RGB PNG: a red pixel and a blue one.
    fn png_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, 2, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255, 0, 0, 0, 0, 255]).unwrap();
        writer.finish().unwrap();
        out
    }

    #[test]
    fn decodes_a_png() {
        let picture = decode(&png_bytes()).unwrap();
        assert_eq!((picture.width(), picture.height()), (2, 1));
    }

    #[test]
    fn decodes_a_jpeg() {
        let bytes = include_bytes!("../tests/fixtures/two-colours.jpg");
        let picture = decode(bytes).unwrap();
        assert_eq!((picture.width(), picture.height()), (4, 2));
    }

    #[test]
    fn refuses_other_files() {
        assert!(matches!(
            decode(b"GIF89a....").unwrap_err(),
            PictureError::UnknownFormat
        ));
        // A truncated PNG is an error, not a panic.
        assert!(decode(&png_bytes()[..20]).is_err());
        assert!(decode(&[0xff, 0xd8, 0xff]).is_err());
    }

    #[test]
    fn a_missing_file_is_a_read_error() {
        let error = load(Path::new("/nonexistent/neue/picture.png")).unwrap_err();
        assert!(matches!(error, PictureError::Read { .. }));
    }

    #[test]
    fn the_home_directory_is_resolved() {
        let config = DesktopConfig {
            picture: Some("~/Pictures/a.png".into()),
            fit: PictureFit::Fit,
        };
        let want = wanted(&config, Some(Path::new("/home/sophie"))).unwrap();
        assert_eq!(want.path, Path::new("/home/sophie/Pictures/a.png"));
        assert_eq!(want.fit, Fit::Fit);
        assert_eq!(wanted(&DesktopConfig::default(), None), None);
    }
}
