//! The dock (3.16, 3.17a) on the fake host: a click on a tile, Space on a focused tile and
//! an assistive technology's click become the tile's key; tiles are labelled buttons; a
//! tile's bounce goes on across updates and stops after the bounce under way; icons grow
//! under the pointer and shrink when it leaves (not under reduced motion), and the dock
//! reports where they are; on the left edge the tiles run downwards. Editing (3.17b): a
//! tile dragged along the dock opens a gap and asks to move, dragged off it asks to go,
//! and the tiles slide to where the app puts them; a tile's menu is asked for by a
//! secondary click, the Menu key or an assistive technology and opens against its icon.

use masonry_core::accesskit::Role;
use masonry_core::core::{PointerButton, PointerEvent};
use std::sync::Arc;

use masonry_core::accesskit::Action;
use masonry_core::kurbo::{Point, Rect, Vec2};
use neue_platform::{PopoverAnchor, Side};
use neue_theme::materials::dock::MENU_GAP;

use super::*;
use crate::desktop::{Dock, DockBounce, DockEdge, DockLayout, DockTile, DockTileKind};
use crate::widgets::{DockWidget, MenuWidget};
use crate::{Menu, MenuItem};

#[derive(Debug, Clone)]
enum DockMsg {
    Clicked(String),
    Placed(Vec<(String, Rect)>),
    Tiles(Vec<DockTile>),
    Moved(String, usize),
    Removed(String),
    MenuWanted(String),
    Chose(&'static str, String),
}

#[derive(Default)]
struct Shelf {
    layout: DockLayout,
    dock: Handle<Dock<DockMsg>>,
    /// The tiles shown; empty: the two of [`Shelf::start`].
    tiles: Vec<DockTile>,
    /// Moves are taken (the tiles reordered) unless this is set.
    refuse: bool,
    clicked: Vec<String>,
    placed: Vec<Vec<(String, Rect)>>,
    edits: Vec<String>,
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
        ..DockTile::default()
    }
}

impl Application for Shelf {
    type Message = DockMsg;

    fn start(&mut self, cx: &mut Cx<'_, DockMsg>) {
        if self.tiles.is_empty() {
            self.tiles = vec![
                tile("files", true, DockBounce::None),
                tile("term", false, DockBounce::None),
            ];
        }
        let dock = Dock::new(self.tiles.clone())
            .layout(self.layout)
            .on_click(|key| Some(DockMsg::Clicked(key.to_owned())))
            .on_placed(|icons| Some(DockMsg::Placed(icons.to_vec())))
            .on_move(|key, to| Some(DockMsg::Moved(key.to_owned(), to)))
            .on_remove(|key| Some(DockMsg::Removed(key.to_owned())))
            .on_menu(|key| Some(DockMsg::MenuWanted(key.to_owned())))
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
            DockMsg::Moved(key, to) => {
                self.edits.push(format!("move {key} {to}"));
                if !self.refuse
                    && let Some(i) = self.tiles.iter().position(|t| t.key == key)
                {
                    let moved = self.tiles.remove(i);
                    self.tiles.insert(to, moved);
                    self.dock.set_tiles(cx, self.tiles.clone());
                }
            }
            DockMsg::Removed(key) => {
                self.edits.push(format!("remove {key}"));
                self.tiles.retain(|t| t.key != key);
                self.dock.set_tiles(cx, self.tiles.clone());
            }
            DockMsg::MenuWanted(key) => {
                self.edits.push(format!("menu {key}"));
                let menu = Menu::new()
                    .with(
                        MenuItem::new("Keep in Dock")
                            .on_choose(DockMsg::Chose("keep", key.clone())),
                    )
                    .with(MenuItem::new("Quit").on_choose(DockMsg::Chose("quit", key.clone())));
                self.dock.show_menu(cx, &key, menu);
            }
            DockMsg::Chose(item, key) => self.edits.push(format!("{item} {key}")),
        }
    }
}

fn setup() -> (Runner<Shelf>, FakeHost) {
    setup_with(DockLayout::default(), false)
}

fn setup_with(layout: DockLayout, reduced_motion: bool) -> (Runner<Shelf>, FakeHost) {
    setup_shelf(
        Shelf {
            layout,
            ..Shelf::default()
        },
        reduced_motion,
    )
}

/// The default dock with three tiles: a, b and c.
fn setup_three() -> (Runner<Shelf>, FakeHost) {
    setup_shelf(
        Shelf {
            tiles: ["a", "b", "c"]
                .map(|k| tile(k, false, DockBounce::None))
                .to_vec(),
            ..Shelf::default()
        },
        false,
    )
}

fn setup_shelf(shelf: Shelf, reduced_motion: bool) -> (Runner<Shelf>, FakeHost) {
    let theme = Theme {
        reduced_motion,
        ..Theme::default()
    };
    let mut runner = Runner::new(
        shelf,
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

/// Presses the primary button at `from` (in the dock) and moves to each point of `path`,
/// leaving the button down.
fn drag(runner: &mut Runner<Shelf>, host: &mut FakeHost, from: Point, path: &[Point]) {
    let start = at(runner, host, from.x, from.y);
    host.only().mouse_move(start);
    host.only().mouse_button_press(Some(PointerButton::Primary));
    for p in path {
        let p = at(runner, host, p.x, p.y);
        host.only().mouse_move(p);
    }
    deliver(runner, host);
}

fn release(runner: &mut Runner<Shelf>, host: &mut FakeHost) {
    host.only()
        .mouse_button_release(Some(PointerButton::Primary));
    deliver(runner, host);
}

fn shown(runner: &Runner<Shelf>, host: &mut FakeHost) -> Vec<String> {
    dock(runner, host, DockWidget::shown_keys)
}

fn drawn(runner: &Runner<Shelf>, host: &mut FakeHost, key: &str) -> Rect {
    dock(runner, host, DockWidget::drawn_icons)
        .into_iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, r)| r)
        .expect("the tile is drawn")
}

#[test]
fn dragging_a_tile_opens_a_gap_and_dropping_moves_it() {
    let (mut runner, mut host) = setup_three();
    let rest = DockLayout::default().rest(3);
    let a = rest.icons[0].center();
    let over_b = rest.icons[1].center() + Vec2::new(5.0, -10.0);
    drag(&mut runner, &mut host, a, &[a + Vec2::new(2.0, 0.0)]);
    // Under the threshold: still a press.
    assert_eq!(shown(&runner, &mut host), ["a", "b", "c"]);
    drag_on(&mut runner, &mut host, &[over_b]);
    assert_eq!(shown(&runner, &mut host), ["b", "a", "c"]);
    // The dragged icon is under the pointer, held where it was taken.
    assert_eq!(drawn(&runner, &mut host, "a").center(), over_b);
    // b slides into the first place.
    settle(&mut host);
    assert_eq!(drawn(&runner, &mut host, "b"), rest.icons[0]);

    release(&mut runner, &mut host);
    assert_eq!(runner.app().edits, ["move a 1"]);
    assert!(runner.app().clicked.is_empty());
    // The app moved it: it slides from where it was dropped into its new place.
    assert_eq!(shown(&runner, &mut host), ["b", "a", "c"]);
    assert_eq!(drawn(&runner, &mut host, "a").center(), over_b);
    settle(&mut host);
    assert_eq!(drawn(&runner, &mut host, "a"), rest.icons[1]);
}

#[test]
fn a_refused_move_slides_back() {
    let (mut runner, mut host) = setup_three();
    runner.app_mut().refuse = true;
    let rest = DockLayout::default().rest(3);
    let a = rest.icons[0].center();
    drag(&mut runner, &mut host, a, &[rest.icons[2].center()]);
    assert_eq!(shown(&runner, &mut host), ["b", "c", "a"]);
    release(&mut runner, &mut host);
    assert_eq!(runner.app().edits, ["move a 2"]);
    assert_eq!(shown(&runner, &mut host), ["a", "b", "c"]);
    assert_ne!(drawn(&runner, &mut host, "a"), rest.icons[0]);
    settle(&mut host);
    assert_eq!(drawn(&runner, &mut host, "a"), rest.icons[0]);
}

#[test]
fn dragged_off_the_dock_a_tile_is_removed() {
    let (mut runner, mut host) = setup_three();
    let layout = DockLayout::default();
    let rest = layout.rest(3);
    let c = rest.icons[2].center();
    let band_top = layout.size(3).height - layout.band();
    // Just above the shelf it is still on the dock; well above, off it.
    drag(
        &mut runner,
        &mut host,
        c,
        &[Point::new(c.x, band_top - 5.0)],
    );
    assert_eq!(shown(&runner, &mut host), ["a", "b", "c"]);
    drag_on(&mut runner, &mut host, &[Point::new(c.x, -100.0)]);
    assert_eq!(shown(&runner, &mut host), ["a", "b"]);
    // The others close up in the middle; the dragged icon stays inside the surface.
    settle(&mut host);
    let middle = layout.size(3).width / 2.0;
    let (a, b) = (
        drawn(&runner, &mut host, "a"),
        drawn(&runner, &mut host, "b"),
    );
    assert!(((a.x0 + b.x1) / 2.0 - middle).abs() < 1e-9);
    assert_eq!(drawn(&runner, &mut host, "c").y0, 0.0);
    release(&mut runner, &mut host);
    assert_eq!(runner.app().edits, ["remove c"]);
    assert_eq!(shown(&runner, &mut host), ["a", "b"]);
    // The surface shrank by a tile around the same middle: nothing jumps.
    assert_eq!(drawn(&runner, &mut host, "a"), layout.rest(2).icons[0]);
}

#[test]
fn a_secondary_click_asks_for_the_menu_and_it_opens_above_the_icon() {
    let (mut runner, mut host) = setup_three();
    let rest = DockLayout::default().rest(3);
    let b = at(
        &runner,
        &mut host,
        rest.icons[1].center().x,
        rest.icons[1].center().y,
    );
    host.only().mouse_move(b);
    host.only()
        .mouse_button_press(Some(PointerButton::Secondary));
    host.only()
        .mouse_button_release(Some(PointerButton::Secondary));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().edits, ["menu b"]);
    assert_eq!(super::menus::menus_open(&mut host), 1);
    // It asks to go centred above the icon, the gap away.
    let tile = dock(&runner, &mut host, DockWidget::tile_ids)[1];
    let request = host.only().render_root().edit_widget(tile, |w| {
        let menu = w.ctx.get_attached_layer::<MenuWidget>()?;
        Some(menu)
    });
    let menu = request.expect("the tile owns the menu");
    let PopoverAnchor(Some(request)) = *host
        .only()
        .get_widget_with_id(menu)
        .get_prop::<PopoverAnchor>()
    else {
        panic!("no placement request");
    };
    assert_eq!(request.side, Side::Top);
    let icon = Rect::from_origin_size(
        at(&runner, &mut host, rest.icons[1].x0, rest.icons[1].y0),
        rest.icons[1].size(),
    );
    assert_eq!(request.anchor, icon.inflate(MENU_GAP, MENU_GAP));
    super::menus::click_item(&mut runner, &mut host, "Quit");
    assert_eq!(runner.app().edits, ["menu b", "quit b"]);
}

#[test]
fn the_menu_key_and_assistive_technology_ask_for_the_menu() {
    let (mut runner, mut host) = setup_three();
    let tiles = dock(&runner, &mut host, DockWidget::tile_ids);
    let node = host.only().access_node(tiles[0]).expect("a node");
    assert!(node.data().supports_action(Action::ShowContextMenu));
    host.only().focus_on(Some(tiles[0]));
    super::menus::named(&mut runner, &mut host, NamedKey::ContextMenu);
    assert_eq!(super::menus::menus_open(&mut host), 1);
    // Opened from the keyboard, the first item is highlighted.
    assert_eq!(
        super::menus::focused(&mut host).as_deref(),
        Some("Keep in Dock")
    );
    super::menus::named(&mut runner, &mut host, NamedKey::Enter);
    super::menus::a11y(&mut host, tiles[2].into(), Action::ShowContextMenu);
    deliver(&mut runner, &mut host);
    assert_eq!(super::menus::menus_open(&mut host), 1);
    assert_eq!(runner.app().edits, ["menu a", "keep a", "menu c"]);
}

/// Moves the pointer on to each point of `path` (in the dock), the button still down.
fn drag_on(runner: &mut Runner<Shelf>, host: &mut FakeHost, path: &[Point]) {
    for p in path {
        let p = at(runner, host, p.x, p.y);
        host.only().mouse_move(p);
    }
    deliver(runner, host);
}

/// Two apps, then a divider, a minimised window and the Trash (fixed): a, b, divider,
/// window and trash.
fn setup_sections() -> (Runner<Shelf>, FakeHost) {
    let special = |key: &str, kind, fixed| DockTile {
        kind,
        fixed,
        ..tile(key, false, DockBounce::None)
    };
    setup_shelf(
        Shelf {
            tiles: vec![
                tile("a", false, DockBounce::None),
                tile("b", false, DockBounce::None),
                special("divider", DockTileKind::Divider, false),
                special("window", DockTileKind::Minimised, false),
                special("trash", DockTileKind::App, true),
            ],
            ..Shelf::default()
        },
        false,
    )
}

#[test]
fn the_divider_takes_a_slot_but_no_pointer_and_no_focus() {
    let (mut runner, mut host) = setup_sections();
    let rest = DockLayout::default().rest(5);
    let divider = rest.icons[2].center();
    let point = at(&runner, &mut host, divider.x, divider.y);
    click_at(&mut runner, &mut host, point);
    let window = rest.icons[3].center();
    let point = at(&runner, &mut host, window.x, window.y);
    click_at(&mut runner, &mut host, point);
    let trash = rest.icons[4].center();
    let point = at(&runner, &mut host, trash.x, trash.y);
    click_at(&mut runner, &mut host, point);
    assert_eq!(runner.app().clicked, ["window", "trash"]);

    // Tab skips the divider: a, b, window, trash.
    let tiles = dock(&runner, &mut host, DockWidget::tile_ids);
    host.only().focus_on(Some(tiles[1]));
    tap(host.only(), &Key::Named(NamedKey::Tab));
    assert_eq!(host.only().focused_widget_id(), Some(tiles[3]));
    assert_eq!(
        host.only().access_node(tiles[2]).map(|n| n.role()),
        Some(Role::GenericContainer)
    );
    assert_eq!(
        host.only().access_node(tiles[3]).map(|n| n.role()),
        Some(Role::Button)
    );
}

#[test]
fn fixed_tiles_are_clicked_but_never_dragged() {
    let (mut runner, mut host) = setup_sections();
    let rest = DockLayout::default().rest(5);
    for (i, key) in [(3, "window"), (4, "trash")] {
        let from = rest.icons[i].center();
        // Far enough along and off the dock to be a drag of an app.
        drag(
            &mut runner,
            &mut host,
            from,
            &[from + Vec2::new(-80.0, 0.0), from + Vec2::new(-80.0, -60.0)],
        );
        assert_eq!(
            shown(&runner, &mut host),
            ["a", "b", "divider", "window", "trash"]
        );
        release(&mut runner, &mut host);
        assert!(runner.app().edits.is_empty(), "{key}");
        // Released off its tile: no click either.
        assert!(runner.app().clicked.is_empty(), "{key}");
    }
    // An app is dragged past them: it lands among the section.
    let a = rest.icons[0].center();
    drag(&mut runner, &mut host, a, &[rest.icons[4].center()]);
    assert_eq!(
        shown(&runner, &mut host),
        ["b", "divider", "window", "trash", "a"]
    );
    release(&mut runner, &mut host);
    assert_eq!(runner.app().edits, ["move a 4"]);
}

#[test]
fn a_secondary_click_on_the_trash_asks_for_its_menu() {
    let (mut runner, mut host) = setup_sections();
    let rest = DockLayout::default().rest(5);
    let trash = rest.icons[4].center();
    let point = at(&runner, &mut host, trash.x, trash.y);
    host.only().mouse_move(point);
    host.only()
        .mouse_button_press(Some(PointerButton::Secondary));
    host.only()
        .mouse_button_release(Some(PointerButton::Secondary));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().edits, ["menu trash"]);
    // The divider has none.
    let divider = rest.icons[2].center();
    let point = at(&runner, &mut host, divider.x, divider.y);
    host.only().mouse_move(point);
    host.only()
        .mouse_button_press(Some(PointerButton::Secondary));
    host.only()
        .mouse_button_release(Some(PointerButton::Secondary));
    deliver(&mut runner, &mut host);
    assert_eq!(runner.app().edits, ["menu trash"]);
}
