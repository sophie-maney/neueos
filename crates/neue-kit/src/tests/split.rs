//! The 2.21a split view end to end on the fake host: layout, dragging dividers within
//! limits, collapsing, resizing the window, keys, the handle, and the accessibility node.

use masonry_core::accesskit::{Action, ActionData, Orientation, Role};
use masonry_core::core::keyboard::Modifiers;
use masonry_core::kurbo::Point;

use super::table::click_with;
use super::*;
use crate::widgets::SplitWidget;
use crate::{DividerStyle, Pane, SplitView};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Resized(Vec<f64>),
    Position(usize, f64),
    Collapse(usize, bool),
}

struct Split {
    split: Handle<SplitView<Msg>>,
    style: DividerStyle,
    three: bool,
    vertical: bool,
    log: Vec<Msg>,
}

impl Split {
    fn new(style: DividerStyle) -> Self {
        Self {
            split: Handle::default(),
            style,
            three: false,
            vertical: false,
            log: Vec::new(),
        }
    }
}

impl Application for Split {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        // A 400 × 300 split: a sidebar 100 long between 80 and 200 that holds its size and
        // collapses, then the content (at least 100).
        let mut view = if self.vertical {
            SplitView::vertical().width(400.0).height(300.0)
        } else {
            SplitView::horizontal().width(400.0).height(300.0)
        };
        view = view
            .divider(self.style)
            .with_pane(
                Label::new("Sidebar"),
                Pane::new()
                    .size(100.0)
                    .min(80.0)
                    .max(200.0)
                    .holds_size()
                    .collapsible(),
            )
            .with_pane(Label::new("Content"), Pane::new().min(100.0));
        if self.three {
            view = view.with_pane(Label::new("Inspector"), Pane::new().size(60.0).min(40.0));
        }
        let view = view
            .on_resize(|lengths| Msg::Resized(lengths.to_vec()))
            .bind(&mut self.split);
        cx.open_window(Window::new("Split", (400, 300), view));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Position(pane, length) => self.split.set_position(cx, pane, length),
            Msg::Collapse(pane, on) => self.split.set_collapsed(cx, pane, on),
            _ => self.log.push(message),
        }
    }
}

fn setup_with(app: Split) -> (Runner<Split>, FakeHost) {
    let (mut runner, mut host, _) = start(app);
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn setup() -> (Runner<Split>, FakeHost) {
    setup_with(Split::new(DividerStyle::Thin))
}

fn split(runner: &Runner<Split>) -> WidgetId {
    id(&runner.app().split)
}

fn state(host: &mut FakeHost, split: WidgetId) -> (Vec<f64>, Vec<bool>) {
    let _ = host.only().render();
    host.only()
        .get_widget_with_id(split)
        .downcast::<SplitWidget>()
        .expect("a split view")
        .inner()
        .state()
}

fn lengths(host: &mut FakeHost, split: WidgetId) -> Vec<f64> {
    state(host, split).0
}

fn divider(host: &mut FakeHost, split: WidgetId, i: usize) -> Point {
    let _ = host.only().render();
    let widget = host.only().get_widget_with_id(split);
    let rect = widget
        .downcast::<SplitWidget>()
        .expect("a split view")
        .inner()
        .divider_rects()[i];
    widget.ctx().to_window(rect.center())
}

/// Drags the pointer from `from` to each point of `to`, then releases.
fn drag(runner: &mut Runner<Split>, host: &mut FakeHost, from: Point, to: &[Point]) {
    host.only().mouse_move(from);
    host.only().mouse_button_press(None);
    for &p in to {
        host.only().mouse_move(p);
    }
    host.only().mouse_button_release(None);
    let _ = host.only().render();
    deliver(runner, host);
}

fn press(runner: &mut Runner<Split>, host: &mut FakeHost, split: WidgetId, key: NamedKey) {
    let harness = host.only();
    harness.focus_on(Some(split));
    tap(harness, &Key::Named(key));
    deliver(runner, host);
}

fn take_log(runner: &mut Runner<Split>) -> Vec<Msg> {
    std::mem::take(&mut runner.app_mut().log)
}

#[test]
fn panes_are_laid_out_with_a_divider_between_them() {
    let (runner, mut host) = setup();
    assert_eq!(lengths(&mut host, split(&runner)), [100.0, 299.0]);
    let (runner, mut host) = setup_with(Split::new(DividerStyle::Thick));
    assert_eq!(lengths(&mut host, split(&runner)), [100.0, 291.0]);
    let mut three = Split::new(DividerStyle::Thin);
    three.three = true;
    let (runner, mut host) = setup_with(three);
    // The panes without a size share what's left: here the content alone.
    assert_eq!(lengths(&mut host, split(&runner)), [100.0, 238.0, 60.0]);
}

#[test]
fn dragging_a_divider_resizes_within_the_limits() {
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    let at = divider(&mut host, s, 0);
    let to = Point::new(at.x + 50.0, at.y);
    drag(&mut runner, &mut host, at, &[to]);
    assert_eq!(lengths(&mut host, s), [150.0, 249.0]);
    assert_eq!(
        take_log(&mut runner).last(),
        Some(&Msg::Resized(vec![150.0, 249.0]))
    );
    // Past the sidebar's maximum.
    let at = divider(&mut host, s, 0);
    drag(
        &mut runner,
        &mut host,
        at,
        &[Point::new(at.x + 300.0, at.y)],
    );
    assert_eq!(lengths(&mut host, s), [200.0, 199.0]);
    // Back to where the minimum applies, short of half of it: it collapses instead.
    let at = divider(&mut host, s, 0);
    drag(
        &mut runner,
        &mut host,
        at,
        &[Point::new(at.x - 140.0, at.y)],
    );
    assert_eq!(lengths(&mut host, s), [80.0, 319.0]);
    assert_eq!(state(&mut host, s).1, [false, false]);
}

#[test]
fn a_press_just_beside_a_thin_divider_drags_it_and_shows_the_resize_cursor() {
    use masonry_core::core::CursorIcon;
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    // The line is 1 px wide; 2 px either side of it is over a pane's label area.
    for side in [-2.0, 2.0] {
        let at = divider(&mut host, s, 0);
        let beside = Point::new(at.x + side, 20.0);
        host.only().mouse_move(beside);
        assert_eq!(host.only().cursor_icon(), CursorIcon::ColResize, "{side}");
        let before = lengths(&mut host, s)[0];
        drag(
            &mut runner,
            &mut host,
            beside,
            &[Point::new(beside.x + 30.0, 20.0)],
        );
        assert_eq!(lengths(&mut host, s)[0], before + 30.0, "{side}");
    }
    // Further away it is the pane's, not the divider's.
    let at = divider(&mut host, s, 0);
    let far = Point::new(at.x + 10.0, 20.0);
    host.only().mouse_move(far);
    assert_eq!(host.only().cursor_icon(), CursorIcon::Default);
    let before = lengths(&mut host, s);
    drag(
        &mut runner,
        &mut host,
        far,
        &[Point::new(far.x + 30.0, 20.0)],
    );
    assert_eq!(lengths(&mut host, s), before);
}

#[test]
fn dragging_far_enough_collapses_a_collapsible_pane_and_dragging_back_restores_it() {
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    let at = divider(&mut host, s, 0);
    drag(&mut runner, &mut host, at, &[Point::new(at.x - 90.0, at.y)]);
    assert_eq!(state(&mut host, s), (vec![0.0, 399.0], vec![true, false]));
    assert!(take_log(&mut runner).contains(&Msg::Resized(vec![0.0, 399.0])));
    // The content pane can't collapse: it isn't collapsible, so it stops at its minimum.
    let at = divider(&mut host, s, 0);
    drag(
        &mut runner,
        &mut host,
        at,
        &[Point::new(at.x + 150.0, at.y)],
    );
    assert_eq!(
        state(&mut host, s),
        (vec![150.0, 249.0], vec![false, false])
    );
}

#[test]
fn double_clicking_a_divider_collapses_and_restores_the_pane() {
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    let at = divider(&mut host, s, 0);
    click_with(&mut runner, &mut host, at, Modifiers::default(), 2);
    assert_eq!(state(&mut host, s), (vec![0.0, 399.0], vec![true, false]));
    let at = divider(&mut host, s, 0);
    click_with(&mut runner, &mut host, at, Modifiers::default(), 2);
    assert_eq!(
        state(&mut host, s),
        (vec![100.0, 299.0], vec![false, false])
    );
}

#[test]
fn a_resize_gives_the_difference_to_the_pane_that_does_not_hold() {
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    let resize = |host: &mut FakeHost, width: f64| {
        host.only().edit_widget_with_id(s, |mut w| {
            let mut w = w.downcast::<SplitWidget>();
            w.widget.set_fixed(Some(width), None);
            w.ctx.request_layout();
        });
    };
    // The sidebar holds its length; the content takes the rest.
    resize(&mut host, 600.0);
    assert_eq!(lengths(&mut host, s), [100.0, 499.0]);
    // A dragged sidebar keeps its new length through further resizes.
    let at = divider(&mut host, s, 0);
    drag(&mut runner, &mut host, at, &[Point::new(at.x + 40.0, at.y)]);
    resize(&mut host, 300.0);
    assert_eq!(lengths(&mut host, s), [140.0, 159.0]);
    // Down to the content's minimum, the sidebar gives way as well.
    resize(&mut host, 200.0);
    assert_eq!(lengths(&mut host, s), [99.0, 100.0]);
}

#[test]
fn arrow_keys_move_the_divider_and_home_end_and_return_collapse_and_restore() {
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    press(&mut runner, &mut host, s, NamedKey::ArrowRight);
    assert_eq!(lengths(&mut host, s), [110.0, 289.0]);
    press(&mut runner, &mut host, s, NamedKey::ArrowLeft);
    press(&mut runner, &mut host, s, NamedKey::ArrowLeft);
    assert_eq!(lengths(&mut host, s), [90.0, 309.0]);
    press(&mut runner, &mut host, s, NamedKey::End);
    assert_eq!(lengths(&mut host, s), [200.0, 199.0]);
    press(&mut runner, &mut host, s, NamedKey::Home);
    assert_eq!(state(&mut host, s), (vec![0.0, 399.0], vec![true, false]));
    // An arrow towards the collapsed pane's other side brings it back.
    press(&mut runner, &mut host, s, NamedKey::ArrowRight);
    assert_eq!(state(&mut host, s).1, [false, false]);
    assert_eq!(lengths(&mut host, s)[0], 200.0, "at the length it had");
    press(&mut runner, &mut host, s, NamedKey::Enter);
    assert_eq!(state(&mut host, s).1, [true, false]);
    press(&mut runner, &mut host, s, NamedKey::Enter);
    assert_eq!(state(&mut host, s).1, [false, false]);
    // Up and Down (across the split) change nothing here, and don't move the divider.
    let before = lengths(&mut host, s);
    press(&mut runner, &mut host, s, NamedKey::ArrowDown);
    assert_eq!(lengths(&mut host, s), before);
}

#[test]
fn across_arrows_pick_the_divider_in_a_split_of_three() {
    let mut three = Split::new(DividerStyle::Thin);
    three.three = true;
    let (mut runner, mut host) = setup_with(three);
    let s = split(&runner);
    press(&mut runner, &mut host, s, NamedKey::ArrowDown);
    press(&mut runner, &mut host, s, NamedKey::ArrowLeft);
    // The second divider: the content shrinks and the inspector grows.
    assert_eq!(lengths(&mut host, s), [100.0, 228.0, 70.0]);
    press(&mut runner, &mut host, s, NamedKey::ArrowUp);
    press(&mut runner, &mut host, s, NamedKey::ArrowRight);
    assert_eq!(lengths(&mut host, s)[0], 110.0);
}

#[test]
fn a_vertical_split_moves_with_up_and_down() {
    let mut app = Split::new(DividerStyle::Thick);
    app.vertical = true;
    let (mut runner, mut host) = setup_with(app);
    let s = split(&runner);
    // 300 high: 100 | 9 | 191.
    assert_eq!(lengths(&mut host, s), [100.0, 191.0]);
    press(&mut runner, &mut host, s, NamedKey::ArrowDown);
    assert_eq!(lengths(&mut host, s), [110.0, 181.0]);
    press(&mut runner, &mut host, s, NamedKey::ArrowRight);
    assert_eq!(lengths(&mut host, s), [110.0, 181.0], "across: no move");
}

#[test]
fn the_handle_moves_the_divider_and_collapses_without_sending_messages() {
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    runner.send(&mut host, Msg::Position(0, 160.0));
    let _ = host.only().render();
    assert_eq!(lengths(&mut host, s), [160.0, 239.0]);
    runner.send(&mut host, Msg::Collapse(0, true));
    assert_eq!(state(&mut host, s), (vec![0.0, 399.0], vec![true, false]));
    runner.send(&mut host, Msg::Collapse(0, false));
    assert_eq!(lengths(&mut host, s), [160.0, 239.0]);
    // The content pane isn't collapsible.
    runner.send(&mut host, Msg::Collapse(1, true));
    assert_eq!(state(&mut host, s).1, [false, false]);
    assert!(take_log(&mut runner).is_empty());
}

#[test]
fn assistive_technology_sees_a_splitter_with_a_value_and_can_move_it() {
    let (mut runner, mut host) = setup();
    let s = split(&runner);
    let _ = host.only().render();
    let node = host.only().access_node(s).expect("a node");
    assert_eq!(node.role(), Role::Splitter);
    assert_eq!(node.orientation(), Some(Orientation::Vertical));
    assert_eq!(node.numeric_value(), Some(100.0));
    assert_eq!(node.max_numeric_value(), Some(399.0));
    a11y_action(host.only(), s, Action::Increment);
    deliver(&mut runner, &mut host);
    assert_eq!(lengths(&mut host, s), [110.0, 289.0]);
    host.only().process_access_event(ActionRequest {
        action: Action::SetValue,
        target_tree: TreeId::ROOT,
        target_node: NodeId(s.to_raw()),
        data: Some(ActionData::NumericValue(150.0)),
    });
    deliver(&mut runner, &mut host);
    assert_eq!(lengths(&mut host, s), [150.0, 249.0]);
}
