//! The shell's menu bar (3.2b) on the fake host: titles on the left, extras on the right,
//! a title's menu opening below it, choosing, the keyboard, and changing a title.

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
}

const WIDTH: u32 = 500;

#[derive(Default)]
struct Bar {
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
            .with_extra(MenuTitle::new("10:09", Menu::new()).bind(&mut self.clock));
        // The bar is the surface's root, as in the shell; the window is taller so that the
        // menus fit in the fake host.
        let layer = LayerSpec::new("test-bar", Anchor::TOP_EDGE);
        cx.open_window(Window::new("Bar", (WIDTH, 300), bar).layer(layer));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        if let Msg::Rename(name) = message {
            self.app.set_text(cx, name);
            return;
        }
        self.log.push(message);
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
