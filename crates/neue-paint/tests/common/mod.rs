//! Golden-image helpers shared by the test binaries (`golden.rs`, `masonry.rs`).

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use neue_paint::materials::ButtonState;
use neue_paint::{Appearance, Pixmap};

/// Largest per-channel difference that still counts as equal. vello_cpu output differs
/// slightly between SIMD levels and architectures (docs/research/vello-cpu.md).
const CHANNEL_TOLERANCE: u8 = 2;
/// Share of pixels allowed to exceed `CHANNEL_TOLERANCE`.
const PIXEL_TOLERANCE: f64 = 0.001;

pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

pub fn bless() -> bool {
    std::env::var_os("NEUE_BLESS").is_some_and(|v| v == "1")
}

/// Compare `actual` against golden `name`, or overwrite the golden when blessing.
pub fn check(name: &str, actual: Pixmap) {
    if bless() {
        fs::create_dir_all(golden_dir()).unwrap();
        let path = golden_dir().join(format!("{name}.png"));
        fs::write(&path, actual.into_png().unwrap()).unwrap();
        return;
    }
    compare(name, actual);
}

/// Compare `actual` against golden `name` within the tolerances above. Never blesses.
pub fn compare(name: &str, actual: Pixmap) {
    let path = golden_dir().join(format!("{name}.png"));
    let bytes = fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "missing golden {}; run `cargo xtask golden --bless`",
            path.display()
        )
    });
    let expected = Pixmap::from_png(Cursor::new(bytes)).unwrap();
    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height()),
        "{name}: size differs from golden"
    );

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
    let share = bad as f64 / total as f64;
    if share > PIXEL_TOLERANCE {
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden-failures");
        fs::create_dir_all(&out).unwrap();
        fs::write(
            out.join(format!("{name}.actual.png")),
            actual.into_png().unwrap(),
        )
        .unwrap();
        fs::write(
            out.join(format!("{name}.diff.png")),
            mask.into_png().unwrap(),
        )
        .unwrap();
        panic!(
            "{name}: {bad}/{total} pixels differ by more than {CHANNEL_TOLERANCE} \
             (max {worst}); see {}",
            out.display()
        );
    }
}

/// Name of the button-tile golden for `appearance`, `state` and `scale`. The Masonry spike
/// test (`masonry.rs`) compares against the same files.
pub fn button_golden_name(appearance: Appearance, state: ButtonState, scale: f64) -> String {
    let state = match state {
        ButtonState::Normal => "normal",
        ButtonState::Pressed => "pressed",
    };
    let appearance = match appearance {
        Appearance::Light => "light",
        Appearance::Dark => "dark",
    };
    format!("button-{state}-{appearance}@{scale}x")
}
