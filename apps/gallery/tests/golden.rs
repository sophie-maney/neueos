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
    self, BEVEL_SIZE, CALENDAR_SIZE, CHROME_SIZE, COLLECTION_SIZE, COLOR_PANEL_SIZE,
    COLOR_WELL_SIZE, COMBO_SIZE, DATE_SIZE, FIELD_SIZE, ICONS_SIZE, MENU_SIZE, OUTLINE_SIZE,
    PATH_SIZE, POPOVER_SIZE, POPUP_SIZE, PROGRESS_SIZE, PULL_DOWN_SIZE, PUSH_BUTTON_SIZE,
    SCROLL_SIZE, SEGMENTED_SIZE, SHELL_BAR_SIZE, SHELL_DESKTOP_SIZE, SHELL_EXTRAS_SIZE,
    SHELL_NOTIFICATION_SIZE, SHELL_NOTIFICATION_STACK_SIZE, SHELL_OSD_SIZE, SHELL_TRAY_SIZE,
    SIDEBAR_SIZE, SLIDER_SIZE, SPINNER_SIZE, SPLIT_SIZE, STEPPER_SIZE, SUBMENU_SIZE, TABLE_SIZE,
    TABS_SIZE, TEXT_VIEW_SIZE, TOGGLE_SIZE,
};
use neue_kit::testing::{
    self, DISABLED, FOCUSED, Fixture, Goldens, INACTIVE, NORMAL, NavKey, PRESSED, State,
};
use neue_kit::tokens::motion::DEFAULT_BUTTON_PULSE;
use neue_kit::{Accent, DisplayMode, DividerStyle, Edge, Element, MaterialTile, Theme, Variant};

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

/// The Specimen widget, painting the HUD, the menu bar, a menu, a popover and a sheet through Masonry: opaque and
/// translucent, light and dark, scale 1 and 2. The materials themselves have their
/// goldens in neue-paint.
#[test]
fn material_specimens() {
    for (tile, name) in [
        (MaterialTile::Hud, "specimen-hud"),
        (MaterialTile::MenuBar, "specimen-menu-bar"),
        (MaterialTile::Menu, "specimen-menu"),
        (MaterialTile::Popover, "specimen-popover"),
        (MaterialTile::Sheet, "specimen-sheet"),
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
/// The shell bar's logo pressed: its menu open below it, the logo highlighted.
const LOGO_OPEN: State = ("logo-open", |f: &mut Fixture| {
    let (w, h) = SHELL_BAR_SIZE;
    f.press_at(20.0 / f64::from(w), 11.0 / f64::from(h));
});
/// The shell bar's clock pressed: today's date in its menu.
const CLOCK_OPEN: State = ("clock-open", |f: &mut Fixture| {
    let (w, h) = SHELL_BAR_SIZE;
    f.press_at(1.0 - 61.0 / f64::from(w), 11.0 / f64::from(h));
});
/// The shell bar's battery pressed: the charge and the power source in its menu.
const BATTERY_OPEN: State = ("battery-open", |f: &mut Fixture| {
    let (w, h) = SHELL_BAR_SIZE;
    f.press_at(1.0 - 138.0 / f64::from(w), 11.0 / f64::from(h));
});
/// The shell bar's volume icon pressed: the slider, Mute and the output devices.
const VOLUME_OPEN: State = ("volume-open", |f: &mut Fixture| {
    let (w, h) = SHELL_BAR_SIZE;
    f.press_at(1.0 - 177.0 / f64::from(w), 11.0 / f64::from(h));
});
/// The shell bar's network icon pressed: the Wi-Fi switch, the networks in range and
/// the Ethernet port.
const NETWORK_OPEN: State = ("network-open", |f: &mut Fixture| {
    let (w, h) = SHELL_BAR_SIZE;
    f.press_at(1.0 - 212.0 / f64::from(w), 11.0 / f64::from(h));
});
/// The shell bar's Bluetooth icon pressed: its switch and the paired devices.
const BLUETOOTH_OPEN: State = ("bluetooth-open", |f: &mut Fixture| {
    let (w, h) = SHELL_BAR_SIZE;
    f.press_at(1.0 - 246.0 / f64::from(w), 11.0 / f64::from(h));
});

/// A tray icon (the sync app's, with its attention icon) pressed: its menu opens below it,
/// the icon highlighted.
const TRAY_OPEN: State = ("tray-open", |f: &mut Fixture| {
    let (w, h) = SHELL_TRAY_SIZE;
    f.press_at(310.0 / f64::from(w), 11.0 / f64::from(h));
});

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

/// The pointer held down on the fourth row: it's selected, and the table has focus.
const ROW_PRESSED: State = ("row-pressed", |f: &mut Fixture| f.press_at(0.3, 0.7));
/// The pointer held down on the Kind column's title.
const HEADER_PRESSED: State = ("header-pressed", |f: &mut Fixture| f.press_at(0.85, 0.07));

/// The pointer over the Favorites group header: its Hide shows.
const GROUP_HOVERED: State = ("group-hovered", |f: &mut Fixture| f.hover_at(0.5, 0.05));

/// The pointer over the path control's middle segment, "Users" (the window's centre falls
/// in the gap between segments).
const PATH_HOVERED: State = ("hovered", |f: &mut Fixture| f.hover_at(0.6, 0.5));
/// The pointer held down on the path control's middle segment.
const PATH_PRESSED: State = ("pressed", |f: &mut Fixture| f.press_at(0.6, 0.5));

/// The pointer held down on the date picker's stepper, on its upper half.
const DATE_STEPPER_PRESSED: State = ("stepper-pressed", |f: &mut Fixture| f.press_at(0.97, 0.25));
/// The pointer over a day of the calendar (17 November).
const CALENDAR_HOVERED: State = ("hovered", |f: &mut Fixture| f.hover_at(0.233, 0.682));
/// The pointer held down on the calendar's next-month button.
const CALENDAR_NEXT_PRESSED: State = ("next-pressed", |f: &mut Fixture| f.press_at(0.917, 0.108));

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
    path_control: "path-control", PATH_SIZE, &[NORMAL, PATH_HOVERED, PATH_PRESSED, FOCUSED, DISABLED], specimens::path_control();
    date_picker: "date-picker", DATE_SIZE, &[NORMAL, DATE_STEPPER_PRESSED, FOCUSED, DISABLED], specimens::date_picker();
    calendar: "calendar", CALENDAR_SIZE, &[NORMAL, CALENDAR_HOVERED, CALENDAR_NEXT_PRESSED, FOCUSED, DISABLED], specimens::calendar();
    color_well: "color-well", COLOR_WELL_SIZE, &[NORMAL, PRESSED, FOCUSED, DISABLED], specimens::color_well();

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
    // The second row is selected: grey at rest, the accent with focus.
    table: "table", TABLE_SIZE, &[NORMAL, FOCUSED, ROW_PRESSED, HEADER_PRESSED, DISABLED], specimens::table();

    // Closed, one level open, two levels open, and a nested row selected (grey at rest, the
    // accent with focus).
    outline: "outline", OUTLINE_SIZE, &[NORMAL, DISABLED], specimens::outline();
    outline_expanded: "outline-expanded", OUTLINE_SIZE, &[NORMAL], specimens::outline_expanded();
    outline_nested: "outline-nested", OUTLINE_SIZE, &[NORMAL], specimens::outline_nested();
    outline_selected: "outline-selected", OUTLINE_SIZE, &[NORMAL, FOCUSED], specimens::outline_selected();

    // Groups and items with icons and badges; the pointer over a group; a selected item
    // (grey at rest, the accent with focus); nested items; a closed group.
    source_list: "source-list", SIDEBAR_SIZE, &[NORMAL, GROUP_HOVERED, DISABLED], specimens::source_list();
    source_list_selected: "source-list-selected", SIDEBAR_SIZE, &[NORMAL, FOCUSED], specimens::source_list_selected();
    source_list_nested: "source-list-nested", SIDEBAR_SIZE, &[NORMAL, FOCUSED], specimens::source_list_nested();
    source_list_collapsed: "source-list-collapsed", SIDEBAR_SIZE, &[NORMAL], specimens::source_list_collapsed();

    // Every glyph; the vector icon at three sizes (its disc has its own colours, its tick
    // follows the text), disabled dimmed.
    icon_glyphs: "icon-glyphs", ICONS_SIZE, &[NORMAL, DISABLED], specimens::icon_glyphs();
    icon_vector: "icon-vector", ICONS_SIZE, &[NORMAL, DISABLED], specimens::icon_vector();

    // A collection of icons over labels (the last label is cut with an ellipsis); two items
    // selected (grey at rest, the accent with focus); disabled, where the selection hides.
    collection: "collection", COLLECTION_SIZE, &[NORMAL], specimens::collection();
    collection_selected: "collection-selected", COLLECTION_SIZE, &[NORMAL, FOCUSED, DISABLED], specimens::collection_selected();

    // Panes side by side or stacked with thin and thick dividers (a grip on the thick one);
    // the divider focused; a collapsed sidebar; disabled, the dividers dimmed.
    split_thin: "split-thin", SPLIT_SIZE, &[NORMAL, FOCUSED, DISABLED], specimens::split_view(DividerStyle::Thin, false);
    split_thick: "split-thick", SPLIT_SIZE, &[NORMAL, FOCUSED, DISABLED], specimens::split_view(DividerStyle::Thick, false);
    split_stacked: "split-stacked", SPLIT_SIZE, &[NORMAL, FOCUSED], specimens::split_view(DividerStyle::Thick, true);
    split_collapsed: "split-collapsed", SPLIT_SIZE, &[NORMAL], specimens::split_view_collapsed(DividerStyle::Thick);

    // A strip of tabs over a bordered box: the first or the second tab selected, a
    // disabled tab, the strip focused, the whole view disabled, the window inactive (the
    // selected tab neutral).
    tabs: "tabs", TABS_SIZE, &[NORMAL, FOCUSED, DISABLED, INACTIVE], specimens::tab_view(0);
    tabs_second: "tabs-second", TABS_SIZE, &[NORMAL], specimens::tab_view(1);

    pop_up_button: "pop-up-button", POPUP_SIZE, &[NORMAL, POPUP_PRESSED, OPEN_BY_KEY, FOCUSED, DISABLED], specimens::pop_up_button();
    pull_down_button: "pull-down-button", PULL_DOWN_SIZE, &[NORMAL, PRESSED, OPEN_BY_KEY, FOCUSED, DISABLED], specimens::pull_down_button(|_| ());
    combo_box: "combo-box", COMBO_SIZE, &[NORMAL, FOCUSED, COMBO_OPEN, TYPED], specimens::combo_box();
    combo_box_filled: "combo-box-filled", COMBO_SIZE, &[NORMAL, DISABLED], specimens::combo_box_filled();
    edit_menu: "edit-menu", MENU_SIZE, &[EDIT_MENU], specimens::edit_menu_field();

    // The shell's menu bar (3.2b): the logo, the active app's name in bold and the clock;
    // the logo's menu and the clock's menu open, their titles highlighted.
    shell_bar: "shell-bar", SHELL_BAR_SIZE, &[NORMAL, LOGO_OPEN, CLOCK_OPEN, BATTERY_OPEN, VOLUME_OPEN, NETWORK_OPEN, BLUETOOTH_OPEN], specimens::shell_bar("Gallery", |_| (), |_| (), |_| (), |_| (), |_| ());

    // The battery, network (3.12b) and Bluetooth extras' icons in each state (3.11b).
    shell_extras: "shell-extras", SHELL_EXTRAS_SIZE, &[NORMAL], specimens::shell_extras();

    // The status-icon tray (3.13b): items in each state, and a bar too narrow for them all.
    shell_tray: "shell-tray", SHELL_TRAY_SIZE, &[NORMAL, TRAY_OPEN], specimens::shell_tray(());

    // The desktop background (3.2c): the built-in picture, and a photo filling the output
    // (cut off at the sides) and fitting inside it (black bars).
    shell_desktop: "shell-desktop", SHELL_DESKTOP_SIZE, &[NORMAL], specimens::shell_desktop();
    shell_desktop_fill: "shell-desktop-fill", SHELL_DESKTOP_SIZE, &[NORMAL], specimens::shell_desktop_photo(neue_kit::desktop::Fit::Fill);
    shell_desktop_fit: "shell-desktop-fit", SHELL_DESKTOP_SIZE, &[NORMAL], specimens::shell_desktop_photo(neue_kit::desktop::Fit::Fit);

    // The on-screen display (3.14a): the volume at nine and a quarter steps (two waves, a
    // quarter-lit step), muted (no waves, every step unlit) and the brightness at three
    // quarters.
    shell_osd_volume: "shell-osd-volume", SHELL_OSD_SIZE, &[NORMAL], specimens::shell_osd(neue_kit::desktop::OsdGlyph::Volume, 9.25 / 16.0);
    shell_osd_muted: "shell-osd-muted", SHELL_OSD_SIZE, &[NORMAL], specimens::shell_osd(neue_kit::desktop::OsdGlyph::Muted, 9.25 / 16.0);
    shell_osd_brightness: "shell-osd-brightness", SHELL_OSD_SIZE, &[NORMAL], specimens::shell_osd(neue_kit::desktop::OsdGlyph::Brightness, 0.75);
    shell_osd_microphone: "shell-osd-microphone", SHELL_OSD_SIZE, &[NORMAL], specimens::shell_osd(neue_kit::desktop::OsdGlyph::Microphone, 0.75);
    shell_osd_microphone_muted: "shell-osd-microphone-muted", SHELL_OSD_SIZE, &[NORMAL], specimens::shell_osd(neue_kit::desktop::OsdGlyph::MicrophoneMuted, 0.75);
    shell_osd_keyboard: "shell-osd-keyboard", SHELL_OSD_SIZE, &[NORMAL], specimens::shell_osd(neue_kit::desktop::OsdGlyph::Keyboard, 0.5);

    // Notifications (3.15b): a banner at rest, with the pointer on its close button and
    // with keyboard focus; a banner with an image; an alert with two actions, at rest and
    // with the pointer on its close button.
    shell_banner: "shell-banner", SHELL_NOTIFICATION_SIZE, &[NORMAL, NOTE_CLOSE_HOVERED, FOCUSED], specimens::shell_banner(|_| None);
    shell_banner_image: "shell-banner-image", SHELL_NOTIFICATION_SIZE, &[NORMAL], specimens::shell_banner_image(|_| None);
    shell_alert: "shell-alert", SHELL_NOTIFICATION_SIZE, &[NORMAL, NOTE_CLOSE_HOVERED], specimens::shell_alert(|_| None);
    // The notification host (3.15d2): three panels at rest, a banner over an alert over a
    // banner with an image.
    shell_notification_stack: "shell-notification-stack", SHELL_NOTIFICATION_STACK_SIZE, &[NORMAL], specimens::shell_notification_stack(|_, _| None);

    context_menu: "context-menu", MENU_SIZE, &[MENU_OPEN, MENU_HIGHLIGHTED], specimens::context_menu(|_| (), specimens::command);
    context_submenu: "context-menu", SUBMENU_SIZE, &[MENU_SUBMENU], specimens::context_menu(|_| (), specimens::command);
}

/// The pointer on a notification's close button, over the panel's top left corner.
const NOTE_CLOSE_HOVERED: State = ("close-hovered", |f: &mut Fixture| {
    use neue_kit::tokens::materials::notification::{CLOSE_INSET, MARGIN};
    f.hover_at_px(MARGIN + CLOSE_INSET, MARGIN + CLOSE_INSET);
});

/// The pointer over the traffic lights (between close and minimise): every glyph shows.
const LIGHTS_HOVERED: State = ("lights-hovered", |f: &mut Fixture| {
    f.hover_at(
        LIGHTS_X / f64::from(CHROME_SIZE.0),
        LIGHTS_Y / f64::from(CHROME_SIZE.1),
    );
});
/// The pointer held down on the close button.
const CLOSE_PRESSED: State = ("close-pressed", |f: &mut Fixture| {
    f.press_at(
        CLOSE_X / f64::from(CHROME_SIZE.0),
        LIGHTS_Y / f64::from(CHROME_SIZE.1),
    );
});
/// Centre of the close button, the gap after it, and the lights' midline (logical pixels).
const CLOSE_X: f64 = 14.0;
const LIGHTS_X: f64 = 24.0;
const LIGHTS_Y: f64 = 11.0;

// A window's own title bar (task 2.22b): at rest, inactive, the lights hovered, close held.
#[test]
fn window_chrome() {
    testing::check_window_states(
        &goldens(),
        "window-chrome",
        Theme::default(),
        &[NORMAL, INACTIVE, LIGHTS_HOVERED, CLOSE_PRESSED],
        specimens::window_chrome::<()>,
    )
    .unwrap();
}

// A unified title bar and toolbar, at rest and inactive.
#[test]
fn window_toolbar() {
    testing::check_window_states(
        &goldens(),
        "window-toolbar",
        Theme::default(),
        &[NORMAL, INACTIVE],
        specimens::window_toolbar::<()>,
    )
    .unwrap();
}

/// The pointer held down on the first toolbar button.
const TOOLBAR_BUTTON_PRESSED: State = ("button-pressed", |f: &mut Fixture| {
    let (x, y) = specimens::TOOLBAR_FIRST_BUTTON;
    f.press_at(
        x / f64::from(specimens::TOOLBAR_ITEMS_SIZE.0),
        y / f64::from(specimens::TOOLBAR_ITEMS_SIZE.1),
    );
});

// Toolbar items (task 2.22c): buttons with icon and label (at rest, inactive window, the
// first held down, the first focused), icon only, label only.
#[test]
fn toolbar_items_icon_and_text() {
    testing::check_window_states(
        &goldens(),
        "toolbar-items",
        Theme::default(),
        &[NORMAL, INACTIVE, TOOLBAR_BUTTON_PRESSED, FOCUSED],
        || specimens::window_toolbar_items::<()>(DisplayMode::IconAndText),
    )
    .unwrap();
}

#[test]
fn toolbar_items_icon_only() {
    testing::check_window_states(
        &goldens(),
        "toolbar-items-icons",
        Theme::default(),
        &[NORMAL],
        || specimens::window_toolbar_items::<()>(DisplayMode::IconOnly),
    )
    .unwrap();
}

#[test]
fn toolbar_items_text_only() {
    testing::check_window_states(
        &goldens(),
        "toolbar-items-text",
        Theme::default(),
        &[NORMAL],
        || specimens::window_toolbar_items::<()>(DisplayMode::TextOnly),
    )
    .unwrap();
}

// Popovers (task 2.23a): one on each side of its button, the arrow pointing at it.
const POPOVER_BOTTOM: State = ("bottom", |f: &mut Fixture| {
    f.show_popover(specimens::popover::<()>(Edge::Bottom));
});
const POPOVER_TOP: State = ("top", |f: &mut Fixture| {
    f.show_popover(specimens::popover::<()>(Edge::Top));
});
const POPOVER_LEFT: State = ("left", |f: &mut Fixture| {
    f.show_popover(specimens::popover::<()>(Edge::Left));
});
const POPOVER_RIGHT: State = ("right", |f: &mut Fixture| {
    f.show_popover(specimens::popover::<()>(Edge::Right));
});

#[test]
fn popover() {
    testing::check_states(
        &goldens(),
        "popover",
        POPOVER_SIZE,
        Theme::default(),
        &[POPOVER_BOTTOM, POPOVER_TOP, POPOVER_LEFT, POPOVER_RIGHT],
        || specimens::popover_anchor::<()>().into(),
    )
    .unwrap();
}

// The colour well's panel (task 2.24b): opened from the keyboard, under its well.
const PANEL_OPEN: State = ("open", |f: &mut Fixture| {
    f.focus();
    f.press_key(NavKey::Return);
});

#[test]
fn color_panel() {
    testing::check_states(
        &goldens(),
        "color-panel",
        COLOR_PANEL_SIZE,
        Theme::default(),
        &[PANEL_OPEN],
        || specimens::color_well_anchor::<()>().into(),
    )
    .unwrap();
}

// Sheets (task 2.23b): drawn in their window under its title bar (macOS), and as a window
// of their own (Wayland).
const SHEET_ATTACHED: State = ("attached", |f: &mut Fixture| {
    f.show_sheet(specimens::sheet((), ()));
});

// HUD windows (task 2.23d): the dark panel with its small title bar and a line of light text,
// whatever the appearance.
const CLOSE_HOVERED: State = ("close-hovered", |f: &mut Fixture| {
    use neue_kit::tokens::materials::hud::{CLOSE_INSET, CLOSE_SIZE, TITLE_HEIGHT};
    let (w, h) = specimens::HUD_SIZE;
    f.hover_at(
        (CLOSE_INSET + CLOSE_SIZE / 2.0) / f64::from(w),
        TITLE_HEIGHT / 2.0 / f64::from(h),
    );
});

#[test]
fn hud_window() {
    testing::check_window_states(
        &goldens(),
        "hud",
        Theme::default(),
        &[NORMAL, INACTIVE, CLOSE_HOVERED],
        specimens::hud_window::<()>,
    )
    .unwrap();
}

#[test]
fn sheet_attached() {
    testing::check_window_states(
        &goldens(),
        "sheet",
        Theme::default(),
        &[SHEET_ATTACHED],
        specimens::sheet_parent::<()>,
    )
    .unwrap();
}

#[test]
fn sheet_window() {
    testing::check_sheet_states(
        &goldens(),
        "sheet",
        Theme::default(),
        &[("window", |_| {})],
        || specimens::sheet((), ()),
    )
    .unwrap();
}
