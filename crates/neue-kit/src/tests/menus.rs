//! Context menus (2.17a) end to end on the fake host: opening, choosing by pointer, by
//! press-drag-release, by keyboard and by assistive technology; submenus; type-select;
//! closing; focus coming back; and what assistive technologies see.

use masonry_core::accesskit::{Action, ActionRequest, HasPopup, NodeId, Role, Toggled, TreeId};
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};
use masonry_core::kurbo::{Point, Rect};

use super::*;
use crate::{ContextMenu, Menu, MenuItem, Shortcut, TextField};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Cut,
    Copy,
    Paste,
    Wrap,
    Size(&'static str),
    Delete,
    Other,
    /// Script: the app replaces the menu.
    Swap,
}

#[derive(Default)]
struct Menus {
    area: Handle<ContextMenu<Msg>>,
    field: Handle<TextField<Msg>>,
    log: Vec<Msg>,
}

/// Rows: Cut, Copy, Paste (disabled), separator, Wrap (checked), Size ▸ (Small, Large),
/// Delete.
fn menu() -> Menu<Msg> {
    Menu::new()
        .with(
            MenuItem::new("Cut")
                .shortcut(Shortcut::command("X"))
                .on_choose(Msg::Cut),
        )
        .with(
            MenuItem::new("Copy")
                .shortcut(Shortcut::command("C"))
                .on_choose(Msg::Copy),
        )
        .with(MenuItem::new("Paste").disabled(true).on_choose(Msg::Paste))
        .separator()
        .with(MenuItem::new("Wrap").checked(true).on_choose(Msg::Wrap))
        .with(MenuItem::submenu(
            "Size",
            Menu::new()
                .with(
                    MenuItem::new("Small")
                        .radio(true)
                        .on_choose(Msg::Size("small")),
                )
                .with(
                    MenuItem::new("Large")
                        .radio(false)
                        .on_choose(Msg::Size("large")),
                ),
        ))
        .with(MenuItem::new("Delete").on_choose(Msg::Delete))
}

impl Application for Menus {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column().with(
            ContextMenu::new(TextField::new().bind(&mut self.field), menu()).bind(&mut self.area),
        );
        cx.open_window(Window::new("Menus", (400, 400), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        if message == Msg::Swap {
            self.area.set_menu(
                cx,
                Menu::new().with(MenuItem::new("Other").on_choose(Msg::Other)),
            );
            return;
        }
        self.log.push(message);
    }
}

fn setup() -> (Runner<Menus>, FakeHost) {
    let (runner, mut host, _) = start(Menus::default());
    let _ = host.only().render();
    (runner, host)
}

/// A node of the accessibility tree: its widget's id and bounds.
#[derive(Debug, Clone, Copy)]
struct Found {
    node: NodeId,
    bounds: Rect,
}

/// Every node with `role` (and `label`, if given), in tree order.
fn find(host: &mut FakeHost, role: Role, label: Option<&str>) -> Vec<Found> {
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let mut found = Vec::new();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.role() == role && label.is_none_or(|l| node.label().as_deref() == Some(l)) {
            let b = node.bounding_box().unwrap_or_default();
            found.push(Found {
                node: node.locate().0,
                bounds: Rect::new(b.x0, b.y0, b.x1, b.y1),
            });
        }
        let mut children: Vec<_> = node.children().collect();
        children.reverse();
        stack.extend(children);
    }
    found
}

fn menus_open(host: &mut FakeHost) -> usize {
    find(host, Role::Menu, None).len()
}

/// The menu row labelled `label` (any item role).
fn item(host: &mut FakeHost, label: &str) -> Found {
    [Role::MenuItem, Role::MenuItemCheckBox, Role::MenuItemRadio]
        .into_iter()
        .find_map(|role| find(host, role, Some(label)).into_iter().next())
        .unwrap_or_else(|| panic!("no menu item {label:?}"))
}

/// The label of the node with keyboard focus.
fn focused(host: &mut FakeHost) -> Option<String> {
    let harness = host.only();
    let _ = harness.render();
    harness.access_tree().state().focus_in_tree().label()
}

fn area_center(runner: &Runner<Menus>, host: &mut FakeHost) -> Point {
    let harness = host.only();
    let widget = harness.get_widget_with_id(id(&runner.app().area));
    widget.ctx().window_transform() * widget.ctx().border_box().center()
}

/// A secondary click (press and release in place) on the area.
fn open(runner: &mut Runner<Menus>, host: &mut FakeHost) {
    let at = area_center(runner, host);
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Secondary));
    harness.mouse_button_release(Some(PointerButton::Secondary));
    deliver(runner, host);
}

fn hover(runner: &mut Runner<Menus>, host: &mut FakeHost, label: &str) {
    let at = item(host, label).bounds.center();
    host.only().mouse_move(at);
    deliver(runner, host);
}

fn click_item(runner: &mut Runner<Menus>, host: &mut FakeHost, label: &str) {
    hover(runner, host, label);
    let harness = host.only();
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn press(runner: &mut Runner<Menus>, host: &mut FakeHost, key: Key, modifiers: Modifiers) {
    for state in [KeyState::Down, KeyState::Up] {
        host.only()
            .process_text_event(TextEvent::Keyboard(KeyboardEvent {
                state,
                key: key.clone(),
                modifiers,
                ..KeyboardEvent::default()
            }));
    }
    deliver(runner, host);
}

fn named(runner: &mut Runner<Menus>, host: &mut FakeHost, key: NamedKey) {
    press(runner, host, Key::Named(key), Modifiers::empty());
}

fn typed(runner: &mut Runner<Menus>, host: &mut FakeHost, text: &str) {
    for c in text.chars() {
        press(
            runner,
            host,
            Key::Character(c.to_string()),
            Modifiers::empty(),
        );
    }
}

fn a11y(host: &mut FakeHost, node: NodeId, action: Action) {
    host.only().process_access_event(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: node,
        data: None,
    });
}

#[test]
fn a_secondary_click_opens_the_menu_and_a_click_chooses() {
    let (mut runner, mut host) = setup();
    assert_eq!(menus_open(&mut host), 0);
    open(&mut runner, &mut host);
    // Released in place: the menu stays up, nothing is highlighted.
    assert_eq!(menus_open(&mut host), 1);
    assert!(runner.app().log.is_empty());

    click_item(&mut runner, &mut host, "Copy");
    assert_eq!(runner.app().log, [Msg::Copy]);
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn press_drag_release_chooses() {
    let (mut runner, mut host) = setup();
    let at = area_center(&runner, &mut host);
    host.only().mouse_move(at);
    host.only()
        .mouse_button_press(Some(PointerButton::Secondary));
    deliver(&mut runner, &mut host);
    hover(&mut runner, &mut host, "Cut");
    host.only()
        .mouse_button_release(Some(PointerButton::Secondary));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Cut]);
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn disabled_items_and_separators_do_nothing() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    click_item(&mut runner, &mut host, "Paste");
    assert!(runner.app().log.is_empty());
    assert_eq!(menus_open(&mut host), 1);
}

#[test]
fn a_click_outside_closes_without_a_message() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    let harness = host.only();
    harness.mouse_move(Point::new(390.0, 390.0));
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 0);
    assert!(runner.app().log.is_empty());
}

#[test]
fn arrow_keys_skip_separators_and_disabled_items() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("Cut"));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("Wrap"));
    named(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(focused(&mut host).as_deref(), Some("Copy"));
    named(&mut runner, &mut host, NamedKey::End);
    assert_eq!(focused(&mut host).as_deref(), Some("Delete"));
    // No wrapping around.
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("Delete"));
    named(&mut runner, &mut host, NamedKey::Home);
    assert_eq!(focused(&mut host).as_deref(), Some("Cut"));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Cut]);
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn typing_selects_the_first_item_with_that_prefix() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    typed(&mut runner, &mut host, "c");
    assert_eq!(focused(&mut host).as_deref(), Some("Cut"));
    typed(&mut runner, &mut host, "o");
    assert_eq!(focused(&mut host).as_deref(), Some("Copy"));
    // "cop" + "z" matches nothing: the highlight stays.
    typed(&mut runner, &mut host, "z");
    assert_eq!(focused(&mut host).as_deref(), Some("Copy"));
    // Disabled items are skipped: "p" alone would be Paste.
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Copy]);
}

#[test]
fn submenus_open_with_right_and_close_with_left() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    typed(&mut runner, &mut host, "s");
    assert_eq!(focused(&mut host).as_deref(), Some("Size"));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(menus_open(&mut host), 2);
    assert_eq!(focused(&mut host).as_deref(), Some("Small"));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("Large"));
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(focused(&mut host).as_deref(), Some("Size"));
    // Return on a submenu item opens it too; then choose inside it.
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(menus_open(&mut host), 2);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Size("large")]);
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn hovering_opens_a_submenu_and_a_click_inside_chooses() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    hover(&mut runner, &mut host, "Size");
    assert_eq!(menus_open(&mut host), 2);
    // Moving to another item closes it.
    hover(&mut runner, &mut host, "Wrap");
    assert_eq!(menus_open(&mut host), 1);
    hover(&mut runner, &mut host, "Size");
    assert_eq!(menus_open(&mut host), 2);
    // A press inside the submenu doesn't count as outside.
    click_item(&mut runner, &mut host, "Large");
    assert_eq!(runner.app().log, [Msg::Size("large")]);
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn escape_closes_every_menu_and_gives_focus_back() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    host.only().focus_on(Some(field));
    open(&mut runner, &mut host);
    typed(&mut runner, &mut host, "s");
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(menus_open(&mut host), 2);
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
    assert!(runner.app().log.is_empty());
    assert_eq!(host.only().focused_widget_id(), Some(field));
}

#[test]
fn choosing_gives_focus_back_too() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    host.only().focus_on(Some(field));
    open(&mut runner, &mut host);
    click_item(&mut runner, &mut host, "Delete");
    assert_eq!(runner.app().log, [Msg::Delete]);
    assert_eq!(host.only().focused_widget_id(), Some(field));
}

#[test]
fn the_menu_key_opens_with_the_first_item_highlighted() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    host.only().focus_on(Some(field));
    named(&mut runner, &mut host, NamedKey::ContextMenu);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(focused(&mut host).as_deref(), Some("Cut"));
    named(&mut runner, &mut host, NamedKey::Escape);
    // Shift-F10 does the same.
    press(
        &mut runner,
        &mut host,
        Key::Named(NamedKey::F10),
        Modifiers::SHIFT,
    );
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(focused(&mut host).as_deref(), Some("Cut"));
}

#[test]
fn reopening_while_open_keeps_one_menu_and_the_focus_to_return_to() {
    let (mut runner, mut host) = setup();
    let field = id(&runner.app().field);
    host.only().focus_on(Some(field));
    open(&mut runner, &mut host);
    open(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(host.only().focused_widget_id(), Some(field));
}

#[test]
fn assistive_technologies_open_menus_and_choose_items() {
    let (mut runner, mut host) = setup();
    let area = NodeId(id(&runner.app().area).to_raw());
    a11y(&mut host, area, Action::ShowContextMenu);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    let copy = item(&mut host, "Copy").node;
    a11y(&mut host, copy, Action::Click);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Copy]);
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn assistive_technologies_see_roles_states_and_shortcuts() {
    let (mut runner, mut host) = setup();
    open(&mut runner, &mut host);
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let by_label = |label: &str| {
        let mut stack = vec![state.root()];
        while let Some(node) = stack.pop() {
            if node.label().as_deref() == Some(label) {
                return node;
            }
            stack.extend(node.children());
        }
        panic!("no node {label:?}");
    };
    let cut = by_label("Cut");
    assert_eq!(cut.role(), Role::MenuItem);
    let spoken = if cfg!(target_os = "macos") {
        "Cmd+X"
    } else {
        "Ctrl+X"
    };
    assert_eq!(cut.data().keyboard_shortcut(), Some(spoken));
    assert!(cut.data().supports_action(Action::Click));
    assert!(by_label("Paste").is_disabled());
    let wrap = by_label("Wrap");
    assert_eq!(wrap.role(), Role::MenuItemCheckBox);
    assert_eq!(wrap.toggled(), Some(Toggled::True));
    assert_eq!(by_label("Size").has_popup(), Some(HasPopup::Menu));
}

#[test]
fn the_app_can_replace_the_menu() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::Swap);
    open(&mut runner, &mut host);
    assert!(find(&mut host, Role::MenuItem, Some("Cut")).is_empty());
    click_item(&mut runner, &mut host, "Other");
    assert_eq!(runner.app().log, [Msg::Other]);
}
