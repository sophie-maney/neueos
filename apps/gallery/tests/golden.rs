//! Push-button goldens (task 2.9): every state × light/dark × scale 1 and 2, rendered by
//! neue-kit's golden fixture with `vello_cpu`.
//!
//! Regenerate with `cargo xtask golden --bless` and inspect the diff before committing.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers; a panic is a test failure"
)]

use std::time::Duration;

use neue_gallery::specimens::{
    self, BEVEL_SIZE, FIELD_SIZE, PROGRESS_SIZE, PUSH_BUTTON_SIZE, SEGMENTED_SIZE, SLIDER_SIZE,
    SPINNER_SIZE, STEPPER_SIZE, TOGGLE_SIZE,
};
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

/// The barber pole a quarter period on (150 ms of its 600 ms).
const STRIPES_MOVED: State = ("moved", |f: &mut Fixture| {
    f.advance(Duration::from_millis(150))
});

/// A focused field after typing.
const TYPED: State = ("typed", |f: &mut Fixture| {
    f.focus();
    f.type_text("Typed text");
});

/// A focused field with all its text selected.
const SELECTED: State = ("selected", |f: &mut Fixture| {
    f.focus();
    f.type_text("Selected");
    f.select_text();
});

/// A focused field while an input method composes (underlined preedit after the caret).
const COMPOSING: State = ("composing", |f: &mut Fixture| {
    f.focus();
    f.type_text("Hi ");
    f.compose("there");
});

/// Typed past the right edge: the text scrolls to keep the caret in view.
const SCROLLED: State = ("scrolled", |f: &mut Fixture| {
    f.focus();
    f.type_text("A sentence too long to fit in the field");
});

/// Typed into, in a window that is no longer active: no ring, no caret.
const TYPED_INACTIVE: State = ("inactive", |f: &mut Fixture| {
    f.focus();
    f.type_text("Typed text");
    f.set_window_focused(false);
});

/// The spinner three of its twelve steps on (250 ms of its 1 s).
const SPOKES_TURNED: State = ("turned", |f: &mut Fixture| {
    f.advance(Duration::from_millis(250))
});

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

    // The pointer goes down on the middle of the slider: the knob at 0.5 doesn't move.
    slider: "slider", SLIDER_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::slider(0.5);
    slider_min: "slider-min", SLIDER_SIZE, &[NORMAL, FOCUSED], specimens::slider(0.0);
    slider_max: "slider-max", SLIDER_SIZE, &[NORMAL, FOCUSED], specimens::slider(1.0);
    // The pointer goes down on the midline: the lower half is held.
    stepper: "stepper", STEPPER_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::stepper();
    progress_empty: "progress-empty", PROGRESS_SIZE, &[NORMAL], specimens::progress_bar(0.0);
    progress_half: "progress-half", PROGRESS_SIZE, &[NORMAL, DISABLED], specimens::progress_bar(0.5);
    progress_full: "progress-full", PROGRESS_SIZE, &[NORMAL], specimens::progress_bar(1.0);
    barber_pole: "barber-pole", PROGRESS_SIZE, &[NORMAL, STRIPES_MOVED, DISABLED], specimens::barber_pole();
    spinner: "spinner", SPINNER_SIZE, &[NORMAL, SPOKES_TURNED, DISABLED], specimens::spinner();

    text_field: "text-field", FIELD_SIZE, &[NORMAL, FOCUSED, TYPED, SELECTED, COMPOSING, SCROLLED, TYPED_INACTIVE], specimens::text_field();
    text_field_filled: "text-field-filled", FIELD_SIZE, &[NORMAL, DISABLED], specimens::text_field_filled();
    search_field: "search-field", FIELD_SIZE, &[NORMAL, FOCUSED, TYPED], specimens::search_field();
    search_field_filled: "search-field-filled", FIELD_SIZE, &[NORMAL, DISABLED], specimens::search_field_filled();
    secure_field: "secure-field", FIELD_SIZE, &[NORMAL, TYPED, SELECTED], specimens::secure_field();
}
