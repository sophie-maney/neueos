//! The shell's menu bar (3.2b) on the fake host: titles on the left, extras on the right,
//! a title's menu opening below it, choosing, the keyboard, and changing a title; the
//! active app's menus being replaced, and their open and close messages (3.7a); moving
//! between menus by pointing and with Left and Right, and opening the first menu from the
//! keyboard (3.7b).

use masonry_core::accesskit::{Action, Role};
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::NamedKey;
use masonry_core::kurbo::Rect;
use neue_theme::materials::menu_bar::{EDGE_INSET, HEIGHT};

use super::menus::{a11y, click_item, find, focused, menus_open, named};
use super::*;
use crate::desktop::{Anchor, LayerSpec, MenuBar, MenuTitle};
use crate::{Menu, MenuItem};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    About,
    Quit,
    /// Script: the app renames its title.
    Rename(&'static str),
    /// Script: the app's menus become these titles, each with one item named after it.
    SetMenus(Vec<&'static str>),
    /// Script: the shell was asked to focus the menu bar (Ctrl+F2).
    FocusBar,
    Opened(&'static str),
    Closed(&'static str),
    Item(&'static str),
}

/// An app menu titled `name`, with an item `name item`, reporting opening and closing.
fn app_menu(name: &'static str) -> MenuTitle<Msg> {
    let item: &'static str = Box::leak(format!("{name} item").into_boxed_str());
    MenuTitle::new(
        name,
        Menu::new().with(MenuItem::new(item).on_choose(Msg::Item(item))),
    )
    .on_open(Msg::Opened(name))
    .on_close(Msg::Closed(name))
}

const WIDTH: u32 = 500;

#[derive(Default)]
struct Bar {
    bar: Handle<MenuBar<Msg>>,
    logo: Handle<MenuTitle<Msg>>,
    app: Handle<MenuTitle<Msg>>,
    clock: Handle<MenuTitle<Msg>>,
    log: Vec<Msg>,
}

impl Application for Bar {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let bar = MenuBar::new()
            .with(
                MenuTitle::logo(
                    "System",
                    Menu::new().with(MenuItem::new("About").on_choose(Msg::About)),
                )
                .bind(&mut self.logo),
            )
            .with(
                MenuTitle::new(
                    "Files",
                    Menu::new().with(MenuItem::new("Quit Files").on_choose(Msg::Quit)),
                )
                .bold(true)
                .bind(&mut self.app),
            )
            .with_menu(app_menu("Edit"))
            .with_extra(MenuTitle::new("10:09", Menu::new()).bind(&mut self.clock))
            .bind(&mut self.bar);
        // The bar is the surface's root, as in the shell; the window is taller so that the
        // menus fit in the fake host.
        let layer = LayerSpec::new("test-bar", Anchor::TOP_EDGE);
        cx.open_window(Window::new("Bar", (WIDTH, 300), bar).layer(layer));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Rename(name) => self.app.set_text(cx, name),
            Msg::FocusBar => self.bar.open_first_menu(cx),
            Msg::SetMenus(names) => {
                self.bar
                    .set_menus(cx, names.into_iter().map(app_menu).collect());
            }
            message => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Bar>, FakeHost) {
    let (runner, mut host, _) = start(Bar::default());
    let _ = host.only().render();
    (runner, host)
}

fn bounds_of(host: &mut FakeHost, id: WidgetId) -> Rect {
    let widget = host.only().get_widget_with_id(id);
    widget
        .ctx()
        .window_transform()
        .transform_rect_bbox(widget.ctx().border_box())
}

fn click_widget(runner: &mut Runner<Bar>, host: &mut FakeHost, id: WidgetId) {
    let at = bounds_of(host, id).center();
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

/// Whether the title labelled `label` says its menu is up.
fn expanded(host: &mut FakeHost, label: &str) -> Option<bool> {
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.role() == Role::MenuItem && node.label().as_deref() == Some(label) {
            return node.data().is_expanded();
        }
        stack.extend(node.children());
    }
    None
}

#[test]
fn titles_sit_left_and_extras_right_at_the_bar_height() {
    let (runner, mut host) = setup();
    let logo = bounds_of(&mut host, id(&runner.app().logo));
    let app = bounds_of(&mut host, id(&runner.app().app));
    let clock = bounds_of(&mut host, id(&runner.app().clock));
    assert!((logo.x0 - EDGE_INSET).abs() < 0.5, "{logo:?}");
    assert!((app.x0 - logo.x1).abs() < 0.5, "titles abut: {app:?}");
    let bar = find(&mut host, Role::MenuBar, None)[0].bounds;
    assert!((bar.width() - f64::from(WIDTH)).abs() < 0.5, "{bar:?}");
    assert!(
        (clock.x1 - (f64::from(WIDTH) - EDGE_INSET)).abs() < 0.5,
        "{clock:?}"
    );
    for rect in [logo, app, clock] {
        assert!((rect.height() - HEIGHT).abs() < 0.5);
        assert!(rect.y0.abs() < 0.5);
    }
    assert_eq!(find(&mut host, Role::MenuBar, None).len(), 1);
}

#[test]
fn a_title_opens_its_menu_below_itself_and_choosing_sends_the_message() {
    let (mut runner, mut host) = setup();
    let app = id(&runner.app().app);
    let title = bounds_of(&mut host, app);
    click_widget(&mut runner, &mut host, app);
    assert_eq!(menus_open(&mut host), 1);
    let menu = find(&mut host, Role::Menu, None)[0].bounds;
    assert!((menu.y0 - title.y1).abs() < 1.5, "right below the bar");
    assert!((menu.x0 - title.x0).abs() < 1.5, "level with the title");
    assert_eq!(expanded(&mut host, "Files"), Some(true));

    click_item(&mut runner, &mut host, "Quit Files");
    assert_eq!(runner.app().log, [Msg::Quit]);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(expanded(&mut host, "Files"), Some(false));
}

#[test]
fn the_keyboard_and_assistive_technologies_open_a_title() {
    let (mut runner, mut host) = setup();
    let logo = id(&runner.app().logo);
    host.only().focus_on(Some(logo));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log, [Msg::About]);

    let node = find(&mut host, Role::MenuItem, Some("System"))[0].node;
    a11y(&mut host, node, Action::Click);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
}

#[test]
fn a_title_with_no_menu_opens_nothing_and_text_can_change() {
    let (mut runner, mut host) = setup();
    let clock = id(&runner.app().clock);
    click_widget(&mut runner, &mut host, clock);
    assert_eq!(menus_open(&mut host), 0);

    let before = bounds_of(&mut host, id(&runner.app().app));
    runner.send(&mut host, Msg::Rename("A Much Longer Name"));
    let _ = host.only().render();
    let after = bounds_of(&mut host, id(&runner.app().app));
    assert!(after.width() > before.width());
    assert_eq!(
        find(&mut host, Role::MenuItem, Some("A Much Longer Name")).len(),
        1
    );
}

/// The bounds of the title labelled `label`, if it is in the bar.
fn title(host: &mut FakeHost, label: &str) -> Option<Rect> {
    find(host, Role::MenuItem, Some(label))
        .first()
        .map(|n| n.bounds)
}

#[test]
fn app_menus_follow_the_fixed_titles_and_report_opening_and_closing() {
    let (mut runner, mut host) = setup();
    let files = bounds_of(&mut host, id(&runner.app().app));
    let edit = title(&mut host, "Edit").unwrap();
    assert!(
        (edit.x0 - files.x1).abs() < 0.5,
        "after the app's name: {edit:?}"
    );

    let at = edit.center();
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(runner.app().log, [Msg::Opened("Edit")]);
    click_item(&mut runner, &mut host, "Edit item");
    assert_eq!(
        runner.app().log,
        [
            Msg::Opened("Edit"),
            Msg::Item("Edit item"),
            Msg::Closed("Edit")
        ]
    );
}

#[test]
fn app_menus_are_replaced_whole() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::SetMenus(vec!["Window", "Help"]));
    let _ = host.only().render();
    assert!(title(&mut host, "Edit").is_none());
    let files = bounds_of(&mut host, id(&runner.app().app));
    let window = title(&mut host, "Window").unwrap();
    let help = title(&mut host, "Help").unwrap();
    assert!((window.x0 - files.x1).abs() < 0.5, "{window:?}");
    assert!((help.x0 - window.x1).abs() < 0.5, "{help:?}");

    // The new titles' items act.
    let harness = host.only();
    harness.mouse_move(help.center());
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    click_item(&mut runner, &mut host, "Help item");
    assert!(runner.app().log.contains(&Msg::Item("Help item")));

    runner.send(&mut host, Msg::SetMenus(Vec::new()));
    let _ = host.only().render();
    assert!(title(&mut host, "Window").is_none());
    assert_eq!(
        find(&mut host, Role::MenuItem, None).len(),
        3,
        "logo, app, clock"
    );
}

/// A bar that is translucent over a blur, as the shell's.
struct BlurBar;

impl Application for BlurBar {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let bar = MenuBar::new()
            .with(MenuTitle::new("Files", Menu::new()))
            .backdrop(cx.backdrop());
        let layer = LayerSpec::new("test-bar", Anchor::TOP_EDGE);
        cx.open_window(
            Window::new("Bar", (WIDTH, HEIGHT as u32), bar)
                .layer(layer)
                .blur_behind(),
        );
    }

    fn update(&mut self, _cx: &mut Cx<'_, Msg>, _message: Msg) {}
}

fn blur_bar(blur: bool) -> FakeHost {
    let mut runner = Runner::new(BlurBar, Settings::default());
    let mut host = FakeHost {
        blur,
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    host
}

#[test]
fn the_bar_asks_for_a_plain_blur_and_turns_translucent_when_it_is_offered() {
    let mut host = blur_bar(true);
    let blur = host.blurs.values().next().copied();
    assert_eq!(blur.map(|b| b.corner_radius), Some(0.0));
    let translucent = host.only().render().get_pixel(250, 10).0;

    let mut host = blur_bar(false);
    assert!(host.blurs.is_empty());
    let opaque = host.only().render().get_pixel(250, 10).0;

    // The harness paints over a backdrop of its own, so the two tints differ in colour.
    assert_ne!(translucent, opaque, "the bar draws the translucent tint");
}

/// Moves the pointer to the centre of the title labelled `label`.
fn point_at(runner: &mut Runner<Bar>, host: &mut FakeHost, label: &str) {
    let at = title(host, label).unwrap().center();
    host.only().mouse_move(at);
    deliver(runner, host);
}

#[test]
fn pointing_at_another_title_while_a_menu_is_up_switches_menus() {
    let (mut runner, mut host) = setup();
    // Nothing is up: pointing opens nothing.
    point_at(&mut runner, &mut host, "Edit");
    assert_eq!(menus_open(&mut host), 0);

    let files = id(&runner.app().app);
    click_widget(&mut runner, &mut host, files);
    assert_eq!(expanded(&mut host, "Files"), Some(true));
    point_at(&mut runner, &mut host, "Edit");
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(expanded(&mut host, "Files"), Some(false));
    assert_eq!(expanded(&mut host, "Edit"), Some(true));
    let menu = find(&mut host, Role::Menu, None)[0].bounds;
    let edit = title(&mut host, "Edit").unwrap();
    assert!((menu.x0 - edit.x0).abs() < 1.5, "below Edit: {menu:?}");
    assert_eq!(runner.app().log, [Msg::Opened("Edit")]);

    // The extras on the right take no part.
    point_at(&mut runner, &mut host, "10:09");
    assert_eq!(expanded(&mut host, "Edit"), Some(true));

    // Back to the logo; the new menu works like one opened by a click.
    point_at(&mut runner, &mut host, "System");
    assert_eq!(expanded(&mut host, "System"), Some(true));
    assert_eq!(menus_open(&mut host), 1);
    click_item(&mut runner, &mut host, "About");
    assert_eq!(
        runner.app().log,
        [Msg::Opened("Edit"), Msg::Closed("Edit"), Msg::About]
    );
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn left_and_right_walk_the_menus_and_wrap() {
    let (mut runner, mut host) = setup();
    let logo = id(&runner.app().logo);
    host.only().focus_on(Some(logo));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));

    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("Quit Files"));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("Edit item"));
    assert_eq!(menus_open(&mut host), 1);
    // Past the last app menu: back to the logo (the clock extra is skipped).
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut host).as_deref(), Some("Edit item"));
    assert_eq!(
        runner.app().log,
        [
            Msg::Opened("Edit"),
            Msg::Closed("Edit"),
            Msg::Opened("Edit")
        ]
    );

    // Escape closes the menu, and focus goes back to where it was before the first one.
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
    assert_eq!(focused(&mut host).as_deref(), Some("System"));

    // Between focused titles, Left and Right move focus; Escape leaves the bar.
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut host).as_deref(), Some("Edit"));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("System"));
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_ne!(focused(&mut host).as_deref(), Some("System"));
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn the_shortcut_opens_the_first_menu_from_the_keyboard() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::FocusBar);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(expanded(&mut host, "System"), Some(true));
    assert_eq!(focused(&mut host).as_deref(), Some("About"));
    // Again while it is up: nothing changes.
    runner.send(&mut host, Msg::FocusBar);
    deliver(&mut runner, &mut host);
    assert_eq!(menus_open(&mut host), 1);
    assert_eq!(focused(&mut host).as_deref(), Some("About"));

    named(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(focused(&mut host).as_deref(), Some("Quit Files"));
    named(&mut runner, &mut host, NamedKey::Escape);
    assert_eq!(menus_open(&mut host), 0);
}
