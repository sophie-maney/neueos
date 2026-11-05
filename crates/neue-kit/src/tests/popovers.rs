//! Popovers (2.23a) end to end on the fake host: where they open on each side of their
//! anchor, the arrow after the driver flips one, closing (Escape, a press outside, the
//! anchor pressed again, the app), focus going in and coming back, and their content's
//! messages.

use masonry_core::accesskit::Role;
use masonry_core::core::keyboard::NamedKey;
use masonry_core::core::{PointerButton, WidgetId};
use masonry_core::kurbo::{Point, Rect};
use neue_paint::materials::Edge;
use neue_platform::{PopoverAnchor, PopoverLanded, Side};
use neue_theme::materials::popover::{ARROW_HEIGHT, GAP, PADDING, SHADOW_REACH};

use super::menus::{Found, find, focused, named};
use super::*;
use crate::Popover;
use crate::widgets::PopoverWidget;
use crate::widgets::popover::shown;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Show,
    Inner,
    Closed,
    /// Script: the app closes the popover.
    Close,
}

struct Popovers {
    edge: Edge,
    button: Handle<Button<Msg>>,
    log: Vec<Msg>,
}

impl Popovers {
    fn new(edge: Edge) -> Self {
        Self {
            edge,
            button: Handle::default(),
            log: Vec::new(),
        }
    }
}

impl Application for Popovers {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let root = Stack::column().padding(200.0).with(
            Button::new("Info")
                .on_press(Msg::Show)
                .bind(&mut self.button),
        );
        cx.open_window(Window::new("Popovers", (520, 460), root));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Show => {
                let content = Stack::column()
                    .with(Label::new("Details"))
                    .with(Button::new("Inside").on_press(Msg::Inner));
                cx.show_popover(
                    &self.button,
                    Popover::new(content).edge(self.edge).on_close(Msg::Closed),
                );
            }
            Msg::Close => cx.close_popover(&self.button),
            _ => {}
        }
        self.log.push(message);
    }
}

fn setup(edge: Edge) -> (Runner<Popovers>, FakeHost) {
    let (runner, mut host, _) = start(Popovers::new(edge));
    let _ = host.only().render();
    (runner, host)
}

fn bounds(host: &mut FakeHost, id: WidgetId) -> Rect {
    let harness = host.only();
    let _ = harness.render();
    let widget = harness.get_widget_with_id(id);
    widget
        .ctx()
        .window_transform()
        .transform_rect_bbox(widget.ctx().border_box())
}

fn click_at(runner: &mut Runner<Popovers>, host: &mut FakeHost, at: Point) {
    let harness = host.only();
    harness.mouse_move(at);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn click_button(runner: &mut Runner<Popovers>, host: &mut FakeHost) {
    let at = bounds(host, id(&runner.app().button)).center();
    click_at(runner, host, at);
}

/// Clicks the button and returns the popover that opened.
fn open(runner: &mut Runner<Popovers>, host: &mut FakeHost) -> Found {
    click_button(runner, host);
    let found = dialogs(host);
    assert_eq!(found.len(), 1, "one popover");
    found[0]
}

fn dialogs(host: &mut FakeHost) -> Vec<Found> {
    find(host, Role::Dialog, None)
}

/// The popover's widget id.
fn popover_id(runner: &Runner<Popovers>, host: &mut FakeHost) -> WidgetId {
    let button = id(&runner.app().button);
    host.only()
        .render_root()
        .edit_widget(button, |w| shown(&w))
        .expect("a popover is shown")
}

fn placed(runner: &Runner<Popovers>, host: &mut FakeHost) -> (Edge, f64) {
    let pop = popover_id(runner, host);
    host.only().render_root().edit_widget(pop, |mut w| {
        let w = w.downcast::<PopoverWidget>();
        (w.widget.placed_edge(), w.widget.arrow_at())
    })
}

#[test]
fn it_opens_below_its_anchor_centred_with_the_arrow_on_top() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    let button = bounds(&mut host, id(&runner.app().button));
    let pop = open(&mut runner, &mut host).bounds;
    assert!(
        (pop.y0 - (button.y1 + GAP)).abs() < 0.5,
        "{pop:?} under {button:?}"
    );
    assert!((pop.center().x - button.center().x).abs() < 0.5);
    let (edge, at) = placed(&runner, &mut host);
    assert_eq!(edge, Edge::Bottom);
    assert!((at - pop.width() / 2.0).abs() < 0.5);
    // Its content sits under the arrow, inside the padding.
    let inside = find(&mut host, Role::Button, Some("Inside"))[0].bounds;
    assert!(inside.y0 >= pop.y0 + ARROW_HEIGHT + PADDING - 0.5);
    assert!(inside.x0 >= pop.x0 + PADDING - 0.5);
}

#[test]
fn it_opens_on_each_side_it_asks_for() {
    for edge in [Edge::Top, Edge::Left, Edge::Right] {
        let (mut runner, mut host) = setup(edge);
        let b = bounds(&mut host, id(&runner.app().button));
        let pop = open(&mut runner, &mut host).bounds;
        let (near, centred) = match edge {
            Edge::Top => (pop.y1 - (b.y0 - GAP), pop.center().x - b.center().x),
            Edge::Left => (pop.x1 - (b.x0 - GAP), pop.center().y - b.center().y),
            Edge::Right => (pop.x0 - (b.x1 + GAP), pop.center().y - b.center().y),
            Edge::Bottom => unreachable!(),
        };
        assert!(near.abs() < 0.5, "{edge:?}: {pop:?} by {b:?}");
        // Within the half pixel Masonry snaps layers to.
        assert!(centred.abs() <= 0.5, "{edge:?}: {pop:?} by {b:?}");
        assert_eq!(placed(&runner, &mut host).0, edge);
    }
}

#[test]
fn it_tells_the_driver_its_anchor_and_side() {
    let (mut runner, mut host) = setup(Edge::Right);
    let button = bounds(&mut host, id(&runner.app().button));
    open(&mut runner, &mut host);
    let pop = popover_id(&runner, &mut host);
    let PopoverAnchor(Some(request)) = *host
        .only()
        .get_widget_with_id(pop)
        .get_prop::<PopoverAnchor>()
    else {
        panic!("no popover request");
    };
    assert_eq!(request.side, Side::Right);
    assert_eq!(request.anchor, button.inflate(GAP, GAP));
    assert!((request.reach - SHADOW_REACH).abs() < f64::EPSILON);
}

#[test]
fn flipped_by_the_driver_it_points_back_at_its_anchor() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    let button = bounds(&mut host, id(&runner.app().button));
    let pop = open(&mut runner, &mut host).bounds;
    // The compositor put it above the button, slid 30 px to the right.
    let landed = Point::new(pop.x0 + 30.0, button.y0 - GAP - pop.height());
    let id = popover_id(&runner, &mut host);
    let root = host.only().render_root();
    root.reposition_layer(id, landed);
    root.edit_widget(id, |mut w| {
        w.insert_prop(PopoverLanded(Some(landed)));
    });
    let moved = dialogs(&mut host)[0].bounds;
    assert!(
        (moved.origin() - landed).hypot() < 0.5,
        "it stays where it landed"
    );
    let (edge, at) = placed(&runner, &mut host);
    assert_eq!(edge, Edge::Top);
    assert!((at - (button.center().x - landed.x)).abs() < 0.5);
    // The content moved to leave the arrow's strip at the bottom.
    let inside = find(&mut host, Role::Button, Some("Inside"))[0].bounds;
    assert!(inside.y1 <= moved.y1 - ARROW_HEIGHT - PADDING + 0.5);
    assert!(inside.y0 >= moved.y0 + PADDING - 0.5);
}

#[test]
fn it_takes_focus_and_escape_closes_it_and_gives_focus_back() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    let button = id(&runner.app().button);
    host.only().focus_on(Some(button));
    open(&mut runner, &mut host);
    let pop = popover_id(&runner, &mut host);
    assert_eq!(host.only().focused_widget_id(), Some(pop));
    named(&mut runner, &mut host, NamedKey::Escape);
    assert!(dialogs(&mut host).is_empty());
    assert_eq!(runner.app().log, [Msg::Show, Msg::Closed]);
    assert_eq!(host.only().focused_widget_id(), Some(button));
}

#[test]
fn tab_moves_into_its_content() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    open(&mut runner, &mut host);
    named(&mut runner, &mut host, NamedKey::Tab);
    assert_eq!(focused(&mut host).as_deref(), Some("Inside"));
}

#[test]
fn its_content_sends_messages_and_a_press_inside_keeps_it_open() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    open(&mut runner, &mut host);
    let inside = find(&mut host, Role::Button, Some("Inside"))[0].bounds;
    click_at(&mut runner, &mut host, inside.center());
    assert_eq!(runner.app().log, [Msg::Show, Msg::Inner]);
    assert_eq!(dialogs(&mut host).len(), 1);
}

#[test]
fn a_press_outside_closes_it() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    open(&mut runner, &mut host);
    click_at(&mut runner, &mut host, Point::new(10.0, 10.0));
    assert!(dialogs(&mut host).is_empty());
    assert_eq!(runner.app().log, [Msg::Show, Msg::Closed]);
}

#[test]
fn pressing_the_anchor_again_closes_it_without_reopening() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    open(&mut runner, &mut host);
    click_button(&mut runner, &mut host);
    assert!(dialogs(&mut host).is_empty());
    assert_eq!(runner.app().log, [Msg::Show, Msg::Show, Msg::Closed]);
    // And the next press opens it again.
    open(&mut runner, &mut host);
}

#[test]
fn a_pointer_cancel_closes_it() {
    // What the Wayland driver sends when the compositor dismisses the popup.
    let (mut runner, mut host) = setup(Edge::Bottom);
    open(&mut runner, &mut host);
    host.only()
        .process_pointer_event(masonry_core::core::PointerEvent::Cancel(
            masonry_core::core::PointerInfo {
                pointer_id: None,
                persistent_device_id: None,
                pointer_type: masonry_core::core::PointerType::Mouse,
            },
        ));
    deliver(&mut runner, &mut host);
    assert!(dialogs(&mut host).is_empty());
    assert_eq!(runner.app().log, [Msg::Show, Msg::Closed]);
}

#[test]
fn the_app_closes_it_without_hearing_back() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    open(&mut runner, &mut host);
    runner.app_mut().log.clear();
    {
        let (runner, host) = (&mut runner, &mut host);
        runner.send(host, Msg::Close);
        deliver(runner, host);
    }
    assert!(dialogs(&mut host).is_empty());
    assert_eq!(runner.app().log, [Msg::Close]);
}

#[test]
fn showing_it_again_replaces_it() {
    let (mut runner, mut host) = setup(Edge::Bottom);
    open(&mut runner, &mut host);
    let first = popover_id(&runner, &mut host);
    runner.send(&mut host, Msg::Show);
    deliver(&mut runner, &mut host);
    assert_eq!(dialogs(&mut host).len(), 1);
    assert_ne!(popover_id(&runner, &mut host), first);
}

fn setup_with_blur(blur: bool) -> (Runner<Popovers>, FakeHost) {
    let mut runner = Runner::new(Popovers::new(Edge::Bottom), Settings::default());
    let mut host = FakeHost {
        blur,
        ..FakeHost::default()
    };
    runner.start(&mut host, Arc::new(|| {}));
    let _ = host.only().render();
    (runner, host)
}

#[test]
fn it_asks_for_a_blur_without_its_arrow_and_turns_translucent_when_it_is_offered() {
    use neue_platform::{PopupBlur, PopupBlurRequest};

    let (mut runner, mut host) = setup_with_blur(true);
    let pop = open(&mut runner, &mut host).bounds;
    let id = popover_id(&runner, &mut host);
    let request = host.only().get_widget_with_id(id).get_prop::<PopupBlur>().0;
    assert_eq!(
        request,
        Some(PopupBlurRequest {
            radius: neue_theme::materials::popover::RADIUS,
            arrow: ARROW_HEIGHT
        })
    );
    // Inside the panel, near a corner: nothing but the body's tint.
    let (x, y) = (pop.x0 as u32 + 10, pop.y1 as u32 - 10);
    let translucent = host.only().render().get_pixel(x, y).0;

    let (mut runner, mut host) = setup_with_blur(false);
    open(&mut runner, &mut host);
    let opaque = host.only().render().get_pixel(x, y).0;
    assert_ne!(translucent, opaque, "the panel draws the translucent tint");
}
