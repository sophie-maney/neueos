//! Golden-image helpers shared by the test binaries (`golden.rs`, `masonry.rs`).

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the helpers"
)]

use neue_paint::Appearance;
use neue_paint::Pixmap;
use neue_paint::golden::Goldens;
use neue_paint::materials::ButtonState;

fn goldens() -> Goldens {
    Goldens::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"),
        env!("CARGO_TARGET_TMPDIR"),
    )
}

/// Compare `actual` against golden `name`, or overwrite the golden when blessing.
pub fn check(name: &str, actual: Pixmap) {
    goldens().check(name, actual).unwrap();
}

/// Compare `actual` against golden `name` within the tolerances. Never blesses.
pub fn compare(name: &str, actual: Pixmap) {
    goldens().compare(name, actual).unwrap();
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
