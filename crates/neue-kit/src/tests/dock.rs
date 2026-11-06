//! The dock (3.16, 3.17a) on the fake host: a click on a tile, Space on a focused tile and
//! an assistive technology's click become the tile's key; tiles are labelled buttons; a
//! tile's bounce goes on across updates and stops after the bounce under way; icons grow
//! under the pointer and shrink when it leaves (not under reduced motion), and the dock
//! reports where they are; on the left edge the tiles run downwards.

use masonry_core::accesskit::Role;
use masonry_core::core::{PointerButton, PointerEvent};
use std::sync::Arc;

use masonry_core::kurbo::{Point, Rect};

use super::*;
use crate::desktop::{Dock, DockBounce, DockEdge, DockLayout, DockTile};
use crate::widgets::DockWidget;

#[derive(Debug, Clone)]
enum DockMsg {
    Clicked(String),
    Placed(Vec<(String, Rect)>),
    Tiles(Vec<DockTile>),
}

#[derive(Default)]
struct Shelf {
    layout: DockLayout,
    dock: Handle<Dock<DockMsg>>,
    clicked: Vec<String>,
    placed: Vec<Vec<(String, Rect)>>,
}

const MAGNIFIED: DockLayout = DockLayout {
    edge: DockEdge::Bottom,
    icon: 48.0,
    magnified: Some(96.0),
};

fn tile(key: &str, running: bool, bounce: DockBounce) -> DockTile {
    DockTile {
        key: key.to_owned(),
        name: key.to_uppercase(),
        icon: None,
        running,
        bounce,
    }
}

impl Application for Shelf {
    type Message = DockMsg;

    fn start(&mut self, cx: &mut Cx<'_, DockMsg>) {
        let dock = Dock::new(vec![
            tile("files", true, DockBounce::None),
            tile("term", false, DockBounce::None),
        ])
        .layout(self.layout)
        .on_click(|key| Some(DockMsg::Clicked(key.to_owned())))
        .on_placed(|icons| Some(DockMsg::Placed(icons.to_vec())))
        .bind(&mut self.dock);
        let size = self.layout.size(3);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test"
        )]
        cx.open_window(Window::new(
            "Dock",
            (size.width as u32, size.height as u32),
            dock,
        ));
    }

    fn update(&mut self, cx: &mut Cx<'_, DockMsg>, message: DockMsg) {
        match message {
            DockMsg::Clicked(key) => self.clicked.push(key),
            DockMsg::Placed(icons) => self.placed.push(icons),
            DockMsg::Tiles(tiles) => self.dock.set_tiles(cx, tiles),
        }
    }
}

fn setup() -> (Runner<Shelf>, FakeHost) {
    setup_with(DockLayout::default(), false)
}

fn setup_with(layout: DockLayout, reduced_motion: bool) -> (Runner<Shelf>, FakeHost) {
    let theme = Theme {
        reduced_motion,
        ..Theme::default()
    };
    let mut runner = Runner::new(
        Shelf {
            layout,
            ..Shelf::default()
        },
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

/// Runs animation frames until nothing moves.
fn settle(host: &mut FakeHost) {
    let mut frames = 0;
    while host.only().render_root().needs_anim() {
        host.only().animate_ms(16);
        frames += 1;
        assert!(frames < 200, "never settled");
    }
}

fn dock<R>(runner: &Runner<Shelf>, host: &mut FakeHost, f: impl FnOnce(&DockWidget) -> R) -> R {
    let harness = host.only();
    let widget = harness.get_widget_with_id(id(&runner.app().dock));
    f(widget.downcast::<DockWidget>().expect("the dock").inner())
}

/// A point in the window `(x, y)` from the dock's top left corner.
fn at(runner: &Runner<Shelf>, host: &mut FakeHost, x: f64, y: f64) -> Point {
    host.only()
        .get_widget_with_id(id(&runner.app().dock))
        .ctx()
        .to_window(Point::new(x, y))
}

fn click_at(runner: &mut Runner<Shelf>, host: &mut FakeHost, point: Point) {
    let harness = host.only();
    harness.mouse_move(point);
    harness.mouse_button_press(Some(PointerButton::Primary));
    harness.mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

#[test]
fn clicks_on_a_tile_send_its_key() {
    let (mut runner, mut host) = setup();
    let rest = DockLayout::default().rest(3);
    let second = rest.icons[1].center();
    let point = at(&runner, &mut host, second.x, second.y);
    click_at(&mut runner, &mut host, point);
    // On the shelf under the first icon counts too; the padding at the end doesn't.
    let first = rest.icons[0];
    let point = at(&runner, &mut host, first.center().x, first.y1 + 10.0);
    click_at(&mut runner, &mut host, point);
    let point = at(&runner, &mut host, 2.0, first.center().y);
    click_at(&mut runner, &mut host, point);
    assert_eq!(runner.app().clicked, ["term", "files"]);
}

#[test]
fn the_keyboard_and_assistive_technology_click_tiles() {
    let (mut runner, mut host) = setup();
    let tiles = dock(&runner, &mut host, DockWidget::tile_ids);
    let node = host.only().access_node(tiles[0]).expect("a node");
    assert_eq!(node.role(), Role::Button);
    assert_eq!(node.label().as_deref(), Some("FILES"));
    assert_eq!(node.description().as_deref(), Some("Running"));
    assert_eq!(
        host.only()
            .access_node(id(&runner.app().dock))
            .and_then(|n| n.label()),
        Some("Dock".to_owned())
    );

    host.only().focus_on(Some(tiles[0]));
    tap(host.only(), &Key::Named(NamedKey::Tab));
    assert_eq!(host.only().focused_widget_id(), Some(tiles[1]));
    tap(host.only(), &SPACE());
    a11y_click(host.only(), tiles[0]);
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().clicked, ["term", "files"]);
}

#[test]
fn a_bounce_goes_on_across_updates_and_ends_after_landing() {
    let (mut runner, mut host) = setup();
    let launching = vec![
        tile("files", true, DockBounce::None),
        tile("term", false, DockBounce::Launch),
    ];
    runner.send(&mut host, DockMsg::Tiles(launching.clone()));
    host.only().animate_ms(100);
    let lift = dock(&runner, &mut host, DockWidget::lifts)[1];
    assert!(lift > 0.0);
    // The same tiles again, and one more: the bounce isn't restarted.
    let mut more = launching;
    more.push(tile("mail", false, DockBounce::None));
    runner.send(&mut host, DockMsg::Tiles(more));
    host.only().animate_ms(16);
    assert!(dock(&runner, &mut host, DockWidget::lifts)[1] > lift);

    // Launched: the bounce under way lands, then it stands still.
    runner.send(
        &mut host,
        DockMsg::Tiles(vec![
            tile("files", true, DockBounce::None),
            tile("term", true, DockBounce::None),
        ]),
    );
    let mut frames = 0;
    while host.only().render_root().needs_anim() {
        host.only().animate_ms(16);
        frames += 1;
        assert!(frames < 200, "the bounce never ended");
    }
    assert_eq!(dock(&runner, &mut host, DockWidget::lifts), [0.0, 0.0]);
}

#[test]
fn icons_grow_under_the_pointer_and_shrink_when_it_leaves() {
    let (mut runner, mut host) = setup_with(MAGNIFIED, false);
    deliver(&mut runner, &mut host);
    let rest = MAGNIFIED.rest(2);
    assert_eq!(dock(&runner, &mut host, DockWidget::icons), rest.icons);
    // Placed once at rest: the minimise targets.
    let reported = |runner: &Runner<Shelf>| runner.app().placed.last().cloned();
    assert_eq!(
        reported(&runner).map(|p| p.into_iter().map(|(_, r)| r).collect::<Vec<_>>()),
        Some(rest.icons.clone())
    );

    let over = rest.icons[1].center();
    let point = at(&runner, &mut host, over.x, over.y);
    host.only().mouse_move(point);
    settle(&mut host);
    deliver(&mut runner, &mut host);
    let grown = dock(&runner, &mut host, DockWidget::icons);
    assert!((grown[1].width() - 96.0).abs() < 0.5, "{grown:?}");
    assert!(grown[0].width() > 48.0 && grown[0].width() < grown[1].width());
    // The pointer stays over the icon it was on, and the report follows.
    assert!(grown[1].contains(over));
    let placed = reported(&runner).expect("placed");
    assert_eq!(placed[1].0, "term");
    assert_eq!(placed[1].1, grown[1]);

    // A click goes to the grown icon under the pointer.
    click_at(&mut runner, &mut host, point);
    assert_eq!(runner.app().clicked, ["term"]);
    // Moving onto the part of it that spreads over the first tile's place moves the
    // magnification there: the pointer is always over the tile whose place it is in.
    let near = at(&runner, &mut host, grown[1].x0 + 2.0, over.y);
    host.only().mouse_move(near);
    host.only().animate_ms(16);
    let now = dock(&runner, &mut host, DockWidget::icons);
    assert!(now[0].width() > now[1].width(), "{now:?}");

    // Off the dock (into the room above it): back to rest.
    let point = at(&runner, &mut host, over.x, 2.0);
    host.only().mouse_move(point);
    settle(&mut host);
    deliver(&mut runner, &mut host);
    assert_eq!(dock(&runner, &mut host, DockWidget::icons), rest.icons);

    // Grown again, then the pointer leaves the window: back to rest, and reported.
    let point = at(&runner, &mut host, over.x, over.y);
    host.only().mouse_move(point);
    settle(&mut host);
    host.only()
        .process_pointer_event(PointerEvent::Leave(masonry_testing::PRIMARY_MOUSE));
    settle(&mut host);
    deliver(&mut runner, &mut host);
    assert_eq!(dock(&runner, &mut host, DockWidget::icons), rest.icons);
    let placed = reported(&runner).expect("placed");
    assert_eq!(placed[1].1, rest.icons[1]);
}

#[test]
fn reduced_motion_turns_magnification_off() {
    let (runner, mut host) = setup_with(MAGNIFIED, true);
    let runner = &runner;
    let rest = MAGNIFIED.rest(2);
    let over = rest.icons[0].center();
    let point = at(runner, &mut host, over.x, over.y);
    host.only().mouse_move(point);
    host.only().animate_ms(500);
    assert_eq!(dock(runner, &mut host, DockWidget::icons), rest.icons);
}

#[test]
fn on_the_left_edge_tiles_run_downwards() {
    let left = DockLayout {
        edge: DockEdge::Left,
        ..DockLayout::default()
    };
    let (mut runner, mut host) = setup_with(left, false);
    let rest = left.rest(2);
    assert_eq!(dock(&runner, &mut host, DockWidget::icons), rest.icons);
    assert!(rest.icons[1].y0 > rest.icons[0].y1);
    let second = rest.icons[1].center();
    let point = at(&runner, &mut host, second.x, second.y);
    click_at(&mut runner, &mut host, point);
    assert_eq!(runner.app().clicked, ["term"]);
}
