//! The edit context menu of text fields and text views (2.17b): opened by a secondary
//! press, the Menu key or an assistive technology; Cut, Copy and Select All act on the
//! widget; a context menu around the widget takes precedence.

use masonry_core::accesskit::{Action, NodeId, Role};
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::kurbo::Point;

use super::menus::{a11y, click_item, find, focused, menus_open, named, press};
use super::*;
use crate::{ContextMenu, Menu, MenuItem, TextField, TextView};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Field(String),
    View(String),
    Wrapped,
}

#[derive(Default)]
struct Editors {
    field: Handle<TextField<Msg>>,
    view: Handle<TextView<Msg>>,
    wrapped: Handle<TextField<Msg>>,
    log: Vec<Msg>,
}

impl Application for Editors {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .padding(10.0)
            .spacing(30.0)
            .with(
                TextField::new()
                    .text("hello world")
                    .on_change(Msg::Field)
                    .bind(&mut self.field),
            )
            .with(
                TextView::new()
                    .text("one\ntwo")
                    .height(60.0)
                    .on_change(Msg::View)
                    .bind(&mut self.view),
            )
            .with(ContextMenu::new(
                TextField::new().bind(&mut self.wrapped),
                Menu::new().with(MenuItem::new("Mine").on_choose(Msg::Wrapped)),
            ));
        cx.open_window(Window::new("Editors", (400, 400), root));
    }

    fn update(&mut self, _cx: &mut Cx<'_, Msg>, message: Msg) {
        self.log.push(message);
    }
}

fn setup() -> (Runner<Editors>, FakeHost) {
    let (runner, mut host, _) = start(Editors::default());
    let _ = host.only().render();
    (runner, host)
}

fn center(host: &mut FakeHost, id: WidgetId) -> Point {
    let widget = host.only().get_widget_with_id(id);
    widget.ctx().window_transform() * widget.ctx().border_box().center()
}

/// A default edit menu opens after the event that asked for it (so that a context menu
/// around the widget gets the event first), and its rows are in the tree one pass later.
fn settle(host: &mut FakeHost) {
    for _ in 0..2 {
        let _ = host.only().render();
    }
}

fn right_click(runner: &mut Runner<Editors>, host: &mut FakeHost, id: WidgetId) {
    let at = center(host, id);
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Secondary));
    harness.mouse_button_release(Some(PointerButton::Secondary));
    settle(host);
    deliver(runner, host);
}

/// The command key: Super (⌘) on every host (ADR 0080).
fn command() -> Modifiers {
    Modifiers::META
}

fn disabled(host: &mut FakeHost, label: &str) -> bool {
    let _ = host.only().render();
    let state = host.only().access_tree().state();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.label().as_deref() == Some(label) && node.role() == Role::MenuItem {
            return node.is_disabled();
        }
        stack.extend(node.children());
    }
    panic!("no item {label:?}");
}

#[test]
fn a_secondary_press_on_a_field_opens_the_edit_menu() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    right_click(&mut runner, &mut host, field);
    assert_eq!(menus_open(&mut host), 1);
    // Nothing is selected: only Select All works.
    assert!(disabled(&mut host, "Cut"));
    assert!(disabled(&mut host, "Copy"));
    assert!(!disabled(&mut host, "Select All"));
    // The field had no focus: it takes it, and gets it back when the menu closes.
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(host.only().focused_widget_id(), Some(field));
}

#[test]
fn select_all_then_copy_and_cut_act_on_the_field() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    right_click(&mut runner, &mut host, field);
    click_item(&mut runner, &mut host, "Select All");
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(host.only().focused_widget_id(), Some(field));

    // The selection survived the menu: Copy is enabled and copies it.
    right_click(&mut runner, &mut host, field);
    assert!(!disabled(&mut host, "Copy"));
    click_item(&mut runner, &mut host, "Copy");
    assert_eq!(host.only().clipboard_contents(), "hello world");
    assert!(runner.app().log.is_empty());

    right_click(&mut runner, &mut host, field);
    click_item(&mut runner, &mut host, "Cut");
    assert_eq!(runner.app().log, [Msg::Field(String::new())]);
    assert_eq!(host.only().clipboard_contents(), "hello world");
}

#[test]
fn the_menu_key_opens_it_with_the_first_item_highlighted() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    host.only().focus_on(Some(field));
    named(&mut runner, &mut host, NamedKey::ContextMenu);
    settle(&mut host);
    assert_eq!(menus_open(&mut host), 1);
    // Focusing it selected its text, so Cut works: the first item.
    assert_eq!(focused(&mut host).as_deref(), Some("Cut"));
    named(&mut runner, &mut host, NamedKey::Escape);
    press(
        &mut runner,
        &mut host,
        Key::Named(NamedKey::F10),
        Modifiers::SHIFT,
    );
    settle(&mut host);
    assert_eq!(menus_open(&mut host), 1);
}

#[test]
fn assistive_technologies_can_open_it() {
    let (mut runner, mut host) = setup();
    let field = NodeId(id(&runner.app().field).to_raw());
    a11y(&mut host, field, Action::ShowContextMenu);
    deliver(&mut runner, &mut host);
    settle(&mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert!(!find(&mut host, Role::MenuItem, Some("Select All")).is_empty());
}

#[test]
fn a_context_menu_around_a_field_takes_precedence() {
    let (mut runner, mut host) = setup();
    let wrapped = id(&runner.app().wrapped);
    right_click(&mut runner, &mut host, wrapped);
    assert_eq!(menus_open(&mut host), 1);
    assert!(find(&mut host, Role::MenuItem, Some("Cut")).is_empty());
    click_item(&mut runner, &mut host, "Mine");
    assert_eq!(runner.app().log, [Msg::Wrapped]);
    // The same with the Menu key.
    host.only().focus_on(Some(wrapped));
    named(&mut runner, &mut host, NamedKey::ContextMenu);
    assert_eq!(menus_open(&mut host), 1);
    assert!(find(&mut host, Role::MenuItem, Some("Cut")).is_empty());
}

#[test]
fn a_text_view_has_the_menu_too_and_a_read_only_one_cannot_cut() {
    let (mut runner, mut host) = setup();
    let view = id(&runner.app().view);
    host.only().focus_on(Some(view));
    // Select everything with the keyboard, then open the menu.
    press(
        &mut runner,
        &mut host,
        Key::Character("a".into()),
        command(),
    );
    right_click(&mut runner, &mut host, view);
    assert_eq!(menus_open(&mut host), 1);
    assert!(!disabled(&mut host, "Cut"));
    click_item(&mut runner, &mut host, "Cut");
    assert_eq!(host.only().clipboard_contents(), "one\ntwo");
    assert_eq!(runner.app().log, [Msg::View(String::new())]);
}
