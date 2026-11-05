//! The path control (2.24a) end to end on the fake host: clicking segments, the keyboard,
//! collapsing into an ellipsis, the popup style, changes from the app, and what assistive
//! technologies see.

use masonry_core::accesskit::Role;
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::NamedKey;
use masonry_core::kurbo::Point;

use super::menus::{focused, menus_open, named};
use super::*;
use crate::widgets::PathWidget;
use crate::{PathControl, PopUpButton};

const PATH: [&str; 4] = ["Macintosh HD", "Users", "sophie", "Documents"];

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Path(usize),
    Popup(usize),
    /// Script: the app shows another path.
    Shorten,
    /// Script: the app shows the three-level path of the gallery.
    ThreeSegments,
    /// Script: the app disables the control.
    Disable,
}

#[derive(Default)]
struct Paths {
    /// The window's width; 0 is the default, wide enough for the whole path.
    width: u32,
    path: Handle<PathControl<Msg>>,
    popup: Handle<PopUpButton<Msg>>,
    log: Vec<Msg>,
}

impl Application for Paths {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .align(crate::Align::Fill)
            .padding(20.0)
            .spacing(60.0)
            .with(
                PathControl::new(PATH)
                    .on_select(Msg::Path)
                    .bind(&mut self.path),
            )
            .with(PathControl::popup(PATH, Msg::Popup).bind(&mut self.popup));
        let width = if self.width == 0 { 600 } else { self.width };
        // Chrome spans the window, so the row gets the window's width.
        cx.open_window(Window::new("Path", (width, 300), root).chrome());
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Shorten => self.path.set_segments(cx, ["Macintosh HD", "Applications"]),
            Msg::ThreeSegments => self
                .path
                .set_segments(cx, ["Macintosh HD", "Users", "sophie"]),
            Msg::Disable => self.path.set_enabled(cx, false),
            _ => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Paths>, FakeHost) {
    setup_in(0)
}

fn setup_in(width: u32) -> (Runner<Paths>, FakeHost) {
    let (runner, mut host, _) = start(Paths {
        width,
        ..Paths::default()
    });
    let _ = host.only().render();
    (runner, host)
}

/// The drawn segments, with their centres in window space.
fn drawn(host: &mut FakeHost, widget: WidgetId) -> Vec<(Option<usize>, Point)> {
    let _ = host.only().render();
    let w = host.only().get_widget_with_id(widget);
    let ctx = w.ctx();
    w.downcast::<PathWidget>()
        .expect("a path control")
        .inner()
        .drawn()
        .into_iter()
        .map(|(i, r)| (i, ctx.to_window(r.center())))
        .collect()
}

fn centre_of(host: &mut FakeHost, widget: WidgetId, index: usize) -> Point {
    drawn(host, widget)
        .into_iter()
        .find(|(i, _)| *i == Some(index))
        .expect("segment is drawn")
        .1
}

fn click_at(runner: &mut Runner<Paths>, host: &mut FakeHost, at: Point) {
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn focus(host: &mut FakeHost, widget: WidgetId) {
    host.only().focus_on(Some(widget));
}

/// The path control's accessibility group: its role, value and the labels of its buttons.
fn a11y_path(host: &mut FakeHost, widget: WidgetId) -> (Option<String>, Vec<String>) {
    let harness = host.only();
    let _ = harness.render();
    let node = harness.access_node(widget).expect("a node");
    assert_eq!(node.role(), Role::Group);
    let buttons = node
        .children()
        .filter(|c| c.role() == Role::Button)
        .filter_map(|c| c.label())
        .collect();
    (node.value(), buttons)
}

#[test]
fn clicking_a_segment_sends_its_index() {
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    for i in [1, 0, 3] {
        let at = centre_of(&mut host, path, i);
        click_at(&mut runner, &mut host, at);
    }
    assert_eq!(runner.app().log, [Msg::Path(1), Msg::Path(0), Msg::Path(3)]);
}

#[test]
fn a_press_that_leaves_the_segment_sends_nothing() {
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    let on = centre_of(&mut host, path, 1);
    let off = centre_of(&mut host, path, 2);
    let harness = host.only();
    harness.mouse_move(on);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(off);
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert!(runner.app().log.is_empty());
}

#[test]
fn the_keyboard_moves_between_segments_and_chooses() {
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    focus(&mut host, path);
    // The current level (the last segment) has the focus to begin with.
    named(&mut runner, &mut host, NamedKey::Enter);
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    named(&mut runner, &mut host, NamedKey::Enter);
    // Left stops at the root and Right at the current level.
    for _ in 0..5 {
        named(&mut runner, &mut host, NamedKey::ArrowLeft);
    }
    named(&mut runner, &mut host, NamedKey::Enter);
    for _ in 0..5 {
        named(&mut runner, &mut host, NamedKey::ArrowRight);
    }
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(
        runner.app().log,
        [Msg::Path(3), Msg::Path(1), Msg::Path(0), Msg::Path(3)]
    );
}

#[test]
fn tabbing_in_focuses_the_last_segment_and_left_twice_reaches_the_first_of_three() {
    // Step 5.10 of the owner runbook: a three-segment path reached with Tab.
    for width in [0, 300, 260, 200, 150] {
        let (mut runner, mut host) = setup_in(width);
        let path = id(&runner.app().path);
        runner.send(&mut host, Msg::ThreeSegments);
        let _ = host.only().render();
        named(&mut runner, &mut host, NamedKey::Tab);
        assert_eq!(host.only().focused_widget_id(), Some(path));
        named(&mut runner, &mut host, NamedKey::ArrowLeft);
        named(&mut runner, &mut host, NamedKey::ArrowLeft);
        named(&mut runner, &mut host, NamedKey::Enter);
        assert_eq!(runner.app().log, [Msg::Path(0)], "width {width}");
    }
}

#[test]
fn tabbing_in_after_a_press_that_left_the_segment_still_starts_on_the_current_level() {
    // Runbook 5.9 then 5.10: press on "Users", move off, release; later Tab in.
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    let on = centre_of(&mut host, path, 1);
    let off = centre_of(&mut host, path, 2);
    let harness = host.only();
    harness.mouse_move(on);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_move(off);
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    // Leave the control, then Tab back in (the window has the popup button after it).
    host.only().focus_on(None);
    named(&mut runner, &mut host, NamedKey::Tab);
    assert_eq!(host.only().focused_widget_id(), Some(path));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Path(3)]);
}

#[test]
fn clicking_moves_the_keyboard_focus_to_that_segment() {
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    let at = centre_of(&mut host, path, 1);
    click_at(&mut runner, &mut host, at);
    assert_eq!(host.only().focused_widget_id(), Some(path));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Path(1), Msg::Path(2)]);
}

#[test]
fn a_disabled_control_ignores_clicks_and_keys() {
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    runner.send(&mut host, Msg::Disable);
    let at = centre_of(&mut host, path, 1);
    click_at(&mut runner, &mut host, at);
    assert!(runner.app().log.is_empty());
    assert_ne!(host.only().focused_widget_id(), Some(path));
}

#[test]
fn the_app_can_show_another_path_without_a_message() {
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    runner.send(&mut host, Msg::Shorten);
    assert_eq!(
        a11y_path(&mut host, path).1,
        ["Macintosh HD", "Applications"]
    );
    focus(&mut host, path);
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Path(1)]);
}

#[test]
fn assistive_technology_sees_a_group_of_buttons_and_can_activate_the_focus() {
    let (mut runner, mut host) = setup();
    let path = id(&runner.app().path);
    let (value, buttons) = a11y_path(&mut host, path);
    assert_eq!(
        value.as_deref(),
        Some("Macintosh HD / Users / sophie / Documents")
    );
    assert_eq!(buttons, PATH);
    a11y_click(host.only(), path);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Path(3)]);
}

#[test]
fn a_narrow_row_collapses_the_middle_into_an_ellipsis() {
    let (runner, mut host) = setup();
    let path = id(&runner.app().path);
    assert_eq!(drawn(&mut host, path).len(), 4);

    // A narrow window: the middle segments go, the first and the last stay.
    let (runner, mut host) = setup_in(260);
    let path = id(&runner.app().path);
    let squeezed = drawn(&mut host, path);
    let indices: Vec<Option<usize>> = squeezed.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices.first(), Some(&Some(0)));
    assert_eq!(indices.last(), Some(&Some(3)));
    assert!(indices.contains(&None), "an ellipsis: {indices:?}");
    assert!(indices.len() < 4, "{indices:?}");
    let (_, buttons) = a11y_path(&mut host, path);
    assert!(buttons.contains(&"Macintosh HD".to_owned()));
    assert!(buttons.contains(&"Documents".to_owned()));
    assert!(!buttons.contains(&"Users".to_owned()), "{buttons:?}");
}

#[test]
fn the_ellipsis_does_nothing_and_the_last_segment_still_works_when_collapsed() {
    let (mut runner, mut host) = setup_in(260);
    let path = id(&runner.app().path);
    let ellipsis = drawn(&mut host, path)
        .into_iter()
        .find(|(i, _)| i.is_none())
        .expect("an ellipsis")
        .1;
    click_at(&mut runner, &mut host, ellipsis);
    assert!(runner.app().log.is_empty());
    let last = centre_of(&mut host, path, 3);
    click_at(&mut runner, &mut host, last);
    assert_eq!(runner.app().log, [Msg::Path(3)]);
}

#[test]
fn the_popup_style_lists_the_current_level_first_and_sends_path_indices() {
    let (mut runner, mut host) = setup();
    let popup = id(&runner.app().popup);
    focus(&mut host, popup);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(focused(&mut host).as_deref(), Some("Documents"));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("sophie"));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("Macintosh HD"));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Popup(0)]);
}
