//! Golden-image comparison, shared by every crate with golden tests (neue-paint's own,
//! neue-kit's widget harness, the gallery).
//!
//! Goldens are premultiplied RGBA PNGs rendered with `vello_cpu`. A render matches when at
//! most [`PIXEL_TOLERANCE`] of its pixels differ by more than [`CHANNEL_TOLERANCE`] in any
//! channel: `vello_cpu` output differs slightly between SIMD levels and architectures
//! (docs/research/vello-cpu.md). `NEUE_BLESS=1` (`cargo xtask golden --bless`) rewrites
//! the goldens instead of comparing.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use vello_common::pixmap::Pixmap;

/// Largest per-channel difference that still counts as equal.
pub const CHANNEL_TOLERANCE: u8 = 2;
/// Share of pixels allowed to exceed [`CHANNEL_TOLERANCE`].
pub const PIXEL_TOLERANCE: f64 = 0.001;

/// Why a render didn't match its golden.
#[derive(Debug, thiserror::Error)]
pub enum Mismatch {
    #[error("missing golden {0}; run `cargo xtask golden --bless`")]
    Missing(PathBuf),
    #[error("{name}: size {actual:?} differs from the golden's {expected:?}")]
    Size {
        name: String,
        actual: (u16, u16),
        expected: (u16, u16),
    },
    #[error(
        "{name}: {bad}/{total} pixels differ by more than {CHANNEL_TOLERANCE} (max {worst}); \
         see {}",
        failures.display()
    )]
    Pixels {
        name: String,
        bad: usize,
        total: usize,
        worst: u8,
        failures: PathBuf,
    },
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("{0}: {1}")]
    Png(String, String),
}

/// A pixmap from straight-alpha RGBA8 rows (`image`'s layout, as Masonry's test harness
/// returns them), premultiplied as goldens are stored. `None` if `rgba` isn't
/// `width × height × 4` bytes.
pub fn pixmap_from_unpremultiplied(width: u16, height: u16, mut rgba: Vec<u8>) -> Option<Pixmap> {
    if rgba.len() != usize::from(width) * usize::from(height) * 4 {
        return None;
    }
    vello_common::pixmap::premultiply_rgba8(&mut rgba);
    let mut pixmap = Pixmap::new(width, height);
    pixmap.data_as_u8_slice_mut().copy_from_slice(&rgba);
    Some(pixmap)
}

/// Whether goldens are being rewritten (`NEUE_BLESS=1`).
pub fn bless() -> bool {
    std::env::var_os("NEUE_BLESS").is_some_and(|v| v == "1")
}

/// A directory of goldens, and where to write the actual image and a diff mask when one
/// doesn't match (tests pass `env!("CARGO_TARGET_TMPDIR")`).
#[derive(Clone, Debug)]
pub struct Goldens {
    dir: PathBuf,
    failures: PathBuf,
}

impl Goldens {
    /// Goldens in `dir`; failures go to `failures/golden-failures`.
    pub fn new(dir: impl Into<PathBuf>, failures: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.into(),
            failures: failures.as_ref().join("golden-failures"),
        }
    }

    /// The golden file for `name`.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.png"))
    }

    /// Compares `actual` with golden `name`, or overwrites the golden when blessing.
    pub fn check(&self, name: &str, actual: Pixmap) -> Result<(), Mismatch> {
        if bless() {
            let path = self.path(name);
            let io = |e| Mismatch::Io(path.display().to_string(), e);
            fs::create_dir_all(&self.dir).map_err(io)?;
            let png = actual
                .into_png()
                .map_err(|e| Mismatch::Png(name.to_owned(), e.to_string()))?;
            return fs::write(&path, png).map_err(io);
        }
        self.compare(name, actual)
    }

    /// Compares `actual` with golden `name` within the tolerances. Never blesses.
    pub fn compare(&self, name: &str, actual: Pixmap) -> Result<(), Mismatch> {
        let path = self.path(name);
        let bytes = fs::read(&path).map_err(|_| Mismatch::Missing(path.clone()))?;
        let expected = Pixmap::from_png(Cursor::new(bytes))
            .map_err(|e| Mismatch::Png(path.display().to_string(), e.to_string()))?;
        let (actual_size, expected_size) = (
            (actual.width(), actual.height()),
            (expected.width(), expected.height()),
        );
        if actual_size != expected_size {
            return Err(Mismatch::Size {
                name: name.to_owned(),
                actual: actual_size,
                expected: expected_size,
            });
        }

        let mut worst = 0u8;
        let mut bad = 0usize;
        let mut mask = Pixmap::new(actual.width(), actual.height());
        for ((a, e), m) in actual
            .data()
            .iter()
            .zip(expected.data())
            .zip(mask.data_mut())
        {
            let diff = [
                a.r.abs_diff(e.r),
                a.g.abs_diff(e.g),
                a.b.abs_diff(e.b),
                a.a.abs_diff(e.a),
            ]
            .into_iter()
            .max()
            .unwrap_or(0);
            worst = worst.max(diff);
            if diff > CHANNEL_TOLERANCE {
                bad += 1;
                m.r = 255;
            }
            m.a = 255;
        }
        let total = actual.data().len();
        if bad as f64 / total as f64 <= PIXEL_TOLERANCE {
            return Ok(());
        }
        let io = |e| Mismatch::Io(self.failures.display().to_string(), e);
        let png = |p: Pixmap| {
            p.into_png()
                .map_err(|e| Mismatch::Png(name.to_owned(), e.to_string()))
        };
        fs::create_dir_all(&self.failures).map_err(io)?;
        fs::write(
            self.failures.join(format!("{name}.actual.png")),
            png(actual)?,
        )
        .map_err(io)?;
        fs::write(self.failures.join(format!("{name}.diff.png")), png(mask)?).map_err(io)?;
        Err(Mismatch::Pixels {
            name: name.to_owned(),
            bad,
            total,
            worst,
            failures: self.failures.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_goldens_say_how_to_bless() {
        let goldens = Goldens::new("/nonexistent", std::env::temp_dir());
        let err = goldens.compare("nope", Pixmap::new(1, 1)).unwrap_err();
        assert!(err.to_string().contains("cargo xtask golden --bless"));
    }
}
