//! Pop-up buttons, pull-down buttons and combo boxes (2.17b) end to end on the fake host:
//! where their menus open, choosing by click, by press-drag-release and by keyboard,
//! type-select while closed, closing, focus coming back, and what assistive technologies see.

use masonry_core::accesskit::{Action, HasPopup, NodeId, Role};
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::NamedKey;
use masonry_core::kurbo::Point;

use super::menus::{a11y, click_item, find, focused, hover, item, menus_open, named, typed};
use super::*;
use crate::{ComboBox, Menu, MenuItem, PopUpButton, PullDownButton};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Format(usize),
    Command(&'static str),
    Picked(usize, String),
    Typed(String),
    /// Script: the app reads the combo box's text.
    Read,
}

#[derive(Default)]
struct Popups {
    popup: Handle<PopUpButton<Msg>>,
    pull: Handle<PullDownButton<Msg>>,
    combo: Handle<ComboBox<Msg>>,
    read: Option<String>,
    log: Vec<Msg>,
}

impl Application for Popups {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column()
            .padding(20.0)
            .spacing(60.0)
            .with(
                PopUpButton::new(["PNG", "JPEG", "TIFF"])
                    .selected(1)
                    .on_select(Msg::Format)
                    .bind(&mut self.popup),
            )
            .with(
                PullDownButton::new(
                    "Actions",
                    Menu::new()
                        .with(MenuItem::new("Rename").on_choose(Msg::Command("rename")))
                        .separator()
                        .with(MenuItem::new("Delete").on_choose(Msg::Command("delete"))),
                )
                .bind(&mut self.pull),
            )
            .with(
                ComboBox::new(["Small", "Medium", "Large"])
                    .on_change(Msg::Typed)
                    .on_select(Msg::Picked)
                    .bind(&mut self.combo),
            );
        cx.open_window(Window::new("Popups", (400, 400), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        if message == Msg::Read {
            self.read = self.combo.text(cx);
            return;
        }
        self.log.push(message);
    }
}

fn setup() -> (Runner<Popups>, FakeHost) {
    let (runner, mut host, _) = start(Popups::default());
    let _ = host.only().render();
    (runner, host)
}

/// The window-space rectangle of the widget `id`.
fn bounds_of(host: &mut FakeHost, id: WidgetId) -> masonry_core::kurbo::Rect {
    let widget = host.only().get_widget_with_id(id);
    widget
        .ctx()
        .window_transform()
        .transform_rect_bbox(widget.ctx().border_box())
}

fn click_at(runner: &mut Runner<Popups>, host: &mut FakeHost, at: Point) {
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn click_widget(runner: &mut Runner<Popups>, host: &mut FakeHost, id: WidgetId) {
    let at = bounds_of(host, id).center();
    click_at(runner, host, at);
}

/// The value and expanded state of the first pop-up (a combo box role with a menu).
fn popup_state(host: &mut FakeHost) -> Option<(Option<String>, Option<bool>)> {
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.role() == Role::ComboBox && node.has_popup() == Some(HasPopup::Menu) {
            return Some((
                node.value().map(|v| v.to_owned()),
                node.data().is_expanded(),
            ));
        }
        let mut children: Vec<_> = node.children().collect();
        children.reverse();
        stack.extend(children);
    }
    None
}

fn popup_value(host: &mut FakeHost) -> Option<String> {
    popup_state(host)?.0
}

fn focus(host: &mut FakeHost, id: WidgetId) {
    host.only().focus_on(Some(id));
}

#[test]
fn a_pop_up_button_opens_its_menu_with_the_chosen_item_over_it() {
    let (mut runner, mut host) = setup();
    let button = bounds_of(&mut host, id(&runner.app().popup));
    {
        let id = id(&runner.app().popup);
        click_widget(&mut runner, &mut host, id);
    }
    assert_eq!(menus_open(&mut host), 1);
    let chosen = item(&mut host, "JPEG");
    assert!((chosen.bounds.center().y - button.center().y).abs() < 1.5);
    let menu = find(&mut host, Role::Menu, None)[0].bounds;
    assert!(
        menu.width() >= button.width() - 1.0,
        "as wide as the button"
    );
    assert!((menu.x0 - button.x0).abs() < 1.5);
    // The chosen item carries the check mark, the highlight is on it.
    assert_eq!(
        find(&mut host, Role::MenuItemCheckBox, Some("JPEG")).len(),
        1
    );
    assert_eq!(focused(&mut host).as_deref(), Some("JPEG"));
    assert!(runner.app().log.is_empty());
}

#[test]
fn choosing_an_item_sends_its_index_and_the_button_shows_it() {
    let (mut runner, mut host) = setup();
    assert_eq!(popup_value(&mut host).as_deref(), Some("JPEG"));
    {
        let id = id(&runner.app().popup);
        click_widget(&mut runner, &mut host, id);
    }
    click_item(&mut runner, &mut host, "TIFF");
    assert_eq!(runner.app().log, [Msg::Format(2)]);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(popup_value(&mut host).as_deref(), Some("TIFF"));
    // The menu now opens with TIFF over the button.
    let button = bounds_of(&mut host, id(&runner.app().popup));
    {
        let id = id(&runner.app().popup);
        click_widget(&mut runner, &mut host, id);
    }
    assert!((item(&mut host, "TIFF").bounds.center().y - button.center().y).abs() < 1.5);
}

#[test]
fn press_drag_release_chooses() {
    let (mut runner, mut host) = setup();
    let at = bounds_of(&mut host, id(&runner.app().popup)).center();
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    hover(&mut runner, &mut host, "PNG");
    host.only()
        .mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Format(0)]);
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn pressing_the_button_again_closes_its_menu() {
    // A pull-down's menu is below it, so the button stays pressable.
    let (mut runner, mut host) = setup();
    let button = id(&runner.app().pull);
    click_widget(&mut runner, &mut host, button);
    assert_eq!(menus_open(&mut host), 1);
    click_widget(&mut runner, &mut host, button);
    assert_eq!(menus_open(&mut host), 0);
    assert!(runner.app().log.is_empty());
    click_widget(&mut runner, &mut host, button);
    assert_eq!(menus_open(&mut host), 1);
}

#[test]
fn a_click_outside_closes_without_a_message() {
    let (mut runner, mut host) = setup();
    {
        let id = id(&runner.app().popup);
        click_widget(&mut runner, &mut host, id);
    }
    click_at(&mut runner, &mut host, Point::new(390.0, 390.0));
    assert_eq!(menus_open(&mut host), 0);
    assert!(runner.app().log.is_empty());
}

#[test]
fn the_keyboard_opens_and_chooses_and_gives_focus_back() {
    for key in [NamedKey::ArrowDown, NamedKey::Enter] {
        let (mut runner, mut host) = setup();
        let button = id(&runner.app().popup);
        focus(&mut host, button);
        named(&mut runner, &mut host, key);
        assert_eq!(menus_open(&mut host), 1);
        assert_eq!(focused(&mut host).as_deref(), Some("JPEG"));
        named(&mut runner, &mut host, NamedKey::ArrowDown);
        assert_eq!(focused(&mut host).as_deref(), Some("TIFF"));
        named(&mut runner, &mut host, NamedKey::Enter);
        assert_eq!(runner.app().log, [Msg::Format(2)]);
        assert_eq!(menus_open(&mut host), 0);
        assert_eq!(host.only().focused_widget_id(), Some(button));
    }
}

#[test]
fn space_opens_and_escape_closes() {
    let (mut runner, mut host) = setup();
    let button = id(&runner.app().popup);
    focus(&mut host, button);
    typed(&mut runner, &mut host, " ");
    assert_eq!(menus_open(&mut host), 1);
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
    assert!(runner.app().log.is_empty());
    assert_eq!(host.only().focused_widget_id(), Some(button));
}

#[test]
fn typing_chooses_while_the_menu_is_closed() {
    let (mut runner, mut host) = setup();
    focus(&mut host, id(&runner.app().popup));
    typed(&mut runner, &mut host, "t");
    assert_eq!(runner.app().log, [Msg::Format(2)]);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(popup_value(&mut host).as_deref(), Some("TIFF"));
    // A second letter within the reset time extends the prefix: "tz" matches nothing.
    typed(&mut runner, &mut host, "z");
    assert_eq!(runner.app().log.len(), 1);
}

#[test]
fn a_disabled_pop_up_ignores_presses() {
    let (mut runner, mut host) = setup();
    host.only()
        .edit_widget_with_id(id(&runner.app().popup), |mut w| w.ctx.set_disabled(true));
    {
        let id = id(&runner.app().popup);
        click_widget(&mut runner, &mut host, id);
    }
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn a_pull_down_button_opens_a_menu_below_itself_and_keeps_its_title() {
    let (mut runner, mut host) = setup();
    let button = bounds_of(&mut host, id(&runner.app().pull));
    {
        let id = id(&runner.app().pull);
        click_widget(&mut runner, &mut host, id);
    }
    assert_eq!(menus_open(&mut host), 1);
    let menu = find(&mut host, Role::Menu, None)[0].bounds;
    assert!((menu.y0 - button.y1).abs() < 1.5, "right below the button");
    assert!(menu.width() >= button.width() - 1.0);
    // Nothing is highlighted until the pointer or a key moves.
    click_item(&mut runner, &mut host, "Delete");
    assert_eq!(runner.app().log, [Msg::Command("delete")]);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(find(&mut host, Role::Button, Some("Actions")).len(), 1);
}

#[test]
fn a_pull_down_opens_from_the_keyboard_on_its_first_item() {
    let (mut runner, mut host) = setup();
    let button = id(&runner.app().pull);
    focus(&mut host, button);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("Rename"));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::Command("rename")]);
    assert_eq!(host.only().focused_widget_id(), Some(button));
}

/// The middle of a combo box's arrow button.
fn arrow_of(host: &mut FakeHost, id: WidgetId) -> Point {
    let rect = bounds_of(host, id);
    Point::new(
        rect.x1 - 2.0 - neue_theme::metrics::control::COMBO_ARROW_WIDTH / 2.0,
        rect.center().y,
    )
}

#[test]
fn the_arrow_opens_a_menu_as_wide_as_the_field_and_choosing_fills_it_in() {
    let (mut runner, mut host) = setup();
    let combo = id(&runner.app().combo);
    let field = bounds_of(&mut host, combo);
    let at = arrow_of(&mut host, combo);
    click_at(&mut runner, &mut host, at);
    assert_eq!(menus_open(&mut host), 1);
    let menu = find(&mut host, Role::Menu, None)[0].bounds;
    assert!((menu.y0 - field.y1).abs() < 1.5);
    assert!(menu.width() >= field.width() - 1.0);
    click_item(&mut runner, &mut host, "Medium");
    assert_eq!(runner.app().log, [Msg::Picked(1, "Medium".into())]);
    runner.send(&mut host, Msg::Read);
    assert_eq!(runner.app().read.as_deref(), Some("Medium"));
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn a_press_in_the_text_part_edits_and_down_opens_the_menu() {
    let (mut runner, mut host) = setup();
    let combo = id(&runner.app().combo);
    let rect = bounds_of(&mut host, combo);
    click_at(
        &mut runner,
        &mut host,
        Point::new(rect.x0 + 20.0, rect.center().y),
    );
    assert_eq!(menus_open(&mut host), 0);
    typed(&mut runner, &mut host, "x");
    assert_eq!(runner.app().log, [Msg::Typed("x".into())]);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(focused(&mut host).as_deref(), Some("Small"));
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(host.only().focused_widget_id(), Some(combo));
}

#[test]
fn assistive_technologies_open_and_see_the_state() {
    let (mut runner, mut host) = setup();
    let popup = NodeId(id(&runner.app().popup).to_raw());
    a11y(&mut host, popup, Action::Click);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(
        popup_state(&mut host),
        Some((Some("JPEG".into()), Some(true)))
    );
    let tiff = item(&mut host, "TIFF").node;
    a11y(&mut host, tiff, Action::Click);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Format(2)]);
    assert_eq!(menus_open(&mut host), 0);
}
