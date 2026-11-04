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
    self, BEVEL_SIZE, COMBO_SIZE, FIELD_SIZE, MENU_SIZE, POPUP_SIZE, PROGRESS_SIZE, PULL_DOWN_SIZE,
    PUSH_BUTTON_SIZE, SCROLL_SIZE, SEGMENTED_SIZE, SLIDER_SIZE, SPINNER_SIZE, STEPPER_SIZE,
    SUBMENU_SIZE, TEXT_VIEW_SIZE, TOGGLE_SIZE,
};
use neue_kit::testing::{
    self, DISABLED, FOCUSED, Fixture, Goldens, INACTIVE, NORMAL, NavKey, PRESSED, State,
};
use neue_kit::tokens::motion::DEFAULT_BUTTON_PULSE;
use neue_kit::{Accent, Element, MaterialTile, Theme, Variant};

fn goldens() -> Goldens {
    Goldens::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"),
        env!("CARGO_TARGET_TMPDIR"),
    )
}

/// Scrolled 60 px down with the pointer over the content: the thin knob shows.
const SCROLLED_VIEW: State = ("scrolled", |f: &mut Fixture| f.scroll(0.0, -60.0));
/// Scrolled right and down: both scrollers, with a gap where they would meet.
const SCROLLED_BOTH: State = ("scrolled", |f: &mut Fixture| f.scroll(-60.0, -60.0));
/// Far down a long list: the knob at its minimum length.
const SCROLLED_FAR: State = ("scrolled", |f: &mut Fixture| f.scroll(0.0, -5000.0));
/// The pointer on the vertical scroller after scrolling: knob widened on its track.
const SCROLLER_HOVERED: State = ("scroller-hovered", |f: &mut Fixture| {
    f.scroll(0.0, -60.0);
    f.hover_at(0.95, 0.5);
    f.advance(Duration::from_secs(1));
});

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

/// A focused text view after typing two paragraphs, the second wrapping.
const TYPED_LINES: State = ("typed", |f: &mut Fixture| {
    f.focus();
    f.type_text("First line\nA second line long enough to wrap around the view");
});

/// A focused text view typed into until the caret scrolled the view down.
const TYPED_PAST_BOTTOM: State = ("scrolled", |f: &mut Fixture| {
    f.focus();
    f.type_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
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

/// A secondary click on the area: the menu opens at the pointer, nothing highlighted.
const MENU_OPEN: State = ("open", Fixture::context_click);
/// Opened, then Down twice: Copy highlighted, its shortcut in white.
const MENU_HIGHLIGHTED: State = ("highlighted", |f: &mut Fixture| {
    f.context_click();
    f.press_key(NavKey::Down);
    f.press_key(NavKey::Down);
});
/// Opened, "t" typed (Text Size) and Right pressed: the submenu, its first item highlighted.
const MENU_SUBMENU: State = ("submenu", |f: &mut Fixture| {
    f.context_click();
    f.type_text("t");
    f.press_key(NavKey::Right);
});

/// A pop-up or pull-down button opened from the keyboard: its first (or chosen) item
/// highlighted.
const OPEN_BY_KEY: State = ("open-by-key", |f: &mut Fixture| {
    f.focus();
    f.press_key(NavKey::Down);
});
/// The pointer goes down on a pop-up button (padded, so not `PRESSED`, which wants the
/// padding's child on top): the menu opens over it.
const POPUP_PRESSED: State = ("pressed", |f: &mut Fixture| f.press_at(0.5, 0.5));
/// The pointer goes down on a combo box's arrow button: its menu opens below the field.
const COMBO_OPEN: State = ("open", |f: &mut Fixture| f.press_at(0.95, 0.5));
/// A field with its text selected, right-clicked: the edit menu (opened after the event, so
/// a frame is needed to put its rows in the tree).
const EDIT_MENU: State = ("edit-menu", |f: &mut Fixture| {
    f.focus();
    f.select_text();
    f.context_click();
    f.advance(Duration::from_millis(16));
});

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

    text_view: "text-view", TEXT_VIEW_SIZE, &[NORMAL, FOCUSED, TYPED_LINES, TYPED_PAST_BOTTOM, COMPOSING, TYPED_INACTIVE], specimens::text_view();
    text_view_filled: "text-view-filled", TEXT_VIEW_SIZE, &[NORMAL, SCROLLED_VIEW, SELECTED, DISABLED], specimens::text_view_filled();
    text_view_read_only: "text-view-read-only", TEXT_VIEW_SIZE, &[NORMAL, SELECTED], specimens::text_view_read_only();

    scroll_view: "scroll-view", SCROLL_SIZE, &[NORMAL, SCROLLED_VIEW, SCROLLER_HOVERED, FOCUSED], specimens::scroll_view(12);
    scroll_view_both: "scroll-view-both", SCROLL_SIZE, &[SCROLLED_BOTH], specimens::scroll_view_both();
    virtual_list: "virtual-list", SCROLL_SIZE, &[NORMAL, SCROLLED_FAR], specimens::virtual_list(10_000);

    pop_up_button: "pop-up-button", POPUP_SIZE, &[NORMAL, POPUP_PRESSED, OPEN_BY_KEY, FOCUSED, DISABLED], specimens::pop_up_button();
    pull_down_button: "pull-down-button", PULL_DOWN_SIZE, &[NORMAL, PRESSED, OPEN_BY_KEY, FOCUSED, DISABLED], specimens::pull_down_button(|_| ());
    combo_box: "combo-box", COMBO_SIZE, &[NORMAL, FOCUSED, COMBO_OPEN, TYPED], specimens::combo_box();
    combo_box_filled: "combo-box-filled", COMBO_SIZE, &[NORMAL, DISABLED], specimens::combo_box_filled();
    edit_menu: "edit-menu", MENU_SIZE, &[EDIT_MENU], specimens::edit_menu_field();

    context_menu: "context-menu", MENU_SIZE, &[MENU_OPEN, MENU_HIGHLIGHTED], specimens::context_menu(|_| (), specimens::ctrl);
    context_submenu: "context-menu", SUBMENU_SIZE, &[MENU_SUBMENU], specimens::context_menu(|_| (), specimens::ctrl);
}
