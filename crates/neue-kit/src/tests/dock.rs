//! The dock (3.16) on the fake host: a click on a tile, Space on a focused tile and an
//! assistive technology's click become the tile's key; tiles are labelled buttons; a
//! tile's bounce goes on across updates and stops after the bounce under way.

use masonry_core::accesskit::Role;
use masonry_core::core::PointerButton;
use masonry_core::kurbo::Point;

use super::*;
use crate::desktop::{Dock, DockBounce, DockTile, dock_icon_rect, dock_size};
use crate::widgets::DockWidget;

#[derive(Debug, Clone)]
enum DockMsg {
    Clicked(String),
    Tiles(Vec<DockTile>),
}

#[derive(Default)]
struct Shelf {
    dock: Handle<Dock<DockMsg>>,
    clicked: Vec<String>,
}

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
        .on_click(|key| Some(DockMsg::Clicked(key.to_owned())))
        .bind(&mut self.dock);
        let size = dock_size(3);
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
            DockMsg::Tiles(tiles) => self.dock.set_tiles(cx, tiles),
        }
    }
}

fn setup() -> (Runner<Shelf>, FakeHost) {
    let (runner, mut host, _) = start(Shelf::default());
    let _ = host.only().render();
    (runner, host)
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
    let second = dock_icon_rect(1).center();
    let point = at(&runner, &mut host, second.x, second.y);
    click_at(&mut runner, &mut host, point);
    // On the shelf under the first icon counts too; the padding at the end doesn't.
    let first = dock_icon_rect(0);
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
