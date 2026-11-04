//! Push-button goldens (task 2.9): every state × light/dark × scale 1 and 2, rendered by
//! neue-kit's golden fixture with `vello_cpu`.
//!
//! Regenerate with `cargo xtask golden --bless` and inspect the diff before committing.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a panic is a test failure"
)]

use neue_gallery::specimens::{self, BEVEL_SIZE, PUSH_BUTTON_SIZE, SEGMENTED_SIZE, TOGGLE_SIZE};
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

/// One golden set: `make` in `states`, in a `size` window, under the default theme.
macro_rules! control_goldens {
    ($($test:ident: $name:literal, $size:expr, $states:expr, $make:expr;)+) => {$(
        #[test]
        fn $test() {
            testing::check_states(
                &goldens(),
                $name,
                $size,
                Theme::default(),
                $states,
                || Element::<()>::from($make),
            )
            .unwrap();
        }
    )+};
}

control_goldens! {
    checkbox: "checkbox", TOGGLE_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::checkbox();
    checkbox_on: "checkbox-on", TOGGLE_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::checkbox_on();
    checkbox_mixed: "checkbox-mixed", TOGGLE_SIZE, &[NORMAL, PRESSED, DISABLED], specimens::checkbox_mixed();
    radio: "radio", TOGGLE_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::radio();
    radio_on: "radio-on", TOGGLE_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::radio_on();
    bevel_button: "bevel-button", BEVEL_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::bevel_button();
    bevel_button_on: "bevel-button-on", BEVEL_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::bevel_button_on();
    recessed_button: "recessed-button", BEVEL_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::recessed_button();
    recessed_button_on: "recessed-button-on", BEVEL_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::recessed_button_on();
    // The pointer is over the middle segment: the selected one in `segmented`, a neutral
    // one in `segmented_first`.
    segmented: "segmented", SEGMENTED_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::segmented(1);
    segmented_first: "segmented-first", SEGMENTED_SIZE, &[NORMAL, PRESSED], specimens::segmented(0);
    segmented_last: "segmented-last", SEGMENTED_SIZE, &[NORMAL], specimens::segmented(2);
}
