//! Notification banners and alerts (3.15b) on the fake host: clicks on the panel, the
//! close button and the action buttons become messages, hovering is reported, Tab reaches
//! every button, and assistive technology sees a group or an alert with labelled buttons.
//! A replacement shows in place.

use masonry_core::accesskit::Role;
use masonry_core::core::PointerButton;
use masonry_core::kurbo::Point;
use neue_theme::materials::notification::{CLOSE_INSET, MARGIN};

use super::*;
use crate::desktop::{Notification, NotificationContent, NotificationEvent};
use crate::widgets::NotificationWidget;

#[derive(Debug, Clone)]
enum NoteMsg {
    Event(NotificationEvent),
    Replace(NotificationContent),
}

#[derive(Default)]
struct Notes {
    note: Handle<Notification<NoteMsg>>,
    log: Vec<NotificationEvent>,
}

fn content(actions: &[&str]) -> NotificationContent {
    NotificationContent {
        title: "Meeting".to_owned(),
        body: "Starts in five minutes".to_owned(),
        actions: actions.iter().map(|&a| a.to_owned()).collect(),
        ..NotificationContent::default()
    }
}

impl Application for Notes {
    type Message = NoteMsg;

    fn start(&mut self, cx: &mut Cx<'_, NoteMsg>) {
        let note = Notification::new(content(&["Snooze", "Join"]))
            .on_event(|event| Some(NoteMsg::Event(event)))
            .bind(&mut self.note);
        cx.open_window(Window::new("Notes", (400, 140), note));
    }

    fn update(&mut self, cx: &mut Cx<'_, NoteMsg>, message: NoteMsg) {
        match message {
            NoteMsg::Event(event) => self.log.push(event),
            NoteMsg::Replace(content) => self.note.set_content(cx, content),
        }
    }
}

fn setup() -> (Runner<Notes>, FakeHost) {
    let (runner, mut host, _) = start(Notes::default());
    let _ = host.only().render();
    (runner, host)
}

/// The panel's id, its close button's and its action buttons'.
fn ids(runner: &Runner<Notes>, host: &mut FakeHost) -> (WidgetId, WidgetId, Vec<WidgetId>) {
    let panel = id(&runner.app().note);
    let harness = host.only();
    let widget = harness.get_widget_with_id(panel);
    let inner = widget
        .downcast::<NotificationWidget>()
        .expect("a notification")
        .inner();
    (panel, inner.close_id(), inner.action_ids())
}

/// A point `(x, y)` from the notification's top left corner, in the window.
fn at(host: &mut FakeHost, panel: WidgetId, x: f64, y: f64) -> Point {
    let harness = host.only();
    harness
        .get_widget_with_id(panel)
        .ctx()
        .to_window(Point::new(x, y))
}

fn click_at(runner: &mut Runner<Notes>, host: &mut FakeHost, point: Point) {
    let harness = host.only();
    harness.mouse_move(point);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn clicks(runner: &Runner<Notes>) -> Vec<NotificationEvent> {
    runner
        .app()
        .log
        .iter()
        .copied()
        .filter(|e| !matches!(e, NotificationEvent::Hovered(_)))
        .collect()
}

#[test]
fn clicks_on_the_panel_and_its_buttons_become_events() {
    let (mut runner, mut host) = setup();
    let (panel, _, actions) = ids(&runner, &mut host);

    let body = at(&mut host, panel, MARGIN + 100.0, MARGIN + 20.0);
    click_at(&mut runner, &mut host, body);
    let corner = at(&mut host, panel, MARGIN + CLOSE_INSET, MARGIN + CLOSE_INSET);
    click_at(&mut runner, &mut host, corner);
    for action in actions {
        host.only().mouse_click_on(action, None);
        deliver(&mut runner, &mut host);
    }
    // In the margin, outside the panel: nothing.
    let outside = at(&mut host, panel, MARGIN / 2.0, MARGIN + 30.0);
    click_at(&mut runner, &mut host, outside);
    assert_eq!(
        clicks(&runner),
        [
            NotificationEvent::Activated,
            NotificationEvent::Closed,
            NotificationEvent::Action(0),
            NotificationEvent::Action(1),
        ]
    );
}

#[test]
fn hovering_is_reported_once_each_way() {
    let (mut runner, mut host) = setup();
    let (panel, close, _) = ids(&runner, &mut host);
    let body = at(&mut host, panel, MARGIN + 100.0, MARGIN + 20.0);
    host.only().mouse_move(body);
    // Onto the close button: still over the notification.
    let corner = at(&mut host, panel, MARGIN + CLOSE_INSET, MARGIN + CLOSE_INSET);
    host.only().mouse_move(corner);
    assert!(host.only().get_widget_with_id(close).ctx().is_hovered());
    host.only().mouse_move(Point::new(399.0, 139.0));
    deliver(&mut runner, &mut host);
    assert_eq!(
        runner.app().log,
        [
            NotificationEvent::Hovered(true),
            NotificationEvent::Hovered(false)
        ]
    );
}

#[test]
fn the_keyboard_reaches_every_button() {
    let (mut runner, mut host) = setup();
    let (panel, close, actions) = ids(&runner, &mut host);
    host.only().focus_on(Some(panel));
    let order: Vec<_> = (0..3)
        .map(|_| {
            tap(host.only(), &Key::Named(NamedKey::Tab));
            host.only().focused_widget_id()
        })
        .collect();
    assert_eq!(order, [Some(close), Some(actions[0]), Some(actions[1])]);

    tap(host.only(), &SPACE());
    host.only().focus_on(Some(close));
    tap(host.only(), &RETURN);
    host.only().focus_on(Some(panel));
    tap(host.only(), &RETURN);
    tap(host.only(), &Key::Named(NamedKey::Escape));
    deliver(&mut runner, &mut host);
    assert_eq!(
        clicks(&runner),
        [
            NotificationEvent::Action(1),
            NotificationEvent::Closed,
            NotificationEvent::Activated,
            NotificationEvent::Closed,
        ]
    );
}

#[test]
fn assistive_technology_sees_an_alert_with_its_buttons() {
    let (mut runner, mut host) = setup();
    let (panel, close, actions) = ids(&runner, &mut host);
    let harness = host.only();
    let node = harness.access_node(panel).expect("a node");
    assert_eq!(node.role(), Role::Alert);
    assert_eq!(node.label().as_deref(), Some("Meeting"));
    assert_eq!(
        node.description().as_deref(),
        Some("Starts in five minutes")
    );
    let buttons: Vec<_> = node
        .children()
        .filter(|c| c.role() == Role::Button)
        .filter_map(|c| c.label())
        .collect();
    assert_eq!(buttons, ["Close", "Snooze", "Join"]);

    a11y_click(host.only(), actions[1]);
    a11y_click(host.only(), close);
    a11y_click(host.only(), panel);
    deliver(&mut runner, &mut host);
    assert_eq!(
        clicks(&runner),
        [
            NotificationEvent::Action(1),
            NotificationEvent::Closed,
            NotificationEvent::Activated,
        ]
    );
}

#[test]
fn a_replacement_shows_in_place() {
    let (mut runner, mut host) = setup();
    let (panel, _, actions) = ids(&runner, &mut host);
    let mut banner = content(&[]);
    banner.title = "Meeting moved".to_owned();
    runner.send(&mut host, NoteMsg::Replace(banner));
    let _ = host.only().render();
    let harness = host.only();
    let node = harness.access_node(panel).expect("a node");
    assert_eq!(node.role(), Role::Group, "a banner now");
    assert_eq!(node.label().as_deref(), Some("Meeting moved"));
    let buttons: Vec<_> = node.children().filter_map(|c| c.label()).collect();
    assert_eq!(buttons, ["Close"], "the action buttons are put away");

    runner.send(&mut host, NoteMsg::Replace(content(&["Open"])));
    let _ = host.only().render();
    let harness = host.only();
    let node = harness.access_node(panel).expect("a node");
    assert_eq!(node.role(), Role::Alert);
    let buttons: Vec<_> = node.children().filter_map(|c| c.label()).collect();
    assert_eq!(buttons, ["Close", "Open"]);
    host.only().mouse_click_on(actions[0], None);
    deliver(&mut runner, &mut host);
    assert_eq!(clicks(&runner), [NotificationEvent::Action(0)]);
}
