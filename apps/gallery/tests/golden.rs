//! Push-button goldens (task 2.9): every state × light/dark × scale 1 and 2, rendered by
//! neue-kit's golden fixture with `vello_cpu`.
//!
//! Regenerate with `cargo xtask golden --bless` and inspect the diff before committing.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a panic is a test failure"
)]

use neue_gallery::specimens::{self, PUSH_BUTTON_SIZE};
use neue_kit::testing::{
    self, DISABLED, FOCUSED, Fixture, Goldens, INACTIVE, NORMAL, PRESSED, State,
};
use neue_kit::tokens::motion::DEFAULT_BUTTON_PULSE;
use neue_kit::{Accent, Element, MaterialTile, Theme, Variant};

fn goldens() -> Goldens {
    Goldens::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"),
        env!("CARGO_TARGET_TMPDIR"),
    )
}

/// The default button at the brightest point of its pulse.
const PULSE_PEAK: State = ("pulse", |f: &mut Fixture| {
    f.advance(DEFAULT_BUTTON_PULSE / 2)
});

#[test]
fn push_button() {
    testing::check_states(
        &goldens(),
        "push-button",
        PUSH_BUTTON_SIZE,
        Theme::default(),
        &[NORMAL, PRESSED, FOCUSED, DISABLED],
        || specimens::push_button::<()>().into(),
    )
    .unwrap();
}

#[test]
fn default_button() {
    testing::check_states(
        &goldens(),
        "default-button",
        PUSH_BUTTON_SIZE,
        Theme::default(),
        &[NORMAL, PULSE_PEAK, PRESSED, FOCUSED, DISABLED, INACTIVE],
        || specimens::default_button::<()>().into(),
    )
    .unwrap();
}

#[test]
fn default_button_graphite() {
    let theme = Theme {
        variant: Variant::default().with_accent(Accent::Graphite),
        ..Theme::default()
    };
    testing::check_states(
        &goldens(),
        "default-button-graphite",
        PUSH_BUTTON_SIZE,
        theme,
        &[NORMAL, PRESSED, FOCUSED],
        || specimens::default_button::<()>().into(),
    )
    .unwrap();
}

/// The Specimen widget, painting the HUD and the menu bar through Masonry: opaque and
/// translucent, light and dark, scale 1 and 2. The materials themselves have their
/// goldens in neue-paint.
#[test]
fn material_specimens() {
    for (tile, name) in [
        (MaterialTile::Hud, "specimen-hud"),
        (MaterialTile::MenuBar, "specimen-menu-bar"),
    ] {
        for (translucent, look) in [(false, "opaque"), (true, "translucent")] {
            testing::check_states(
                &goldens(),
                &format!("{name}-{look}"),
                specimens::material_window_size(tile),
                Theme::default(),
                &[NORMAL],
                move || Element::<()>::from(specimens::material(tile, translucent)),
            )
            .unwrap();
        }
    }
}
