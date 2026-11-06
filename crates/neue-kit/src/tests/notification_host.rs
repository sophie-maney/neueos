//! The notification host (3.15d2) on the fake host: panels slide in on top and out to the
//! right, the ones below make room and close up, a replacement shows in place, the host is
//! as tall as the stack, every panel's buttons are reached by Tab top to bottom, and what
//! the user does reaches the app with the notification's id.

use masonry_core::accesskit::Role;
use masonry_core::core::PointerButton;
use masonry_core::kurbo::Point;
use neue_theme::materials::notification::{CLOSE_INSET, MARGIN, SPACING};

use super::*;
use crate::desktop::{NotificationContent, NotificationEvent, NotificationHost};
use crate::notification_stack::SLIDE_DISTANCE;
use crate::widgets::NotificationHostWidget;

#[derive(Debug, Clone)]
enum Msg {
    Event(u32, NotificationEvent),
    Gone(u32),
    Show(u32, NotificationContent),
    Dismiss(u32),
}

#[derive(Default)]
struct Center {
    host: Handle<NotificationHost<Msg>>,
    log: Vec<(u32, NotificationEvent)>,
    gone: Vec<u32>,
}

impl Application for Center {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let host = NotificationHost::new()
            .on_event(|id, event| Some(Msg::Event(id, event)))
            .on_gone(|id| Some(Msg::Gone(id)))
            .bind(&mut self.host);
        cx.open_window(Window::new("Notifications", (400, 500), host));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Event(id, event) => self.log.push((id, event)),
            Msg::Gone(id) => self.gone.push(id),
            Msg::Show(id, content) => self.host.show(cx, id, content),
            Msg::Dismiss(id) => self.host.dismiss(cx, id),
        }
    }
}

fn banner(title: &str) -> NotificationContent {
    NotificationContent {
        title: title.to_owned(),
        body: "One line".to_owned(),
        ..NotificationContent::default()
    }
}

fn alert(title: &str) -> NotificationContent {
    NotificationContent {
        actions: vec!["Snooze".to_owned(), "Join".to_owned()],
        ..banner(title)
    }
}

fn setup(reduced_motion: bool) -> (Runner<Center>, FakeHost) {
    let theme = Theme {
        reduced_motion,
        ..Theme::default()
    };
    let mut runner = Runner::new(
        Center::default(),
        Settings {
            theme,
            ..Settings::default()
        },
    );
    let mut host = FakeHost {
        theme,
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    let _ = host.only().render();
    (runner, host)
}

/// Shows notifications `ids` (banners titled by their id) and lets them settle.
fn show_all(runner: &mut Runner<Center>, host: &mut FakeHost, ids: &[u32]) {
    for &id in ids {
        runner.send(host, Msg::Show(id, banner(&format!("Note {id}"))));
    }
    settle(host);
}

/// Steps animation frames until nothing moves, delivering what the host emits.
fn settle_and_deliver(runner: &mut Runner<Center>, host: &mut FakeHost) {
    settle(host);
    deliver(runner, host);
}

fn settle(host: &mut FakeHost) {
    let mut frames = 0;
    while host.only().render_root().needs_anim() {
        host.only().animate_ms(16);
        frames += 1;
        assert!(frames < 300, "the stack never came to rest");
    }
}

fn host_widget<'a>(runner: &Runner<Center>, host: &'a mut FakeHost) -> &'a NotificationHostWidget {
    host.only()
        .get_widget_with_id(id(&runner.app().host))
        .downcast::<NotificationHostWidget>()
        .expect("the host")
        .inner()
}

/// The notifications' ids and their panels' widget ids, top to bottom.
fn panels(runner: &Runner<Center>, host: &mut FakeHost) -> Vec<(u32, WidgetId)> {
    host_widget(runner, host).panel_ids()
}

/// Where each panel is, from the host's top left corner, top to bottom.
fn positions(runner: &Runner<Center>, host: &mut FakeHost) -> Vec<(u32, Point)> {
    let origin = host
        .only()
        .get_widget_with_id(id(&runner.app().host))
        .ctx()
        .to_window(Point::ZERO);
    panels(runner, host)
        .into_iter()
        .map(|(note, panel)| {
            let at = host
                .only()
                .get_widget_with_id(panel)
                .ctx()
                .to_window(Point::ZERO);
            (note, (at - origin).to_point())
        })
        .collect()
}

/// The host's size.
fn host_size(runner: &Runner<Center>, host: &mut FakeHost) -> (f64, f64) {
    let size = host
        .only()
        .get_widget_with_id(id(&runner.app().host))
        .ctx()
        .border_box()
        .size();
    (size.width, size.height)
}

/// A panel's height, without its margin.
fn panel_height(host: &mut FakeHost, panel: WidgetId) -> f64 {
    host.only()
        .get_widget_with_id(panel)
        .ctx()
        .border_box()
        .size()
        .height
        - 2.0 * MARGIN
}

#[test]
fn a_new_panel_slides_in_on_top_and_the_others_make_room() {
    let (mut runner, mut host) = setup(false);
    assert_eq!(host_size(&runner, &mut host).1, 1.0, "empty: a pixel");
    runner.send(&mut host, Msg::Show(1, banner("First")));
    let _ = host.only().render();
    let start = positions(&runner, &mut host);
    assert_eq!(
        start,
        [(1, Point::new(SLIDE_DISTANCE, 0.0))],
        "starts off to the right"
    );
    host.only().animate_ms(64);
    let x = positions(&runner, &mut host)[0].1.x;
    assert!(x > 0.0 && x < SLIDE_DISTANCE, "sliding: {x}");
    settle(&mut host);
    assert_eq!(positions(&runner, &mut host), [(1, Point::ZERO)]);

    runner.send(&mut host, Msg::Show(2, alert("Second")));
    host.only().animate_ms(64);
    let moving = positions(&runner, &mut host);
    let (_, first) = panels(&runner, &mut host)[1];
    let first_height = panel_height(&mut host, first);
    assert_eq!(moving[1].0, 1);
    assert!(
        moving[1].1.y > 0.0,
        "the first moves down under the second: {:?}",
        moving[1].1
    );
    settle(&mut host);
    let (_, second) = panels(&runner, &mut host)[0];
    let second_height = panel_height(&mut host, second);
    assert_eq!(
        positions(&runner, &mut host),
        [
            (2, Point::ZERO),
            (1, Point::new(0.0, second_height + SPACING))
        ]
    );
    assert!(first_height > 0.0 && second_height > 0.0);
}

#[test]
fn the_host_is_as_tall_as_the_stack() {
    let (mut runner, mut host) = setup(false);
    show_all(&mut runner, &mut host, &[1, 2, 3]);
    let heights: Vec<f64> = panels(&runner, &mut host)
        .into_iter()
        .map(|(_, panel)| panel_height(&mut host, panel))
        .collect();
    let extent = heights.iter().sum::<f64>() + 2.0 * SPACING;
    assert!(host_widget(&runner, &mut host).contains(3));
    assert_eq!(
        host_size(&runner, &mut host),
        (
            neue_theme::materials::notification::WIDTH + 2.0 * MARGIN,
            extent + 2.0 * MARGIN
        )
    );

    // A new one: the host grows at once, before the others have moved down.
    runner.send(&mut host, Msg::Show(4, banner("Fourth")));
    let _ = host.only().render();
    let grown = host_size(&runner, &mut host).1;
    assert!(grown > extent + 2.0 * MARGIN + SPACING, "grown: {grown}");
    settle(&mut host);
    let rest = host_size(&runner, &mut host).1;
    assert_eq!(rest, grown, "already at the size it needs");

    // One gone: it shrinks once the others have closed up.
    runner.send(&mut host, Msg::Dismiss(4));
    settle_and_deliver(&mut runner, &mut host);
    assert_eq!(host_size(&runner, &mut host).1, extent + 2.0 * MARGIN);
}

#[test]
fn a_dismissed_panel_slides_out_then_the_rest_close_up() {
    let (mut runner, mut host) = setup(false);
    show_all(&mut runner, &mut host, &[1, 2, 3]);
    let before = positions(&runner, &mut host);
    let handlers = runner.handler_count();

    runner.send(&mut host, Msg::Dismiss(2));
    host.only().animate_ms(64);
    let moving = positions(&runner, &mut host);
    assert!(moving[1].1.x > 0.0, "2 slides right: {:?}", moving[1]);
    assert_eq!(moving[2], before[2], "1 waits for 2 to go");

    settle_and_deliver(&mut runner, &mut host);
    assert_eq!(runner.app().gone, [2]);
    let after = positions(&runner, &mut host);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], (1, before[1].1), "1 moved up into 2's place");
    // A panel has four handlers: the panel, its close and two action buttons.
    assert_eq!(
        runner.handler_count(),
        handlers - 4,
        "its handlers are forgotten"
    );
}

#[test]
fn a_replacement_shows_in_place_and_moves_the_ones_below() {
    let (mut runner, mut host) = setup(false);
    show_all(&mut runner, &mut host, &[1, 2]);
    let before = panels(&runner, &mut host);
    let (_, top) = before[0];
    let short = panel_height(&mut host, top);

    let mut longer = banner("Note 2, longer");
    longer.body = "A body long enough to run onto a second line of the banner, and \
                   then some more so that it does"
        .to_owned();
    runner.send(&mut host, Msg::Show(2, longer));
    settle(&mut host);
    assert_eq!(panels(&runner, &mut host), before, "the same panels");
    let tall = panel_height(&mut host, top);
    assert!(tall > short, "taller: {tall} > {short}");
    assert_eq!(
        positions(&runner, &mut host)[1],
        (1, Point::new(0.0, tall + SPACING))
    );
    let _ = host.only().render();
    let node = host.only().access_node(top).expect("a node");
    assert_eq!(node.label().as_deref(), Some("Note 2, longer"));

    // Shown again while sliding out: it comes back.
    runner.send(&mut host, Msg::Dismiss(1));
    host.only().animate_ms(48);
    runner.send(&mut host, Msg::Show(1, alert("Back")));
    settle_and_deliver(&mut runner, &mut host);
    assert!(runner.app().gone.is_empty());
    assert_eq!(positions(&runner, &mut host)[1].1.x, 0.0);
    let (_, one) = panels(&runner, &mut host)[1];
    let _ = host.only().render();
    assert_eq!(
        host.only().access_node(one).expect("a node").role(),
        Role::Alert
    );
}

#[test]
fn reduced_motion_jumps() {
    let (mut runner, mut host) = setup(true);
    runner.send(&mut host, Msg::Show(1, banner("First")));
    runner.send(&mut host, Msg::Show(2, banner("Second")));
    let _ = host.only().render();
    // In place before any frame.
    let at = positions(&runner, &mut host);
    assert_eq!(at[0], (2, Point::ZERO));
    assert_eq!(at[1].1.x, 0.0);
    host.only().animate_ms(16);
    assert!(!host.only().render_root().needs_anim(), "nothing moves");

    runner.send(&mut host, Msg::Dismiss(2));
    deliver(&mut runner, &mut host);
    let _ = host.only().render();
    assert_eq!(runner.app().gone, [2], "gone at once");
    assert_eq!(positions(&runner, &mut host), [(1, Point::ZERO)]);
}

#[test]
fn clicks_reach_the_app_with_the_notifications_id() {
    let (mut runner, mut host) = setup(false);
    show_all(&mut runner, &mut host, &[1, 2]);
    let origin = host
        .only()
        .get_widget_with_id(id(&runner.app().host))
        .ctx()
        .to_window(Point::ZERO);
    let at = positions(&runner, &mut host);
    let click = |host: &mut FakeHost, point: Point| {
        let harness = host.only();
        harness.mouse_move(point);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
    };
    // The lower panel's close button, which stands up into the upper one's margin.
    let close = origin + at[1].1.to_vec2() + (MARGIN + CLOSE_INSET, MARGIN + CLOSE_INSET - 4.0);
    click(&mut host, close);
    // The upper panel's body.
    click(&mut host, origin + (MARGIN + 150.0, MARGIN + 20.0));
    deliver(&mut runner, &mut host);
    let clicks: Vec<_> = runner
        .app()
        .log
        .iter()
        .copied()
        .filter(|(_, e)| !matches!(e, NotificationEvent::Hovered(_)))
        .collect();
    assert_eq!(
        clicks,
        [
            (1, NotificationEvent::Closed),
            (2, NotificationEvent::Activated)
        ]
    );
    // The host does not dismiss by itself.
    assert_eq!(panels(&runner, &mut host).len(), 2);
}

#[test]
fn tab_reaches_every_panels_buttons_top_to_bottom() {
    let (mut runner, mut host) = setup(false);
    runner.send(&mut host, Msg::Show(1, alert("Bottom")));
    runner.send(&mut host, Msg::Show(2, banner("Top")));
    settle(&mut host);
    let ids = panels(&runner, &mut host);
    let (top, bottom) = (ids[0].1, ids[1].1);
    let buttons = |host: &mut FakeHost, panel: WidgetId| -> Vec<WidgetId> {
        let harness = host.only();
        let widget = harness.get_widget_with_id(panel);
        let inner = widget
            .downcast::<crate::widgets::NotificationWidget>()
            .expect("a notification")
            .inner();
        std::iter::once(inner.close_id())
            .chain(inner.action_ids())
            .collect()
    };
    let top_buttons = buttons(&mut host, top);
    let bottom_buttons = buttons(&mut host, bottom);

    host.only().focus_on(Some(top));
    let order: Vec<_> = (0..5)
        .map(|_| {
            tap(host.only(), &Key::Named(NamedKey::Tab));
            host.only().focused_widget_id()
        })
        .collect();
    // The banner's action buttons are put away, so Tab skips them.
    assert_eq!(
        order,
        [
            Some(top_buttons[0]),
            Some(bottom),
            Some(bottom_buttons[0]),
            Some(bottom_buttons[1]),
            Some(bottom_buttons[2]),
        ]
    );
    tap(host.only(), &SPACE());
    tap(host.only(), &Key::Named(NamedKey::Tab));
    deliver(&mut runner, &mut host);
    assert!(
        runner
            .app()
            .log
            .contains(&(1, NotificationEvent::Action(1)))
    );
}
