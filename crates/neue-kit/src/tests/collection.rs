//! The 2.20c collection view end to end on the fake host: the grid of cells, selection by
//! click, Shift and Cmd, the arrow keys over the grid, type-select, activation, changes
//! from the app, and what assistive technology sees.

use masonry_core::accesskit::Role;
use masonry_core::core::WidgetId;
use masonry_core::core::WindowEvent;
use masonry_core::core::keyboard::{Key, Modifiers, NamedKey};
use masonry_core::dpi::PhysicalSize;
use masonry_core::kurbo::{Point, Vec2};
use neue_theme::materials::collection::cell_size;

use super::table::{body, body_id, click_with, command, in_body, key, key_with, scroll};
use super::*;
use crate::{Collection, Glyph, Item, Label, Stack};

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Selected(Vec<u32>),
    Activated(u32),
    /// Script: the app changes the collection.
    SetItems(Vec<u32>),
    Select(Vec<u32>),
    ScrollTo(u32),
    IconSize(f64),
    Enabled(bool),
}

/// Item `i` has the id `100 + i`; labels are "Item 05", and the first three are fruit.
fn label(id: u32) -> String {
    match id - 100 {
        0 => "Apple".to_owned(),
        1 => "Banana".to_owned(),
        3 => "Cherry".to_owned(),
        i => format!("Item {i:02}"),
    }
}

const ICON: f64 = 32.0;

#[derive(Default)]
struct Gridded {
    collection: Handle<Collection<Msg, u32>>,
    log: Vec<Msg>,
}

impl Application for Gridded {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        // 72 × 77 cells in a 296 px wide window: 4 columns; 30 items, the last row of 2.
        let collection =
            Collection::new(100..130, |&id| Item::new(label(id)).icon(Glyph::Document))
                .icon_size(ICON)
                .multiple(true)
                .on_select(|ids| Msg::Selected(ids.to_vec()))
                .on_activate(|&id| Msg::Activated(id))
                .height(200.0)
                .bind(&mut self.collection);
        cx.open_window(Window::new("Collection", (296, 200), collection));
    }

    fn update(&mut self, cx: &mut Cx<'_, Msg>, message: Msg) {
        match message {
            Msg::SetItems(ids) => self.collection.set_items(cx, ids),
            Msg::Select(ids) => self.collection.set_selection(cx, ids),
            Msg::ScrollTo(id) => self.collection.scroll_to(cx, &id),
            Msg::IconSize(size) => self.collection.set_icon_size(cx, size),
            Msg::Enabled(enabled) => self.collection.set_enabled(cx, enabled),
            _ => self.log.push(message),
        }
    }
}

fn setup() -> (Runner<Gridded>, FakeHost) {
    let (mut runner, mut host, _) = start(Gridded::default());
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    (runner, host)
}

fn take_log(runner: &mut Runner<Gridded>) -> Vec<Msg> {
    std::mem::take(&mut runner.app_mut().log)
}

fn collection(runner: &Runner<Gridded>) -> WidgetId {
    id(&runner.app().collection)
}

fn columns(host: &mut FakeHost, c: WidgetId) -> usize {
    body(host, c).columns
}

fn selection(host: &mut FakeHost, c: WidgetId) -> Vec<usize> {
    let _ = host.only().render();
    body(host, c)
        .collection
        .as_ref()
        .expect("a collection")
        .selection()
        .rows
        .into_iter()
        .collect()
}

/// The window point in the middle of item `i` (which must be built and on screen).
fn item_point(host: &mut FakeHost, c: WidgetId, i: usize) -> Point {
    let item = body(host, c).row_id(i).expect("built");
    let widget = host.only().get_widget_with_id(item);
    let ctx = widget.ctx();
    let size = ctx.border_box().size();
    ctx.to_window(Point::new(size.width / 2.0, size.height / 2.0))
}

fn click_item(runner: &mut Runner<Gridded>, host: &mut FakeHost, i: usize, mods: Modifiers) {
    let c = collection(runner);
    let at = item_point(host, c, i);
    click_with(runner, host, at, mods, 1);
}

#[test]
fn cells_follow_the_icon_size_and_fill_the_width() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    assert_eq!(columns(&mut host, c), 4);
    let first = item_point(&mut host, c, 0);
    let second = item_point(&mut host, c, 1);
    let below = item_point(&mut host, c, 4);
    assert!(
        second.x > first.x && (second.y - first.y).abs() < 1e-6,
        "a row"
    );
    assert_eq!(below.x, first.x, "the fifth item starts the second row");
    assert!((below.y - first.y - cell_size(ICON).1).abs() < 1e-6);

    runner.send(&mut host, Msg::IconSize(64.0));
    let _ = host.only().render();
    // 64 px icons: 80 px backdrops and 96 px cells fit 3 times in 296 px.
    assert_eq!(columns(&mut host, c), 3);
    let (width, height) = cell_size(64.0);
    assert!(width > cell_size(ICON).0 && height > cell_size(ICON).1);
}

#[test]
fn clicks_select_and_shift_and_command_extend_in_order() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    click_item(&mut runner, &mut host, 2, Modifiers::empty());
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![102])]);
    assert_eq!(
        host.only().focused_widget_id(),
        Some(body_id(&mut host, c)),
        "a click focuses the collection"
    );
    click_item(&mut runner, &mut host, 5, Modifiers::SHIFT);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![102, 103, 104, 105])]
    );
    click_item(&mut runner, &mut host, 7, command());
    assert_eq!(selection(&mut host, c), [2, 3, 4, 5, 7]);
    click_item(&mut runner, &mut host, 3, command());
    assert_eq!(selection(&mut host, c), [2, 4, 5, 7], "command toggles off");
    take_log(&mut runner);
    click_item(&mut runner, &mut host, 6, Modifiers::empty());
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![106])]);
}

#[test]
fn clicking_between_items_clears_the_selection() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    click_item(&mut runner, &mut host, 1, Modifiers::empty());
    runner.send(&mut host, Msg::SetItems(vec![100, 101]));
    let _ = host.only().render();
    take_log(&mut runner);
    // Row 0, fourth column: no item there any more.
    let at = in_body(&mut host, c, 3.5 * 74.0, 20.0);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 1);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![])]);
    // Shift or command on nothing keeps what is selected.
    click_item(&mut runner, &mut host, 0, Modifiers::empty());
    take_log(&mut runner);
    let at = in_body(&mut host, c, 3.5 * 74.0, 20.0);
    click_with(&mut runner, &mut host, at, command(), 1);
    assert_eq!(take_log(&mut runner), []);
    assert_eq!(selection(&mut host, c), [0]);
}

#[test]
fn arrow_keys_move_over_the_grid_and_home_and_end_go_to_the_ends() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    let focus = body_id(&mut host, c);
    host.only().focus_on(Some(focus));
    // Nothing selected: Down starts at the first item.
    key(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(selection(&mut host, c), [0]);
    key(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(selection(&mut host, c), [4], "a row down");
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(selection(&mut host, c), [5]);
    key(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(selection(&mut host, c), [1]);
    key(&mut runner, &mut host, NamedKey::ArrowUp);
    assert_eq!(selection(&mut host, c), [1], "the top row stays");
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(selection(&mut host, c), [0], "the first item stays");
    key(&mut runner, &mut host, NamedKey::End);
    assert_eq!(selection(&mut host, c), [29]);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(selection(&mut host, c), [29], "the last item stays");
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    assert_eq!(selection(&mut host, c), [26]);
    // Right goes on to the next row at the end of one.
    key(&mut runner, &mut host, NamedKey::ArrowLeft);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    assert_eq!(selection(&mut host, c), [27]);
    key(&mut runner, &mut host, NamedKey::Home);
    assert_eq!(selection(&mut host, c), [0]);
    // Down from item 27, under the short last row's end: its last item.
    for _ in 0..6 {
        key(&mut runner, &mut host, NamedKey::ArrowDown);
    }
    assert_eq!(selection(&mut host, c), [24]);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    key(&mut runner, &mut host, NamedKey::ArrowDown);
    assert_eq!(selection(&mut host, c), [29], "27 + 4 is past the end");
}

#[test]
fn shift_arrows_extend_and_the_keys_scroll_the_selection_into_view() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    click_item(&mut runner, &mut host, 1, Modifiers::empty());
    key_with(
        &mut runner,
        &mut host,
        Key::Named(NamedKey::ArrowDown),
        Modifiers::SHIFT,
    );
    assert_eq!(selection(&mut host, c), [1, 2, 3, 4, 5]);
    take_log(&mut runner);
    key(&mut runner, &mut host, NamedKey::End);
    let _ = host.only().render();
    assert!(scroll(&mut host, c).offset().y > 0.0, "scrolled to the end");
    assert!(body(&mut host, c).built().contains(&29));
    key_with(
        &mut runner,
        &mut host,
        Key::Character("a".into()),
        command(),
    );
    assert_eq!(selection(&mut host, c).len(), 30, "select all");
}

#[test]
fn the_columns_follow_the_window_width() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    assert_eq!(columns(&mut host, c), 4);
    for (width, expected) in [(224, 3), (440, 6), (296, 4)] {
        host.only()
            .render_root()
            .handle_window_event(WindowEvent::Resize(PhysicalSize::new(width, 200)));
        let _ = host.only().render();
        deliver(&mut runner, &mut host);
        assert_eq!(columns(&mut host, c), expected, "{width} px wide");
    }
}

#[test]
fn rows_scrolled_into_view_paint_the_content_background() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    // A 4 px scroll stays inside the rows already built, so nothing else repaints the
    // body: its background has to follow the window by itself. (The wheel changes no
    // selection either.)
    let at = item_point(&mut host, c, 0);
    host.only().mouse_move(at);
    host.only().mouse_wheel(Vec2::new(0.0, -4.0));
    for _ in 0..2 {
        let _ = host.only().render();
        deliver(&mut runner, &mut host);
    }
    assert_eq!(scroll(&mut host, c).offset().y, 4.0);
    // The bottom 4 px of the view are new, left of the cells' icons.
    let image = host.only().render();
    let (_, height) = image.dimensions();
    let pixel = image.get_pixel(1, height - 3).0;
    // The theme's light content background is white.
    assert_eq!(pixel, [255, 255, 255, 255], "newly visible space");
}

#[test]
fn typing_selects_the_first_item_whose_label_starts_with_it() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    click_item(&mut runner, &mut host, 5, Modifiers::empty());
    take_log(&mut runner);
    key_with(
        &mut runner,
        &mut host,
        Key::Character("c".into()),
        Modifiers::empty(),
    );
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![103])]);
    key_with(
        &mut runner,
        &mut host,
        Key::Character("h".into()),
        Modifiers::empty(),
    );
    assert_eq!(selection(&mut host, c), [3], "Ch: still Cherry");
}

#[test]
fn return_and_double_click_activate_the_selected_item() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    let at = item_point(&mut host, c, 6);
    click_with(&mut runner, &mut host, at, Modifiers::empty(), 2);
    assert_eq!(
        take_log(&mut runner),
        [Msg::Selected(vec![106]), Msg::Activated(106)]
    );
    key(&mut runner, &mut host, NamedKey::ArrowRight);
    take_log(&mut runner);
    key(&mut runner, &mut host, NamedKey::Enter);
    assert_eq!(take_log(&mut runner), [Msg::Activated(107)]);
}

#[test]
fn the_app_changes_items_selection_and_icon_size_without_messages() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    runner.send(&mut host, Msg::Select(vec![103, 125, 999]));
    assert_eq!(selection(&mut host, c), [3, 25], "ids that exist");
    // Reordered and shortened: the selection follows the ids; 103 is gone.
    runner.send(&mut host, Msg::SetItems(vec![125, 104, 100, 101, 102]));
    assert_eq!(selection(&mut host, c), [0], "125 moved to the front");
    assert_eq!(take_log(&mut runner), [], "no message");
    runner.send(&mut host, Msg::SetItems((100..130).collect()));
    runner.send(&mut host, Msg::ScrollTo(129));
    let _ = host.only().render();
    assert!(scroll(&mut host, c).offset().y > 0.0);
    assert!(body(&mut host, c).built().contains(&29));
}

#[test]
fn a_disabled_collection_ignores_input() {
    let (mut runner, mut host) = setup();
    runner.send(&mut host, Msg::Enabled(false));
    let _ = host.only().render();
    click_item(&mut runner, &mut host, 2, Modifiers::empty());
    assert_eq!(take_log(&mut runner), []);
    runner.send(&mut host, Msg::Enabled(true));
    click_item(&mut runner, &mut host, 2, Modifiers::empty());
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![102])]);
}

#[test]
fn assistive_technology_sees_a_grid_of_cells() {
    let (mut runner, mut host) = setup();
    let c = collection(&runner);
    click_item(&mut runner, &mut host, 5, Modifiers::empty());
    let body_id = body_id(&mut host, c);
    let _ = host.only().render();
    let grid = host.only().access_node(body_id).expect("the body");
    assert_eq!(grid.role(), Role::Grid);
    assert_eq!(grid.data().row_count(), Some(8), "30 items in 4 columns");
    assert_eq!(grid.data().column_count(), Some(4));
    assert!(grid.data().is_multiselectable());

    let cell = body(&mut host, c).row_id(5).expect("built");
    let node = host.only().access_node(cell).expect("the cell");
    assert_eq!(node.role(), Role::GridCell);
    assert_eq!(node.label().as_deref(), Some("Item 05"));
    assert_eq!(node.data().row_index(), Some(1));
    assert_eq!(node.data().column_index(), Some(1));
    assert_eq!(node.data().is_selected(), Some(true));
    let other = body(&mut host, c).row_id(6).expect("built");
    assert_eq!(
        host.only()
            .access_node(other)
            .expect("a cell")
            .is_selected(),
        Some(false)
    );

    // A click from assistive technology selects the cell.
    take_log(&mut runner);
    a11y_click(host.only(), other);
    deliver(&mut runner, &mut host);
    assert_eq!(take_log(&mut runner), [Msg::Selected(vec![106])]);
}

/// Two collections in a row that share its width, after a label that keeps its own.
#[derive(Default)]
struct Pair {
    first: Handle<Collection<Msg, u32>>,
    second: Handle<Collection<Msg, u32>>,
}

impl Application for Pair {
    type Message = Msg;

    fn start(&mut self, cx: &mut Cx<'_, Msg>) {
        let make = |count: u32| {
            Collection::new(100..100 + count, |&id| {
                Item::new(label(id)).icon(Glyph::Document)
            })
            .icon_size(ICON)
            .height(100.0)
        };
        let row = Stack::row()
            .spacing(0.0)
            .with(Label::new("Side"))
            .with_flex(make(30).bind(&mut self.first))
            .with_flex(make(5).bind(&mut self.second));
        cx.open_window(Window::new("Pair", (456, 120), row));
    }

    fn update(&mut self, _cx: &mut Cx<'_, Msg>, _message: Msg) {}
}

#[test]
fn flexible_children_share_what_the_others_leave() {
    let (mut runner, mut host, _) = start(Pair::default());
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    let (first, second) = (id(&runner.app().first), id(&runner.app().second));
    let width = |host: &mut FakeHost, c: WidgetId| {
        host.only().get_widget_with_id(c).ctx().border_box().width()
    };
    let side = host
        .only()
        .get_widget_with_id(first)
        .ctx()
        .to_window(Point::ORIGIN)
        .x;
    assert!(side > 0.0, "the label keeps its width");
    let room = 456.0 - side;
    assert!((width(&mut host, first) - room / 2.0).abs() < 1.0);
    assert!((width(&mut host, second) - room / 2.0).abs() < 1.0);
    let before = columns(&mut host, first);
    assert!(before >= 2, "several 72 px cells fit in half of the room");

    host.only()
        .render_root()
        .handle_window_event(WindowEvent::Resize(PhysicalSize::new(228, 120)));
    let _ = host.only().render();
    deliver(&mut runner, &mut host);
    assert!(width(&mut host, first) < 120.0, "narrower with the window");
    assert!(columns(&mut host, first) < before);
}
