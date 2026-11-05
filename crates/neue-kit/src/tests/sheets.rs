//! Sheets (2.23b) end to end on the fake host, drawn in their parent (as on macOS) and as
//! modal windows of their own (as on Wayland): where they open, Return and Escape, Tab
//! staying inside, the parent taking no pointer input, focus coming back, and closing.

use masonry_core::accesskit::{NodeId, Role};
use masonry_core::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use masonry_core::core::{PointerButton, TextEvent};
use masonry_core::kurbo::{Point, Rect};
use neue_paint::materials::title_bar::TITLE_BAR_HEIGHT;

use super::menus::{find, focused, named, press};
use super::*;
use crate::Sheet;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Show,
    Save,
    Cancel,
    Inside,
    WindowDefault,
    /// Script: the app closes the sheet.
    Close,
    /// Script: the app switches to the dark theme.
    Dark,
}

const WINDOW: (u32, u32) = (480, 320);
const SHEET: (u32, u32) = (200, 120);

#[derive(Default)]
struct Sheets {
    chrome: bool,
    window: Option<WindowId>,
    show: Handle<Button<Msg>>,
    log: Vec<Msg>,
}

impl Application for Sheets {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        // Tall enough for the sheet: the fake host sizes a window's root to its content.
        let root = Stack::column()
            .padding(60.0)
            .spacing(8.0)
            .with(Button::new("Show").on_press(Msg::Show).bind(&mut self.show))
            .with(Button::new("OK").as_default().on_press(Msg::WindowDefault));
        let mut window = Window::new("Document", WINDOW, root);
        if self.chrome {
            window = window.chrome();
        }
        self.window = Some(cx.open_window(window));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        let Some(window) = self.window else { return };
        match message {
            Msg::Show => {
                let content = Stack::column()
                    .spacing(8.0)
                    .with(Label::new("Save changes?"))
                    .with(Button::new("Inside").on_press(Msg::Inside));
                cx.show_sheet(
                    window,
                    Sheet::new(SHEET, content)
                        .title("Save")
                        .default_button("Save", Msg::Save)
                        .cancel_button("Cancel", Msg::Cancel),
                );
            }
            Msg::Close => cx.close_sheet(window),
            Msg::Dark => cx.set_theme(Theme {
                variant: neue_theme::Variant::new(neue_theme::Appearance::Dark),
                ..cx.theme()
            }),
            _ => {}
        }
        self.log.push(message);
    }
}

fn setup(chrome: bool, modal_windows: bool) -> (Runner<Sheets>, FakeHost) {
    let mut runner = Runner::new(
        Sheets {
            chrome,
            ..Sheets::default()
        },
        Settings::default(),
    );
    let mut host = FakeHost {
        modal_windows,
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    for harness in host.windows.values_mut() {
        let _ = harness.render();
    }
    (runner, host)
}

/// The app shows its sheet; clears the log.
fn show(runner: &mut Runner<Sheets>, host: &mut FakeHost) {
    runner.send(host, Msg::Show);
    deliver(runner, host);
    runner.app_mut().log.clear();
}

fn parent(runner: &Runner<Sheets>) -> WindowId {
    runner.app().window.expect("the window is open")
}

/// The sheet's own window (with modal windows).
fn sheet_window(runner: &Runner<Sheets>, host: &FakeHost) -> WindowId {
    let parent = parent(runner);
    *host
        .windows
        .keys()
        .find(|&&w| w != parent)
        .expect("a second window")
}

fn sheets(host: &mut FakeHost) -> Vec<Rect> {
    find(host, Role::Dialog, Some("Save"))
        .into_iter()
        .map(|f| f.bounds)
        .collect()
}

fn click_at(runner: &mut Runner<Sheets>, host: &mut FakeHost, window: WindowId, at: Point) {
    let harness = host.windows.get_mut(&window).expect("an open window");
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

/// Clicks the window's Show button.
fn click_show(runner: &mut Runner<Sheets>, host: &mut FakeHost) {
    let window = parent(runner);
    let harness = host.windows.get_mut(&window).expect("an open window");
    let _ = harness.render();
    let widget = harness.get_widget_with_id(id(&runner.app().show));
    let at = widget.ctx().window_transform() * widget.ctx().border_box().center();
    click_at(runner, host, window, at);
}

/// Presses `key` in `window`.
fn key_in(runner: &mut Runner<Sheets>, host: &mut FakeHost, window: WindowId, key: NamedKey) {
    let harness = host.windows.get_mut(&window).expect("an open window");
    for state in [KeyState::Down, KeyState::Up] {
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state,
            key: Key::Named(key),
            ..KeyboardEvent::default()
        }));
    }
    deliver(runner, host);
}

#[test]
fn it_opens_centred_under_the_title_bar_and_takes_focus() {
    let (mut runner, mut host) = setup(true, false);
    show(&mut runner, &mut host);
    let found = find(&mut host, Role::Dialog, Some("Save"));
    assert_eq!(found.len(), 1, "one sheet");
    let bounds = found[0].bounds;
    let x = f64::from(WINDOW.0 - SHEET.0) / 2.0;
    assert!((bounds.y0 - TITLE_BAR_HEIGHT).abs() < 0.5, "{bounds:?}");
    assert!((bounds.x0 - x).abs() < 0.5, "{bounds:?}");
    assert!((bounds.width() - f64::from(SHEET.0)).abs() < 0.5);
    assert!((bounds.height() - f64::from(SHEET.1)).abs() < 0.5);
    let focus = host.only().focused_widget_id().expect("focus");
    assert_eq!(found[0].node, NodeId(focus.to_raw()), "the sheet has focus");
}

#[test]
fn without_chrome_it_hangs_from_the_top() {
    let (mut runner, mut host) = setup(false, false);
    show(&mut runner, &mut host);
    let bounds = sheets(&mut host)[0];
    assert!(bounds.y0.abs() < 0.5, "{bounds:?}");
}

#[test]
fn return_and_escape_press_its_buttons_and_the_app_closes_it() {
    let (mut runner, mut host) = setup(true, false);
    show(&mut runner, &mut host);
    named(&mut runner, &mut host, NamedKey::Enter);
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(runner.app().log, [Msg::Save, Msg::Cancel]);
    assert_eq!(sheets(&mut host).len(), 1, "its buttons don't close it");
    runner.send(&mut host, Msg::Close);
    deliver(&mut runner, &mut host);
    assert!(sheets(&mut host).is_empty());
}

#[test]
fn its_controls_take_clicks() {
    let (mut runner, mut host) = setup(true, false);
    show(&mut runner, &mut host);
    let window = parent(&runner);
    for label in ["Inside", "Cancel", "Save"] {
        let at = find(&mut host, Role::Button, Some(label))[0]
            .bounds
            .center();
        click_at(&mut runner, &mut host, window, at);
    }
    assert_eq!(runner.app().log, [Msg::Inside, Msg::Cancel, Msg::Save]);
}

#[test]
fn the_window_takes_no_clicks_while_it_is_open() {
    let (mut runner, mut host) = setup(true, false);
    show(&mut runner, &mut host);
    click_show(&mut runner, &mut host);
    assert!(runner.app().log.is_empty(), "{:?}", runner.app().log);
    runner.send(&mut host, Msg::Close);
    deliver(&mut runner, &mut host);
    click_show(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Close, Msg::Show]);
}

#[test]
fn tab_cycles_inside_it() {
    let (mut runner, mut host) = setup(true, false);
    show(&mut runner, &mut host);
    let mut seen = Vec::new();
    for _ in 0..4 {
        named(&mut runner, &mut host, NamedKey::Tab);
        seen.push(focused(&mut host).unwrap_or_default());
    }
    assert_eq!(seen, ["Inside", "Cancel", "Save", "Inside"]);
    press(
        &mut runner,
        &mut host,
        Key::Named(NamedKey::Tab),
        Modifiers::SHIFT,
    );
    assert_eq!(focused(&mut host).as_deref(), Some("Save"));
}

#[test]
fn return_is_the_window_s_again_once_it_closes_and_focus_comes_back() {
    let (mut runner, mut host) = setup(true, false);
    let show_button = id(&runner.app().show);
    host.only().focus_on(Some(show_button));
    show(&mut runner, &mut host);
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(
        runner.app().log,
        [Msg::Save],
        "the sheet's default, not the window's"
    );
    runner.send(&mut host, Msg::Close);
    deliver(&mut runner, &mut host);
    assert_eq!(host.only().focused_widget_id(), Some(show_button));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(
        runner.app().log,
        [Msg::Save, Msg::Close, Msg::WindowDefault]
    );
}

#[test]
fn showing_another_replaces_it() {
    let (mut runner, mut host) = setup(true, false);
    show(&mut runner, &mut host);
    show(&mut runner, &mut host);
    assert_eq!(sheets(&mut host).len(), 1);
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(runner.app().log, [Msg::Cancel]);
}

#[test]
fn with_modal_windows_it_is_a_modal_child_window_without_a_title_bar() {
    let (mut runner, mut host) = setup(true, true);
    show(&mut runner, &mut host);
    assert_eq!(host.windows.len(), 2);
    let parent = parent(&runner);
    let sheet = sheet_window(&runner, &host);
    assert_eq!(host.dialogs[&sheet], (Some(parent), true, SHEET));
    assert_eq!(host.specs[&sheet].1, Decorations::Client);

    key_in(&mut runner, &mut host, sheet, NamedKey::Enter);
    key_in(&mut runner, &mut host, sheet, NamedKey::Escape);
    assert_eq!(runner.app().log, [Msg::Save, Msg::Cancel]);
    // The parent's content still takes no clicks, nor Return.
    click_show(&mut runner, &mut host);
    key_in(&mut runner, &mut host, parent, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Save, Msg::Cancel]);

    runner.send(&mut host, Msg::Close);
    deliver(&mut runner, &mut host);
    assert_eq!(host.windows.len(), 1, "its window closed");
    click_show(&mut runner, &mut host);
    assert_eq!(
        runner.app().log,
        [Msg::Save, Msg::Cancel, Msg::Close, Msg::Show]
    );
}

#[test]
fn with_modal_windows_tab_cycles_inside_it() {
    let (mut runner, mut host) = setup(true, true);
    show(&mut runner, &mut host);
    let sheet = sheet_window(&runner, &host);
    let mut seen = Vec::new();
    for _ in 0..4 {
        key_in(&mut runner, &mut host, sheet, NamedKey::Tab);
        let harness = host.windows.get_mut(&sheet).expect("the sheet's window");
        let _ = harness.render();
        seen.push(
            harness
                .access_tree()
                .state()
                .focus_in_tree()
                .label()
                .unwrap_or_default(),
        );
    }
    assert_eq!(seen, ["Inside", "Cancel", "Save", "Inside"]);
}

#[test]
fn the_compositor_closing_its_window_cancels_it() {
    let (mut runner, mut host) = setup(true, true);
    show(&mut runner, &mut host);
    let sheet = sheet_window(&runner, &host);
    runner.close_requested(&mut host, sheet);
    assert_eq!(runner.app().log, [Msg::Cancel]);
    assert_eq!(host.windows.len(), 2, "the app decides when it closes");
}

#[test]
fn closing_the_window_closes_its_sheet() {
    let (mut runner, mut host) = setup(true, true);
    show(&mut runner, &mut host);
    let parent = parent(&runner);
    runner.close_requested(&mut host, parent);
    assert!(host.windows.is_empty());
}

/// The colour at the middle of the top of `window`'s content, inside the sheet's panel.
fn panel_pixel(host: &mut FakeHost, window: WindowId) -> [u8; 4] {
    let harness = host.windows.get_mut(&window).expect("an open window");
    let image = harness.render();
    image.get_pixel(SHEET.0 / 2, 4).0
}

#[test]
fn a_sheet_window_opened_in_dark_paints_dark() {
    // The driver started in light; the app switches before it shows the sheet.
    let (mut runner, mut host) = setup(false, true);
    show(&mut runner, &mut host);
    let sheet = sheet_window(&runner, &host);
    let light = panel_pixel(&mut host, sheet);
    runner.send(&mut host, Msg::Close);
    deliver(&mut runner, &mut host);

    runner.send(&mut host, Msg::Dark);
    show(&mut runner, &mut host);
    let sheet = sheet_window(&runner, &host);
    let dark = panel_pixel(&mut host, sheet);
    assert!(dark[0] < light[0], "{light:?} → {dark:?}");
}

#[test]
fn a_sheet_window_follows_a_theme_switch_while_open() {
    let (mut runner, mut host) = setup(false, true);
    show(&mut runner, &mut host);
    let sheet = sheet_window(&runner, &host);
    let light = panel_pixel(&mut host, sheet);
    runner.send(&mut host, Msg::Dark);
    deliver(&mut runner, &mut host);
    let dark = panel_pixel(&mut host, sheet);
    assert!(dark[0] < light[0], "{light:?} → {dark:?}");
}
