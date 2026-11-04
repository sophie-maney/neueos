//! The 2.19b outline view end to end on the fake host: expanding by a click on a
//! triangle, by keys and by assistive technology; the selection following its rows; and
//! changes from the app.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use masonry_core::accesskit::{Action, Role};
use masonry_core::core::WidgetId;
use masonry_core::core::keyboard::{Modifiers, NamedKey};
use masonry_core::kurbo::Point;
use neue_theme::materials::table::ROW_HEIGHT;

use super::table::{body, body_id, click_with, in_body, key, key_with, row_point, selected};
use super::*;
use crate::{Cell, Column, Outline};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Selected(Vec<u32>),
    Activated(u32),
    /// Script: the app changes the outline.
    Expand(u32, bool),
    Select(Vec<u32>),
    Reload,
}

/// Documents (11 Work (111 Plan, 112 Budget), 12 Letters), Music (21 Album), notes.txt.
type Children = Rc<RefCell<HashMap<u32, Vec<u32>>>>;

fn names(id: u32) -> &'static str {
    match id {
        1 => "Documents",
        11 => "Work",
        111 => "Plan",
        112 => "Budget",
        12 => "Letters",
        2 => "Music",
        21 => "Album",
        3 => "notes.txt",
        _ => "New",
    }
}

#[derive(Default)]
struct Outlining {
    outline: Handle<Outline<Msg, u32>>,
    children: Children,
    log: Vec<Msg>,
}

impl Application for Outlining {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        *self.children.borrow_mut() = HashMap::from([
            (0, vec![1, 2, 3]),
            (1, vec![11, 12]),
            (11, vec![111, 112]),
            (2, vec![21]),
        ]);
        let tree = Rc::clone(&self.children);
        let outline = Outline::new(
            [
                Column::new("Name").width(120.0),
                Column::new("Kind").width(50.0),
            ],
            move |parent: Option<&u32>| {
                tree.borrow()
                    .get(parent.unwrap_or(&0))
                    .cloned()
                    .unwrap_or_default()
            },
            |&id, column| -> Cell<Msg> {
                match column {
                    0 => names(id).into(),
                    _ => "Item".into(),
                }
            },
        )
        .multiple(true)
        .on_select(|ids| Msg::Selected(ids.to_vec()))
        .on_activate(|&id| Msg::Activated(id))
        .width(200.0)
        .height(160.0)
        .bind(&mut self.outline);
        cx.open_window(Window::new("Outline", (220, 180), outline));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::Expand(id, expanded) => self.outline.set_expanded(cx, &id, expanded),
            Msg::Select(ids) => self.outline.set_selection(cx, ids),
            Msg::Reload => self.outline.reload(cx),
            _ => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Outlining>, FakeHost) {
    let (mut runner, mut host, _) = start(Outlining::default());
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn take_log(runner: &mut Runner<Outlining>) -> Vec<Msg> {
    std::mem::take(&mut runner.app_mut().log)
}

fn outline(runner: &Runner<Outlining>) -> WidgetId {
    id(&runner.app().outline)
}

/// How many rows the body's node says there are.
fn row_count(host: &mut FakeHost, outline: WidgetId) -> usize {
    let node = body_id(host, outline);
    let _ = host.only().render();
    host.only()
        .access_node(node)
        .and_then(|n| n.data().row_count())
        .expect("a row count")
}

/// The window point on row `row`'s disclosure triangle (the top level's).
fn triangle(host: &mut FakeHost, outline: WidgetId, row: usize, level: usize) -> Point {
    #[expect(clippy::cast_precision_loss, reason = "small")]
    let (x, y) = (
        4.0 + level as f64 * 14.0 + 7.0,
        (row as f64 + 0.5) * ROW_HEIGHT,
    );
    in_body(host, outline, x, y)
}

fn click(runner: &mut Runner<Outlining>, host: &mut FakeHost, at: Point, mods: Modifiers) {
    click_with(runner, host, at, mods, 1);
}

fn texts(host: &mut FakeHost, outline: WidgetId) -> Vec<String> {
    let _ = host.only().render();
    let rows = body(host, outline).built();
    rows.iter()
        .map(|&i| {
            let row = body(host, outline).row_id(i).expect("built");
            let node = host.only().access_node(row).expect("a row node");
            node.children()
                .next()
                .and_then(|c| c.label())
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn it_starts_collapsed_and_a_click_on_a_triangle_expands_without_selecting() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    assert_eq!(row_count(&mut host, o), 3);
    assert_eq!(texts(&mut host, o), ["Documents", "Music", "notes.txt"]);
    let at = triangle(&mut host, o, 0, 0);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(row_count(&mut host, o), 5);
    assert_eq!(
        texts(&mut host, o),
        ["Documents", "Work", "Letters", "Music", "notes.txt"]
    );
    assert!(
        selected(&mut host, o).is_empty(),
        "the selection is untouched"
    );
    assert!(take_log(&mut runner).is_empty(), "and nothing is sent");
    // Nested: open Work, one level in.
    let at = triangle(&mut host, o, 1, 1);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(row_count(&mut host, o), 7);
    // And closed again.
    let at = triangle(&mut host, o, 0, 0);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(row_count(&mut host, o), 3);
    let at = triangle(&mut host, o, 0, 0);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(row_count(&mut host, o), 7, "Work is still open inside");
}

#[test]
fn an_option_click_opens_every_level_below() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    let at = triangle(&mut host, o, 0, 0);
    click(&mut runner, &mut host, at, Modifiers::ALT);
    assert_eq!(
        texts(&mut host, o),
        [
            "Documents",
            "Work",
            "Plan",
            "Budget",
            "Letters",
            "Music",
            "notes.txt"
        ]
    );
    click(&mut runner, &mut host, at, Modifiers::ALT);
    assert_eq!(
        row_count(&mut host, o),
        3,
        "on an open row it closes everything below"
    );
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(
        row_count(&mut host, o),
        5,
        "which is forgotten: Work is closed again"
    );
    click(&mut runner, &mut host, at, Modifiers::ALT);
    assert_eq!(row_count(&mut host, o), 3);
    click(&mut runner, &mut host, at, Modifiers::ALT);
    assert_eq!(row_count(&mut host, o), 7);
    // A click on a leaf's text selects it; there's no triangle to press.
    let at = row_point(&mut host, o, 2);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![111])]);
}

#[test]
fn right_and_left_open_close_and_move_through_the_tree() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    let at = row_point(&mut host, o, 0);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![1])]);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(row_count(&mut host, o), 5, "Right opens");
    assert_eq!(selected(&mut host, o), [0], "on the same row");
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(
        selected(&mut host, o),
        [1],
        "Right again goes to the first child"
    );
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(
        selected(&mut host, o),
        [0],
        "Left on a closed row goes to the parent"
    );
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(row_count(&mut host, o), 3, "Left on an open row closes it");
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![11]), Msg::Selected(vec![1])]
    );
    key_with(
        &mut runner,
        &mut host,
        masonry_core::core::keyboard::Key::Named(NamedKey::ArrowRight),
        Modifiers::ALT,
    );
    assert_eq!(row_count(&mut host, o), 7, "Option-Right opens everything");
    key_with(
        &mut runner,
        &mut host,
        masonry_core::core::keyboard::Key::Named(NamedKey::ArrowLeft),
        Modifiers::ALT,
    );
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(
        row_count(&mut host, o),
        5,
        "Option-Left closed Work too, so it is shut again"
    );
    // A leaf: Right does nothing, Left goes to the parent (Music's Album, on the next row).
    let at = row_point(&mut host, o, 4);
    click(&mut runner, &mut host, at, Modifiers::empty());
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(selected(&mut host, o), [4]);
}

#[test]
fn the_selection_stays_on_its_rows_and_a_collapse_that_hides_it_selects_the_parent() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    // Select Music (row 1), then open Documents above it: Music moves, nothing is sent.
    let at = row_point(&mut host, o, 1);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![2])]);
    let at = triangle(&mut host, o, 0, 0);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(selected(&mut host, o), [3]);
    assert!(take_log(&mut runner).is_empty());
    // Select Letters (row 2), then close Documents: it is hidden, so Documents is selected.
    let at = row_point(&mut host, o, 2);
    click(&mut runner, &mut host, at, Modifiers::empty());
    let at = triangle(&mut host, o, 0, 0);
    click(&mut runner, &mut host, at, Modifiers::empty());
    assert_eq!(selected(&mut host, o), [0]);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![12]), Msg::Selected(vec![1])]
    );
}

#[test]
fn assistive_technology_sees_a_tree_grid_and_can_expand_and_collapse() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    let node_id = body_id(&mut host, o);
    let _ = host.only().render();
    let node = host.only().access_node(node_id).expect("the body");
    assert_eq!(node.role(), Role::TreeGrid);
    assert_eq!(node.data().row_count(), Some(3));
    let row = body(&mut host, o).row_id(0).expect("built");
    let node = host.only().access_node(row).expect("a row");
    assert_eq!(node.role(), Role::Row);
    let data = node.data();
    assert_eq!(data.level(), Some(1));
    assert_eq!(data.is_expanded(), Some(false));
    assert_eq!(
        (data.size_of_set(), data.position_in_set()),
        (Some(3), Some(1))
    );
    let leaf = body(&mut host, o).row_id(2).expect("built");
    let node = host.only().access_node(leaf).expect("the leaf");
    assert_eq!(node.data().is_expanded(), None, "a leaf has no state");
    a11y_action(host.only(), row, Action::Expand);
    deliver(&mut runner, &mut host);
    let _ = host.only().render();
    let row = body(&mut host, o).row_id(0).expect("rebuilt");
    assert_eq!(
        host.only().access_node(row).unwrap().data().is_expanded(),
        Some(true)
    );
    let child = body(&mut host, o).row_id(1).expect("a child");
    let data = host.only().access_node(child).unwrap().data().clone();
    assert_eq!(
        (data.level(), data.size_of_set(), data.position_in_set()),
        (Some(2), Some(2), Some(1))
    );
    a11y_action(host.only(), row, Action::Collapse);
    deliver(&mut runner, &mut host);
    assert_eq!(row_count(&mut host, o), 3);
}

#[test]
fn the_app_expands_selects_and_reloads_silently_keeping_both_by_node() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    runner.send(&mut host, Msg::Expand(1, true));
    assert_eq!(row_count(&mut host, o), 5);
    runner.send(&mut host, Msg::Select(vec![2, 999]));
    assert_eq!(selected(&mut host, o), [3], "999 isn't a row");
    // The data changes: Documents gets a new first child; the selection follows Music.
    runner
        .app()
        .children
        .borrow_mut()
        .insert(1, vec![13, 11, 12]);
    runner.send(&mut host, Msg::Reload);
    assert_eq!(row_count(&mut host, o), 6);
    assert_eq!(selected(&mut host, o), [4]);
    // Everything under a node that disappears goes with it.
    runner.app().children.borrow_mut().insert(0, vec![2, 3]);
    runner.send(&mut host, Msg::Reload);
    assert_eq!(row_count(&mut host, o), 2);
    assert_eq!(selected(&mut host, o), [0]);
    // A node that is hidden (or gone) remembers it was open, and comes back so.
    runner.app().children.borrow_mut().insert(0, vec![1, 2, 3]);
    runner.send(&mut host, Msg::Expand(11, true));
    runner.send(&mut host, Msg::Reload);
    assert_eq!(
        row_count(&mut host, o),
        8,
        "Documents and Work were open (13 is new)"
    );
    assert!(
        take_log(&mut runner).is_empty(),
        "the app's changes send nothing"
    );
}

#[test]
fn return_and_double_click_activate_the_node() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    let at = row_point(&mut host, o, 2);
    click(&mut runner, &mut host, at, Modifiers::empty());
    key(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![3]), Msg::Activated(3)]
    );
}

#[test]
fn typing_selects_among_the_rows_that_show() {
    let (mut runner, mut host) = setup();
    let o = outline(&runner);
    let focus = body_id(&mut host, o);
    host.only().focus_on(Some(focus));
    key_with(
        &mut runner,
        &mut host,
        masonry_core::core::keyboard::Key::Character("w".into()),
        Modifiers::empty(),
    );
    assert!(
        selected(&mut host, o).is_empty(),
        "Work is hidden in Documents"
    );
    runner.send(&mut host, Msg::Expand(1, true));
    std::thread::sleep(std::time::Duration::from_millis(1100));
    key_with(
        &mut runner,
        &mut host,
        masonry_core::core::keyboard::Key::Character("w".into()),
        Modifiers::empty(),
    );
    assert_eq!(selected(&mut host, o), [1]);
}
