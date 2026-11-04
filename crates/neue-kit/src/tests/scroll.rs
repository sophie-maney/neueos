//! The 2.15 scroll view and virtual list end to end on the fake host: wheel, keys,
//! assistive technology, the overlay scrollers, scrolling into view, and rows built and
//! dropped as they scroll (with their message handlers).

use masonry_core::accesskit::{
    Action, ActionData, ActionRequest, NodeId, Point as AxPoint, TreeId,
};
use masonry_core::core::WidgetId;
use masonry_core::core::keyboard::{Key, NamedKey};
use masonry_core::kurbo::{Point, Vec2};
use neue_theme::motion::SCROLLER_HIDE_DELAY;

use super::*;
use crate::widgets::{RowsWidget, ScrollWidget};
use crate::{ScrollView, VirtualList};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Row(usize),
    Far,
    /// Script: the app changes the list.
    Count(usize),
    Reload,
    ToRow(usize),
}

#[derive(Default)]
struct Scrolling {
    view: Handle<ScrollView<Msg>>,
    list: Handle<VirtualList<Msg>>,
    far: Handle<Button<Msg>>,
    log: Vec<Msg>,
}

/// Lines in the scroll view, each 20 px apart (a 12 px label and 8 px spacing).
const LINES: usize = 20;

impl Application for Scrolling {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let mut lines = Stack::column();
        for i in 0..LINES {
            lines = lines.with(Label::new(format!("Line {i}")));
        }
        let content = lines.with(Button::new("Far").on_press(Msg::Far).bind(&mut self.far));
        let root = Stack::column()
            .with(
                ScrollView::new(content)
                    .width(150.0)
                    .height(100.0)
                    .bind(&mut self.view),
            )
            .with(
                VirtualList::new(1000, 20.0, |i| {
                    Button::new(format!("Row {i}")).on_press(Msg::Row(i))
                })
                .width(150.0)
                .height(100.0)
                .bind(&mut self.list),
            );
        cx.open_window(Window::new("Scrolling", (200, 220), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Count(n) => self.list.set_count(cx, n),
            Msg::Reload => self.list.reload(cx),
            Msg::ToRow(i) => self.list.scroll_to_row(cx, i),
            _ => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Scrolling>, FakeHost) {
    let (mut runner, mut host, _) = start(Scrolling::default());
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn scroll_widget(host: &mut FakeHost, view: WidgetId) -> &ScrollWidget {
    host.only()
        .get_widget_with_id(view)
        .downcast::<ScrollWidget>()
        .expect("a scroll view")
        .inner()
}

fn offset(host: &mut FakeHost, view: WidgetId) -> Vec2 {
    scroll_widget(host, view).offset()
}

fn rows(host: &mut FakeHost, list: WidgetId) -> &RowsWidget {
    let content = scroll_widget(host, list).content_id();
    host.only()
        .get_widget_with_id(content)
        .downcast::<RowsWidget>()
        .expect("rows")
        .inner()
}

/// The window position `(fx, fy)` of the way across `widget`'s box.
fn point_in(host: &mut FakeHost, widget: WidgetId, fx: f64, fy: f64) -> Point {
    let widget = host.only().get_widget_with_id(widget);
    let ctx = widget.ctx();
    let size = ctx.border_box().size();
    ctx.to_window(Point::new(size.width * fx, size.height * fy))
}

fn wheel(runner: &mut Runner<Scrolling>, host: &mut FakeHost, view: WidgetId, dy: f64) {
    let at = point_in(host, view, 0.3, 0.5);
    host.only().mouse_move(at);
    host.only().mouse_wheel(Vec2::new(0.0, dy));
    deliver(runner, host);
}

fn access(host: &mut FakeHost, widget: WidgetId, action: Action, data: Option<ActionData>) {
    host.only().process_access_event(ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: NodeId(widget.to_raw()),
        data,
    });
}

/// How far the scroll view's content can scroll: 20 lines and a button in 100 px.
fn range(host: &mut FakeHost, view: WidgetId) -> f64 {
    let _ = host.only().render();
    host.only()
        .access_node(view)
        .and_then(|n| n.scroll_y_max())
        .expect("scrolls vertically")
}

#[test]
fn the_wheel_scrolls_and_stops_at_the_ends() {
    let (mut runner, mut host) = setup();
    let view = id(&runner.app().view);
    let max = range(&mut host, view);
    assert!(max > 300.0, "{max}");
    wheel(&mut runner, &mut host, view, -50.0);
    assert_eq!(offset(&mut host, view), Vec2::new(0.0, 50.0));
    let _ = host.only().render();
    assert_eq!(
        host.only().access_node(view).unwrap().scroll_y(),
        Some(50.0)
    );
    wheel(&mut runner, &mut host, view, -10_000.0);
    assert_eq!(offset(&mut host, view).y, max);
    wheel(&mut runner, &mut host, view, 10_000.0);
    assert_eq!(offset(&mut host, view).y, 0.0);
}

#[test]
fn keys_scroll_a_focused_view_by_line_page_and_to_the_ends() {
    let (mut runner, mut host) = setup();
    let view = id(&runner.app().view);
    let max = range(&mut host, view);
    host.only().focus_on(Some(view));
    let mut after = |key: NamedKey| {
        tap(host.only(), &Key::Named(key));
        deliver(&mut runner, &mut host);
        offset(&mut host, view).y
    };
    assert_eq!(after(NamedKey::ArrowDown), 40.0, "a line");
    assert_eq!(after(NamedKey::PageDown), 100.0, "a page less a line");
    assert_eq!(after(NamedKey::End), max);
    assert_eq!(after(NamedKey::PageUp), max - 60.0);
    assert_eq!(after(NamedKey::Home), 0.0);
    assert_eq!(after(NamedKey::ArrowLeft), 0.0, "no horizontal scrolling");
}

#[test]
fn assistive_technology_scrolls_by_line_page_and_to_a_point() {
    let (mut runner, mut host) = setup();
    let view = id(&runner.app().view);
    let _ = range(&mut host, view);
    access(&mut host, view, Action::ScrollDown, None);
    assert_eq!(offset(&mut host, view).y, 40.0);
    access(
        &mut host,
        view,
        Action::ScrollDown,
        Some(ActionData::ScrollUnit(
            masonry_core::accesskit::ScrollUnit::Page,
        )),
    );
    assert_eq!(offset(&mut host, view).y, 100.0);
    access(
        &mut host,
        view,
        Action::SetScrollOffset,
        Some(ActionData::SetScrollOffset(AxPoint::new(0.0, 75.0))),
    );
    assert_eq!(offset(&mut host, view).y, 75.0);
    deliver(&mut runner, &mut host);
}

#[test]
fn scrollers_show_while_scrolling_then_fade() {
    let (mut runner, mut host) = setup();
    let view = id(&runner.app().view);
    assert_eq!(scroll_widget(&mut host, view).scrollers(), (false, 0.0));
    wheel(&mut runner, &mut host, view, -50.0);
    // Leave the view, so only scrolling keeps them.
    host.only().mouse_move(Point::new(190.0, 215.0));
    assert_eq!(scroll_widget(&mut host, view).scrollers(), (true, 1.0));
    host.only().animate_ms(500);
    assert_eq!(scroll_widget(&mut host, view).scrollers(), (true, 1.0));
    host.only().animate_ms(600);
    let mut frames = 0;
    while host.only().render_root().needs_anim() {
        host.only().animate_ms(16);
        let (_, opacity) = scroll_widget(&mut host, view).scrollers();
        assert!(opacity < 1.0, "fading");
        frames += 1;
        assert!(frames < 200, "the fade never ended");
    }
    assert!(frames > 3, "faded over several frames");
    assert_eq!(scroll_widget(&mut host, view).scrollers(), (false, 0.0));
}

#[test]
fn reduced_motion_hides_scrollers_without_a_fade() {
    let mut runner = Runner::new(Scrolling::default(), Settings::default());
    let mut host = FakeHost {
        theme: Theme {
            reduced_motion: true,
            ..Theme::default()
        },
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    let _ = host.only().render();
    let view = id(&runner.app().view);
    wheel(&mut runner, &mut host, view, -50.0);
    host.only().mouse_move(Point::new(190.0, 215.0));
    host.only()
        .animate_ms(u64::try_from(SCROLLER_HIDE_DELAY.as_millis()).unwrap());
    assert_eq!(scroll_widget(&mut host, view).scrollers(), (false, 0.0));
    assert!(!host.only().render_root().needs_anim());
}

#[test]
fn the_pointer_coming_in_flashes_the_scrollers_and_one_under_it_stays() {
    let (runner, mut host) = setup();
    let view = id(&runner.app().view);
    let edge = point_in(&mut host, view, 0.97, 0.1);
    host.only().mouse_move(edge);
    assert_eq!(scroll_widget(&mut host, view).scrollers(), (true, 1.0));
    host.only().animate_ms(3000);
    host.only().animate_ms(16);
    assert!(scroll_widget(&mut host, view).scrollers().0, "held");
}

#[test]
fn dragging_the_knob_and_pressing_the_track_scroll() {
    let (mut runner, mut host) = setup();
    let view = id(&runner.app().view);
    let max = range(&mut host, view);
    // In: the scrollers flash. The knob starts at the top of the strip.
    let knob = point_in(&mut host, view, 0.97, 0.06);
    host.only().mouse_move(knob);
    host.only().mouse_button_press(None);
    host.only().mouse_move(knob + Vec2::new(0.0, 20.0));
    deliver(&mut runner, &mut host);
    let dragged = offset(&mut host, view).y;
    assert!(
        dragged > 20.0,
        "the content moves faster than the knob: {dragged}"
    );
    host.only().mouse_move(knob + Vec2::new(0.0, 500.0));
    assert_eq!(offset(&mut host, view).y, max, "clamped at the end");
    host.only().mouse_button_release(None);
    host.only().mouse_move(knob);
    host.only().mouse_button_press(None);
    host.only().mouse_button_release(None);
    assert_eq!(offset(&mut host, view).y, max - 60.0, "a page back");
    assert!(runner.app().log.is_empty());
}

#[test]
fn a_widget_asking_to_be_seen_scrolls_into_view() {
    let (runner, mut host) = setup();
    let view = id(&runner.app().view);
    let far = id(&runner.app().far);
    let max = range(&mut host, view);
    host.only().scroll_into_view(far);
    assert_eq!(offset(&mut host, view).y, max, "the button is last");
}

#[test]
fn the_list_builds_only_the_rows_near_the_screen() {
    let (mut runner, mut host) = setup();
    let list = id(&runner.app().list);
    // 100 px of 20 px rows: 0..5 show, half a screen more is built below.
    assert_eq!(rows(&mut host, list).built(), (0..8).collect::<Vec<_>>());
    wheel(&mut runner, &mut host, list, -200.0);
    let _ = host.only().render();
    assert_eq!(rows(&mut host, list).built(), (7..18).collect::<Vec<_>>());
    let _ = host.only().render();
    assert_eq!(
        host.only().access_node(list).unwrap().scroll_y_max(),
        Some(1000.0 * 20.0 - 100.0)
    );
}

#[test]
fn rows_send_messages_and_dropped_rows_forget_their_handlers() {
    let (mut runner, mut host) = setup();
    let list = id(&runner.app().list);
    let handlers = runner.handler_count();
    wheel(&mut runner, &mut host, list, -200.0);
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    let row = rows(&mut host, list).row_id(11).expect("row 11 is built");
    host.only().mouse_click_on(row, None);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Row(11)]);
    for _ in 0..50 {
        wheel(&mut runner, &mut host, list, -400.0);
        let _ = host.only().render();
        deliver(&mut runner, &mut host);
    }
    assert!(
        runner.handler_count() <= handlers + 4,
        "handlers of dropped rows are gone: {} vs {handlers}",
        runner.handler_count()
    );
}

#[test]
fn the_app_changes_the_count_reloads_and_scrolls_to_a_row() {
    let (mut runner, mut host) = setup();
    let list = id(&runner.app().list);
    runner.send(&mut host, Msg::ToRow(500));
    let _ = host.only().render();
    assert_eq!(
        offset(&mut host, list).y,
        501.0 * 20.0 - 100.0,
        "row 500 at the bottom"
    );
    assert!(rows(&mut host, list).built().contains(&500));
    runner.send(&mut host, Msg::Count(3));
    let _ = host.only().render();
    assert_eq!(offset(&mut host, list).y, 0.0);
    assert_eq!(rows(&mut host, list).built(), [0, 1, 2]);
    let before = rows(&mut host, list).row_id(0);
    runner.send(&mut host, Msg::Reload);
    let _ = host.only().render();
    let after = rows(&mut host, list).row_id(0);
    assert!(after.is_some() && after != before, "rebuilt");
    deliver(&mut runner, &mut host);
    let row = rows(&mut host, list).row_id(2).unwrap();
    a11y_click(host.only(), row);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().log, [Msg::Row(2)]);
}
