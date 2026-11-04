//! The 2.19c source list end to end on the fake host: group headers that take no selection,
//! keys that skip them, Show and Hide, items that nest, what assistive technology sees, and
//! changes from the app.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use masonry_core::accesskit::{Action, Role};
use masonry_core::core::WidgetId;
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::kurbo::Point;
use neue_theme::materials::sidebar::{ACTION_WIDTH, DISCLOSURE_WIDTH, ITEM_INSET, ROW_HEIGHT};

use super::table::{body, body_id, click_with, in_body, key, key_with, selected};
use super::*;
use crate::{Entry, Glyph, SourceList};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Selected(Option<u32>),
    Activated(u32),
    /// Script: the app changes the list.
    Select(Option<u32>),
    Expand(u32, bool),
    Reload,
}

/// Favorites (11 Documents, 12 Pictures, 13 Music (131 Album)), Devices (21 Disk, 3 on its
/// badge), Tags (no items).
fn label(id: u32) -> &'static str {
    match id {
        1 => "Favorites",
        11 => "Documents",
        12 => "Pictures",
        13 => "Music",
        131 => "Album",
        2 => "Devices",
        21 => "Disk",
        3 => "Tags",
        _ => "New",
    }
}

#[derive(Default)]
struct Sidebar {
    list: Handle<SourceList<Msg, u32>>,
    children: Rc<RefCell<HashMap<u32, Vec<u32>>>>,
    log: Vec<Msg>,
}

impl Application for Sidebar {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        *self.children.borrow_mut() = HashMap::from([
            (0, vec![1, 2, 3]),
            (1, vec![11, 12, 13]),
            (13, vec![131]),
            (2, vec![21]),
        ]);
        let tree = Rc::clone(&self.children);
        let list = SourceList::new(
            move |parent: Option<&u32>| {
                tree.borrow()
                    .get(parent.unwrap_or(&0))
                    .cloned()
                    .unwrap_or_default()
            },
            |&id| {
                let entry = Entry::new(label(id));
                match id {
                    11 => entry.icon(Glyph::Document),
                    21 => entry.icon(Glyph::Folder).badge(3),
                    _ => entry,
                }
            },
        )
        .on_select(|id| Msg::Selected(id.copied()))
        .on_activate(|&id| Msg::Activated(id))
        .width(180.0)
        .height(176.0)
        .bind(&mut self.list);
        cx.open_window(Window::new("Sidebar", (200, 196), list));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Select(id) => self.list.set_selection(cx, id),
            Msg::Expand(id, expanded) => self.list.set_expanded(cx, &id, expanded),
            Msg::Reload => self.list.reload(cx),
            _ => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Sidebar>, FakeHost) {
    let (mut runner, mut host, _) = start(Sidebar::default());
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn take_log(runner: &mut Runner<Sidebar>) -> Vec<Msg> {
    std::mem::take(&mut runner.app_mut().log)
}

fn list(runner: &Runner<Sidebar>) -> WidgetId {
    id(&runner.app().list)
}

/// The window point at `x` in the middle of row `row`.
fn at(host: &mut FakeHost, list: WidgetId, row: usize, x: f64) -> Point {
    #[expect(clippy::cast_precision_loss, reason = "small")]
    let y = (row as f64 + 0.5) * ROW_HEIGHT;
    in_body(host, list, x, y)
}

fn click(runner: &mut Runner<Sidebar>, host: &mut FakeHost, at: Point) {
    click_with(runner, host, at, Modifiers::empty(), 1);
}

/// Click row `row` on its label.
fn click_row(runner: &mut Runner<Sidebar>, host: &mut FakeHost, row: usize) {
    let l = list(runner);
    let p = at(host, l, row, 60.0);
    click(runner, host, p);
}

/// The labels of the rows that exist, from the tree items' nodes.
fn labels(host: &mut FakeHost, list: WidgetId) -> Vec<String> {
    let _ = host.only().render();
    let rows = body(host, list).built();
    rows.iter()
        .map(|&i| {
            let row = body(host, list).row_id(i).expect("built");
            host.only()
                .access_node(row)
                .and_then(|n| n.label())
                .unwrap_or_default()
        })
        .collect()
}

/// Tabs into the list.
fn focus(runner: &mut Runner<Sidebar>, host: &mut FakeHost) {
    let l = list(runner);
    let body = body_id(host, l);
    host.only().focus_on(Some(body));
}

fn row_count(host: &mut FakeHost, list: WidgetId) -> usize {
    let node = body_id(host, list);
    let _ = host.only().render();
    host.only()
        .access_node(node)
        .and_then(|n| n.data().row_count())
        .expect("a row count")
}

#[test]
fn it_starts_with_every_group_open_and_the_items_nested_under_them() {
    let (runner, mut host) = setup();
    let l = list(&runner);
    assert_eq!(row_count(&mut host, l), 7);
    assert_eq!(
        labels(&mut host, l),
        [
            "Favorites",
            "Documents",
            "Pictures",
            "Music",
            "Devices",
            "Disk",
            "Tags"
        ]
    );
}

#[test]
fn a_click_selects_an_item_but_never_a_group_header() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    click_row(&mut runner, &mut host, 0);
    assert!(selected(&mut host, l).is_empty());
    assert!(take_log(&mut runner).is_empty());
    click_row(&mut runner, &mut host, 2);
    assert_eq!(take_log(&mut runner), [Msg::Selected(Some(12))]);
    assert_eq!(selected(&mut host, l), [2]);
    // A group header, a press that drags down onto one, and empty space leave it alone.
    click_row(&mut runner, &mut host, 4);
    let blank = at(&mut host, l, 9, 60.0);
    click(&mut runner, &mut host, blank);
    assert_eq!(selected(&mut host, l), [2]);
    assert!(take_log(&mut runner).is_empty());
    click_row(&mut runner, &mut host, 5);
    assert_eq!(take_log(&mut runner), [Msg::Selected(Some(21))]);
}

#[test]
fn up_and_down_move_from_item_to_item_skipping_group_headers() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    focus(&mut runner, &mut host);
    key(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(selected(&mut host, l), [1], "the first item, not Favorites");
    key(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(selected(&mut host, l), [1], "nothing above it to go to");
    for _ in 0..3 {
        key(&mut runner, &mut host, NamedKey::ArrowDown);
    }
    assert_eq!(selected(&mut host, l), [5], "Devices is stepped over");
    key(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(selected(&mut host, l), [5], "Tags has no items");
    key(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(selected(&mut host, l), [3]);
    key(&mut runner, &mut host, NamedKey::Home);
    assert_eq!(selected(&mut host, l), [1]);
    key(&mut runner, &mut host, NamedKey::End);
    assert_eq!(selected(&mut host, l), [5]);
    assert_eq!(
        take_log(&mut runner),
        [
            Msg::Selected(Some(11)),
            Msg::Selected(Some(12)),
            Msg::Selected(Some(13)),
            Msg::Selected(Some(21)),
            Msg::Selected(Some(13)),
            Msg::Selected(Some(11)),
            Msg::Selected(Some(21)),
        ]
    );
}

#[test]
fn shift_does_not_extend_a_single_selection() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    click_row(&mut runner, &mut host, 1);
    key_with(
        &mut runner,
        &mut host,
        Key::Named(NamedKey::ArrowDown),
        Modifiers::SHIFT,
    );
    assert_eq!(selected(&mut host, l), [2]);
}

#[test]
fn typing_selects_the_first_item_that_matches_but_not_a_group() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    focus(&mut runner, &mut host);
    key_with(
        &mut runner,
        &mut host,
        Key::Character("m".into()),
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, l), [3], "Music");
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    focus(&mut runner, &mut host);
    key_with(
        &mut runner,
        &mut host,
        Key::Character("t".into()),
        Modifiers::empty(),
    );
    assert!(selected(&mut host, l).is_empty(), "Tags is a group");
}

#[test]
fn show_and_hide_collapse_a_group_and_drop_the_selection_it_hides() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    click_row(&mut runner, &mut host, 2);
    take_log(&mut runner);
    // The right end of Favorites' row is its Hide.
    let hide = at(&mut host, l, 0, 180.0 - ACTION_WIDTH / 2.0);
    click(&mut runner, &mut host, hide);
    assert_eq!(row_count(&mut host, l), 4);
    assert_eq!(
        labels(&mut host, l),
        ["Favorites", "Devices", "Disk", "Tags"]
    );
    assert!(
        selected(&mut host, l).is_empty(),
        "the group can't take it instead"
    );
    assert_eq!(take_log(&mut runner), [Msg::Selected(None)]);
    // Show brings the items back.
    click(&mut runner, &mut host, hide);
    assert_eq!(row_count(&mut host, l), 7);
    // A click on the title of a group is only that: nothing happens.
    let title = at(&mut host, l, 0, 20.0);
    click(&mut runner, &mut host, title);
    assert_eq!(row_count(&mut host, l), 7);
    // A group with no items has no Show or Hide to press.
    let tags = at(&mut host, l, 6, 180.0 - ACTION_WIDTH / 2.0);
    click(&mut runner, &mut host, tags);
    assert_eq!(row_count(&mut host, l), 7);
}

#[test]
fn the_pointer_over_a_group_header_shows_its_action() {
    let (runner, mut host) = setup();
    let l = list(&runner);
    let p = at(&mut host, l, 0, 40.0);
    host.only().mouse_move(p);
    assert_eq!(
        body(&mut host, l).table.as_ref().unwrap().hovered(),
        Some(0)
    );
    let p = at(&mut host, l, 1, 40.0);
    host.only().mouse_move(p);
    assert_eq!(
        body(&mut host, l).table.as_ref().unwrap().hovered(),
        None,
        "an item has none"
    );
    let p = at(&mut host, l, 6, 40.0);
    host.only().mouse_move(p);
    assert_eq!(
        body(&mut host, l).table.as_ref().unwrap().hovered(),
        None,
        "nor a group without items"
    );
}

#[test]
fn items_with_items_open_with_their_triangle_and_the_keys() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    // Music is row 3: a press on the triangle left of its icon opens it, and selects nothing.
    let triangle = at(&mut host, l, 3, ITEM_INSET - DISCLOSURE_WIDTH / 2.0);
    click(&mut runner, &mut host, triangle);
    assert_eq!(row_count(&mut host, l), 8);
    assert!(selected(&mut host, l).is_empty());
    assert_eq!(labels(&mut host, l)[4], "Album");
    // Select it, and Left closes it again; Left on a closed item at the first level has no
    // group to go to.
    click_row(&mut runner, &mut host, 3);
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(row_count(&mut host, l), 7);
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(selected(&mut host, l), [3], "not into Favorites");
    // Right opens, and again goes to the first nested item.
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(selected(&mut host, l), [4]);
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(selected(&mut host, l), [3], "Left goes to Music");
}

#[test]
fn return_and_double_click_activate_the_item() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    click_row(&mut runner, &mut host, 2);
    key(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(Some(12)), Msg::Activated(12)]
    );
    let p = at(&mut host, l, 5, 60.0);
    click_with(&mut runner, &mut host, p, Modifiers::empty(), 2);
    assert_eq!(take_log(&mut runner).last(), Some(&Msg::Activated(21)));
}

#[test]
fn assistive_technology_sees_a_tree_of_items_with_labels_badges_and_levels() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    click_row(&mut runner, &mut host, 5);
    let _ = host.only().render();
    let node = body_id(&mut host, l);
    assert_eq!(
        host.only().access_node(node).expect("body").role(),
        Role::Tree
    );
    let group = body(&mut host, l).row_id(0).expect("built");
    let node = host.only().access_node(group).expect("the group");
    assert_eq!(node.role(), Role::TreeItem);
    assert_eq!(node.label().as_deref(), Some("Favorites"));
    let data = node.data();
    assert_eq!(
        (data.level(), data.is_expanded(), data.is_selected()),
        (Some(1), Some(true), Some(false))
    );
    let disk = body(&mut host, l).row_id(5).expect("built");
    let node = host.only().access_node(disk).expect("the item");
    assert_eq!(node.label().as_deref(), Some("Disk"));
    assert_eq!(node.data().description(), Some("3"), "the badge");
    assert_eq!(
        (node.data().level(), node.data().is_selected()),
        (Some(2), Some(true))
    );
    // Collapsing from assistive technology, and clicking a group does not select it.
    a11y_action(host.only(), group, Action::Collapse);
    deliver(&mut runner, &mut host);
    assert_eq!(row_count(&mut host, l), 4);
    let group = body(&mut host, l).row_id(0).expect("built");
    a11y_action(host.only(), group, Action::Click);
    deliver(&mut runner, &mut host);
    assert_eq!(selected(&mut host, l), [2], "Disk is still the selection");
}

#[test]
fn the_app_selects_expands_and_reloads_silently_keeping_both_by_node() {
    let (mut runner, mut host) = setup();
    let l = list(&runner);
    runner.send(&mut host, Msg::Select(Some(21)));
    assert_eq!(selected(&mut host, l), [5]);
    runner.send(&mut host, Msg::Expand(1, false));
    assert_eq!(row_count(&mut host, l), 4);
    assert_eq!(selected(&mut host, l), [2], "Disk moved up");
    runner.app().children.borrow_mut().insert(2, vec![22, 21]);
    runner.send(&mut host, Msg::Reload);
    assert_eq!(row_count(&mut host, l), 5);
    assert_eq!(selected(&mut host, l), [3], "and follows its node");
    runner.send(&mut host, Msg::Select(None));
    assert!(selected(&mut host, l).is_empty());
    assert!(
        take_log(&mut runner).is_empty(),
        "the app's changes send nothing"
    );
}
