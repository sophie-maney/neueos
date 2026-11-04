//! The 2.21b tab view end to end on the fake host: only the selected tab's content is
//! shown, clicks and keys select, a disabled tab can't be selected, the handle, and the
//! accessibility nodes.

use masonry_core::accesskit::{Action, Role};
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::kurbo::Point;

use super::menus::press;
use super::table::click_with;
use super::*;
use crate::TabView;
use crate::widgets::TabViewWidget;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Selected(usize),
    Select(usize),
    Enable(usize, bool),
    Inside,
}

struct Tabs {
    tabs: Handle<TabView<Msg>>,
    inside: Handle<Button<Msg>>,
    log: Vec<Msg>,
}

impl Application for Tabs {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        // A, B, C (disabled) and D; a button in A to have focus inside a tab.
        let view = TabView::new()
            .with_tab(
                "General",
                Stack::column().with(Label::new("general")).with(
                    Button::new("Inside")
                        .on_press(Msg::Inside)
                        .bind(&mut self.inside),
                ),
            )
            .with_tab("Advanced", Label::new("advanced"))
            .with_disabled_tab("Hidden", Label::new("hidden"))
            .with_tab("About", Label::new("about"))
            .width(400.0)
            .height(300.0)
            .on_select(Msg::Selected)
            .bind(&mut self.tabs);
        cx.open_window(Window::new("Tabs", (400, 300), view));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Select(i) => self.tabs.set_selected(cx, i),
            Msg::Enable(i, on) => self.tabs.set_tab_enabled(cx, i, on),
            other => self.log.push(other),
        }
    }
}

fn setup() -> (Runner<Tabs>, FakeHost) {
    let (runner, mut host, _) = start(Tabs {
        tabs: Handle::default(),
        inside: Handle::default(),
        log: Vec::new(),
    });
    let _ = host.only().render();
    (runner, host)
}

fn tabs(runner: &Runner<Tabs>) -> WidgetId {
    id(&runner.app().tabs)
}

fn state(host: &mut FakeHost, view: WidgetId) -> (usize, Vec<bool>) {
    let _ = host.only().render();
    host.only()
        .get_widget_with_id(view)
        .downcast::<TabViewWidget>()
        .expect("a tab view")
        .inner()
        .state()
}

fn selected(host: &mut FakeHost, view: WidgetId) -> usize {
    state(host, view).0
}

/// The centre of tab `i`, in window coordinates.
fn tab(host: &mut FakeHost, view: WidgetId, i: usize) -> Point {
    let _ = host.only().render();
    let widget = host.only().get_widget_with_id(view);
    let rect = widget
        .downcast::<TabViewWidget>()
        .expect("a tab view")
        .inner()
        .tab_rects()[i];
    widget.ctx().to_window(rect.center())
}

fn key(runner: &mut Runner<Tabs>, host: &mut FakeHost, key: NamedKey, mods: Modifiers) {
    press(runner, host, Key::Named(key), mods);
    let _ = host.only().render();
}

fn take_log(runner: &mut Runner<Tabs>) -> Vec<Msg> {
    std::mem::take(&mut runner.app_mut().log)
}

/// Whether the content of tab `i` is shown.
fn shown(host: &mut FakeHost, view: WidgetId, i: usize) -> bool {
    let _ = host.only().render();
    let panel = host
        .only()
        .get_widget_with_id(view)
        .downcast::<TabViewWidget>()
        .expect("a tab view")
        .inner()
        .panel_ids()[i];
    !host.only().get_widget_with_id(panel).ctx().is_stashed()
}

#[test]
fn only_the_selected_tabs_content_is_shown() {
    let (runner, mut host) = setup();
    let v = tabs(&runner);
    assert_eq!(selected(&mut host, v), 0);
    assert_eq!(
        (0..4).map(|i| shown(&mut host, v, i)).collect::<Vec<_>>(),
        [true, false, false, false]
    );
}

#[test]
fn clicking_a_tab_selects_it_and_a_disabled_tab_does_nothing() {
    let (mut runner, mut host) = setup();
    let v = tabs(&runner);
    let at = tab(&mut host, v, 1);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    assert_eq!(selected(&mut host, v), 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(1)]);
    assert!(shown(&mut host, v, 1) && !shown(&mut host, v, 0));
    assert_eq!(
        host.only().focused_widget_id(),
        Some(v),
        "the strip takes focus"
    );
    // The disabled tab.
    let at = tab(&mut host, v, 2);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    assert_eq!(selected(&mut host, v), 1);
    // The selected tab again: no message.
    let at = tab(&mut host, v, 1);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    assert!(take_log(&mut runner).is_empty());
    // A press that moves to another tab before the release selects nothing.
    let from = tab(&mut host, v, 3);
    let to = tab(&mut host, v, 0);
    host.only().mouse_move(from);
    host.only().mouse_button_press(None);
    host.only().mouse_move(to);
    host.only().mouse_button_release(None);
    deliver(&mut runner, &mut host);
    assert_eq!(selected(&mut host, v), 1);
}

#[test]
fn arrow_keys_move_over_enabled_tabs_when_the_strip_has_focus() {
    let (mut runner, mut host) = setup();
    let v = tabs(&runner);
    host.only().focus_on(Some(v));
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowLeft,
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, v), 0, "no wrap");
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowRight,
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, v), 1);
    // The next one is disabled: the arrow skips it.
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowRight,
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, v), 3);
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowRight,
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, v), 3);
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowLeft,
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, v), 1, "back over the disabled one");
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowRight,
        Modifiers::empty(),
    );
    key(&mut runner, &mut host, NamedKey::Home, Modifiers::empty());
    assert_eq!(selected(&mut host, v), 0);
    key(&mut runner, &mut host, NamedKey::End, Modifiers::empty());
    assert_eq!(selected(&mut host, v), 3);
    deliver(&mut runner, &mut host);
    assert_eq!(
        take_log(&mut runner),
        [
            Msg::Selected(1),
            Msg::Selected(3),
            Msg::Selected(1),
            Msg::Selected(3),
            Msg::Selected(0),
            Msg::Selected(3)
        ]
    );
}

#[test]
fn ctrl_tab_cycles_from_inside_a_tab_and_moves_focus_to_the_strip() {
    let (mut runner, mut host) = setup();
    let v = tabs(&runner);
    let inside = id(&runner.app().inside);
    host.only().focus_on(Some(inside));
    // Arrows inside a tab are the content's, not the strip's.
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowRight,
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, v), 0);
    key(&mut runner, &mut host, NamedKey::Tab, Modifiers::CONTROL);
    assert_eq!(selected(&mut host, v), 1);
    assert_eq!(host.only().focused_widget_id(), Some(v));
    key(&mut runner, &mut host, NamedKey::Tab, Modifiers::CONTROL);
    assert_eq!(selected(&mut host, v), 3, "the disabled tab is skipped");
    key(&mut runner, &mut host, NamedKey::Tab, Modifiers::CONTROL);
    assert_eq!(selected(&mut host, v), 0, "wraps around");
    key(
        &mut runner,
        &mut host,
        NamedKey::Tab,
        Modifiers::CONTROL | Modifiers::SHIFT,
    );
    assert_eq!(selected(&mut host, v), 3);
    key(
        &mut runner,
        &mut host,
        NamedKey::Tab,
        Modifiers::CONTROL | Modifiers::SHIFT,
    );
    assert_eq!(selected(&mut host, v), 1);
}

#[test]
fn the_handle_selects_and_enables_without_sending_messages() {
    let (mut runner, mut host) = setup();
    let v = tabs(&runner);
    runner.send(&mut host, Msg::Select(3));
    assert_eq!(selected(&mut host, v), 3);
    assert!(shown(&mut host, v, 3) && !shown(&mut host, v, 0));
    // Out of range: the last tab. A disabled tab can be selected by the app.
    runner.send(&mut host, Msg::Select(9));
    assert_eq!(selected(&mut host, v), 3);
    runner.send(&mut host, Msg::Select(2));
    assert_eq!(selected(&mut host, v), 2);
    runner.send(&mut host, Msg::Enable(2, true));
    runner.send(&mut host, Msg::Enable(3, false));
    runner.send(&mut host, Msg::Enable(9, false));
    assert_eq!(state(&mut host, v), (2, vec![true, true, true, false]));
    assert!(take_log(&mut runner).is_empty());
    // Now the keys skip the new disabled tab and stop at the enabled third.
    host.only().focus_on(Some(v));
    key(
        &mut runner,
        &mut host,
        NamedKey::ArrowRight,
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, v), 2);
}

#[test]
fn assistive_technology_sees_a_tab_list_tabs_and_a_panel() {
    let (mut runner, mut host) = setup();
    let v = tabs(&runner);
    let _ = host.only().render();
    let state = host.only().access_tree().state();
    let mut tabs_found = Vec::new();
    let mut lists = 0;
    let mut panels = Vec::new();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        match node.role() {
            Role::TabList => lists += 1,
            Role::Tab => tabs_found.push((
                node.label().unwrap_or_default().to_string(),
                node.is_selected(),
                node.is_disabled(),
            )),
            Role::TabPanel => panels.push(node.label().unwrap_or_default().to_string()),
            _ => {}
        }
        stack.extend(node.children());
    }
    tabs_found.sort();
    assert_eq!(lists, 1);
    assert_eq!(
        tabs_found,
        [
            ("About".to_owned(), Some(false), false),
            ("Advanced".to_owned(), Some(false), false),
            ("General".to_owned(), Some(true), false),
            ("Hidden".to_owned(), Some(false), true),
        ]
    );
    assert_eq!(panels, ["General"], "only the shown tab has a panel");
    let node = host.only().access_node(v).expect("a node");
    assert_eq!(node.value().as_deref(), Some("General"));
    a11y_action(host.only(), v, Action::Increment);
    deliver(&mut runner, &mut host);
    assert_eq!(selected(&mut host, v), 1);
    a11y_action(host.only(), v, Action::Decrement);
    deliver(&mut runner, &mut host);
    assert_eq!(selected(&mut host, v), 0);
}
