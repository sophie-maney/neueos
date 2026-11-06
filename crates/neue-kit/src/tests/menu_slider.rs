//! Slider items in a menu-bar menu (task 3.14c): a press and a drag set the value and the
//! menu stays up, Left and Right step it, and assistive technologies read and set it.

use masonry_core::accesskit::{Action, ActionData, ActionRequest, Role, TreeId};
use masonry_core::core::PointerButton;
use masonry_core::core::keyboard::NamedKey;
use masonry_core::kurbo::Point;

use super::menus::{find, focused, menus_open, named};
use super::*;
use crate::desktop::{Anchor, LayerSpec, MenuBar, MenuTitle};
use crate::{Menu, MenuItem};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Level(f64),
    Mute,
}

#[derive(Default)]
struct Sound {
    title: Handle<MenuTitle<Msg>>,
    log: Vec<Msg>,
}

impl Application for Sound {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let menu = Menu::new()
            .with(MenuItem::slider("Volume", 0.5).on_change(Msg::Level))
            .with(MenuItem::new("Mute").on_choose(Msg::Mute));
        let bar = MenuBar::new().with_extra(MenuTitle::new("Sound", menu).bind(&mut self.title));
        let layer = LayerSpec::new("test-bar", Anchor::TOP_EDGE);
        cx.open_window(Window::new("Bar", (400, 300), bar).layer(layer));
    }

    fn update(&mut self, _cx: &mut Cx<'_, Msg>, message: Msg) {
        self.log.push(message);
    }
}

fn setup() -> (Runner<Sound>, FakeHost) {
    let (runner, mut host, _) = start(Sound::default());
    let _ = host.only().render();
    (runner, host)
}

fn open_with_pointer(runner: &mut Runner<Sound>, host: &mut FakeHost) {
    let title = find(host, Role::MenuItem, Some("Sound"))[0].bounds.center();
    let harness = host.only();
    harness.mouse_move(title);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
    assert_eq!(menus_open(host), 1);
}

/// The levels sent so far.
fn levels(runner: &Runner<Sound>) -> Vec<f64> {
    runner
        .app()
        .log
        .iter()
        .filter_map(|m| match m {
            Msg::Level(v) => Some(*v),
            Msg::Mute => None,
        })
        .collect()
}

fn slider_value(host: &mut FakeHost) -> Option<f64> {
    let harness = host.only();
    let _ = harness.render();
    let state = harness.access_tree().state();
    let mut stack = vec![state.root()];
    while let Some(node) = stack.pop() {
        if node.role() == Role::Slider {
            return node.numeric_value();
        }
        stack.extend(node.children());
    }
    None
}

#[test]
fn a_press_and_a_drag_set_the_value_and_the_menu_stays_up() {
    let (mut runner, mut host) = setup();
    open_with_pointer(&mut runner, &mut host);
    let slider = find(&mut host, Role::Slider, Some("Volume"))[0].bounds;
    let y = slider.center().y;
    let harness = host.only();
    // Well left of the groove: the knob goes to 0.
    harness.mouse_move(Point::new(slider.x0 + 1.0, y));
    harness.mouse_button_press(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    assert_eq!(levels(&runner), [0.0]);
    // Dragged past the right end, even outside the menu: 1.
    let harness = host.only();
    harness.mouse_move(Point::new(slider.center().x, y + 40.0));
    harness.mouse_move(Point::new(slider.x1 + 30.0, y));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(&mut runner, &mut host);
    let sent = levels(&runner);
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert!((sent[1] - 0.5).abs() < 0.05, "{sent:?}");
    assert_eq!(sent[2], 1.0);
    assert_eq!(menus_open(&mut host), 1, "the menu stays up");
    assert_eq!(slider_value(&mut host), Some(1.0));
}

#[test]
fn left_and_right_step_a_highlighted_slider() {
    let (mut runner, mut host) = setup();
    let title = id(&runner.app().title);
    host.only().focus_on(Some(title));
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(focused(&mut host).as_deref(), Some("Volume"));
    named(&mut runner, &mut host, NamedKey::ArrowRight);
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    named(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(levels(&runner), [0.5625, 0.5, 0.4375]);
    assert_eq!(menus_open(&mut host), 1, "Left doesn't leave the menu");
    // Return on a slider does nothing; Down moves on and Return chooses.
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(menus_open(&mut host), 1);
    named(&mut runner, &mut host, NamedKey::ArrowDown);
    named(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(runner.app().log.last(), Some(&Msg::Mute));
    assert_eq!(menus_open(&mut host), 0);
}

#[test]
fn assistive_technologies_read_and_set_a_slider() {
    let (mut runner, mut host) = setup();
    open_with_pointer(&mut runner, &mut host);
    assert_eq!(slider_value(&mut host), Some(0.5));
    let node = find(&mut host, Role::Slider, Some("Volume"))[0].node;
    for (action, data) in [
        (Action::SetValue, Some(ActionData::NumericValue(0.25))),
        (Action::Increment, None),
        (Action::Click, None),
    ] {
        host.only().process_access_event(ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: node,
            data,
        });
        deliver(&mut runner, &mut host);
    }
    assert_eq!(levels(&runner), [0.25, 0.3125]);
    assert_eq!(slider_value(&mut host), Some(0.3125));
    assert_eq!(menus_open(&mut host), 1);
}
